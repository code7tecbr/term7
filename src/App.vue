<script setup lang="ts">
import { computed, defineAsyncComponent, onBeforeUnmount, onMounted, ref, watch } from "vue";
import { homeDir } from "@tauri-apps/api/path";
import { getCurrentWindow } from "@tauri-apps/api/window";
import TerminalView from "./components/TerminalView.vue";
import TabSidebar from "./components/TabSidebar.vue";
import StatusBar from "./components/StatusBar.vue";
import DiffPanel from "./components/DiffPanel.vue";
import BranchPicker from "./components/BranchPicker.vue";
import { viewer } from "./state/viewer";

// O visualizador (CodeMirror) é pesado: só carrega quando abrir o primeiro arquivo.
const FileViewer = defineAsyncComponent(() => import("./components/FileViewer.vue"));
import type { PtySession } from "./lib/pty";
import { matchShortcut, type Action } from "./lib/shortcuts";
import { useGitStatus } from "./lib/useGitStatus";
import { useGithub } from "./lib/useGithub";
import { restoreSession, saveSession, snapshot } from "./state/session";
import {
  activeTab,
  closePanel,
  closeTab,
  cycleTab,
  goToTab,
  newTab,
  openPanel,
  store,
  tabTitle,
  togglePanel,
  type Tab,
} from "./state/tabs";

const sessions = new Map<number, PtySession>();
const home = ref<string | null>(null);
homeDir()
  .then((h) => (home.value = h.replace(/\/$/, "")))
  .catch(() => {});

// ---- sessão: restaura ao abrir, salva a cada mudança ----
const ready = ref(false);
let closing = false;

restoreSession().then((restored) => {
  if (!restored) newTab();
  ready.value = true;
});

let saveTimer: ReturnType<typeof setTimeout> | undefined;
watch(snapshot, (data) => {
  if (!ready.value || closing) return;
  clearTimeout(saveTimer);
  saveTimer = setTimeout(() => saveSession(data).catch(() => {}), 400);
});

// Salva uma última vez e congela: os shells morrendo no fechamento não podem
// apagar as abas da sessão salva.
getCurrentWindow().onCloseRequested(async () => {
  clearTimeout(saveTimer);
  await saveSession().catch(() => {});
  closing = true;
});

function onExit(tabId: number) {
  sessions.delete(tabId);
  closeTab(tabId);
}

// Fechou a última aba: fecha a janela.
watch(
  () => store.tabs.length,
  (n) => ready.value && n === 0 && getCurrentWindow().close(),
);

watch(
  () => activeTab.value && tabTitle(activeTab.value, home.value),
  (title) => getCurrentWindow().setTitle(title ? `${title} — term7` : "term7").catch(() => {}),
);

// Pasta atual e processo de cada aba (do terminal principal).
async function pollSessions() {
  if (document.hidden) return;
  for (const tab of store.tabs) {
    const session = sessions.get(tab.id);
    if (!session) continue;
    const info = await session.info().catch(() => null);
    if (!info) continue;
    if (info.cwd) tab.cwd = info.cwd;
    tab.process = info.process;
  }
}
const pollTimer = setInterval(pollSessions, 1000);

// ---- git / painel de diff ----
const { status: git, refresh: refreshGit } = useGitStatus(computed(() => activeTab.value?.cwd ?? null));
const diffOpen = ref(false);
const pickerOpen = ref(false);

const { repo, github, refresh: refreshGithub } = useGithub(
  computed(() => git.value?.root ?? null),
  computed(() => git.value?.branch ?? null),
  computed(() => git.value?.ahead ?? 0),
);

// ---- terminal inferior: escrever um comando nele (ex.: gh auth login) ----
const panelSessions = new Map<number, PtySession>();
const panelInput = new Map<number, string>();

function onPanelSpawned(tabId: number, session: PtySession) {
  panelSessions.set(tabId, session);
  const text = panelInput.get(tabId);
  if (text) {
    panelInput.delete(tabId);
    session.write(text);
  }
}

/** Abre o terminal inferior com o comando digitado (sem Enter: quem confirma é o usuário). */
function typeInPanel(text: string) {
  // Painel recém-criado: o shell ainda não subiu, então guarda até o spawn.
  const wasAlive = !!activeTab.value?.panelAlive;
  const tab = openPanel();
  if (!tab) return;
  const session = wasAlive ? panelSessions.get(tab.id) : undefined;
  if (session) session.write(text).catch(() => {});
  else panelInput.set(tab.id, text);
}

function githubLogin() {
  typeInPanel("gh auth login");
  // Depois do login o PR aparece sozinho na próxima consulta; ao voltar pra janela, consulta já.
  window.addEventListener("focus", refreshGithub, { once: true });
}

// ---- larguras/alturas ajustáveis ----
function readNumber(key: string, fallback: number) {
  try {
    return Number(localStorage.getItem(key)) || fallback;
  } catch {
    return fallback;
  }
}

function writeNumber(key: string, value: number) {
  try {
    localStorage.setItem(key, String(value));
  } catch {
    // sem storage, só não lembra o tamanho
  }
}

const sidebarWidth = ref(readNumber("term7.sidebarWidth", 230));
const diffWidth = ref(readNumber("term7.diffWidth", 460));
const viewerWidth = ref(readNumber("term7.viewerWidth", 560));
const clamp = (v: number, min: number, max: number) => Math.round(Math.min(Math.max(v, min), max));

/** Arrasto genérico: chama onMove com a posição do ponteiro e onEnd ao soltar. */
function drag(e: PointerEvent, onMove: (ev: PointerEvent) => void, onEnd?: () => void) {
  const handle = e.currentTarget as HTMLElement;
  handle.setPointerCapture(e.pointerId);
  handle.addEventListener("pointermove", onMove);
  handle.addEventListener(
    "pointerup",
    () => {
      handle.removeEventListener("pointermove", onMove);
      onEnd?.();
    },
    { once: true },
  );
}

const resizeSidebar = (e: PointerEvent) =>
  drag(
    e,
    (ev) => (sidebarWidth.value = clamp(ev.clientX, 160, 420)),
    () => writeNumber("term7.sidebarWidth", sidebarWidth.value),
  );

const resizeDiff = (e: PointerEvent) =>
  drag(
    e,
    (ev) => (diffWidth.value = clamp(window.innerWidth - ev.clientX, 280, window.innerWidth * 0.7)),
    () => writeNumber("term7.diffWidth", diffWidth.value),
  );

// O visualizador fica entre o terminal e o painel de diff.
function resizeViewer(e: PointerEvent) {
  const right = (diffOpen.value && git.value ? diffWidth.value : 0) + 4;
  drag(
    e,
    (ev) => (viewerWidth.value = clamp(window.innerWidth - right - ev.clientX, 320, window.innerWidth * 0.7)),
    () => writeNumber("term7.viewerWidth", viewerWidth.value),
  );
}

function resizePanel(e: PointerEvent, tab: Tab) {
  const slot = (e.currentTarget as HTMLElement).parentElement!.getBoundingClientRect();
  drag(e, (ev) => {
    tab.panelHeight = Math.min(Math.max((slot.bottom - ev.clientY) / slot.height, 0.12), 0.88);
  });
}

// ---- atalhos ----
function run(action: Action) {
  switch (action.type) {
    case "newTab":
      return newTab();
    case "closeTab":
      return store.activeId !== null && closeTab(store.activeId);
    case "nextTab":
      return cycleTab(1);
    case "prevTab":
      return cycleTab(-1);
    case "goTo":
      return goToTab(action.index);
    case "toggleDiff":
      diffOpen.value = !diffOpen.value;
      return;
    case "togglePanel":
      return togglePanel();
  }
}

function onKey(e: KeyboardEvent) {
  const action = matchShortcut(e);
  if (!action) return;
  e.preventDefault();
  e.stopPropagation();
  run(action);
}

onMounted(() => window.addEventListener("keydown", onKey, true));
onBeforeUnmount(() => {
  window.removeEventListener("keydown", onKey, true);
  clearInterval(pollTimer);
});

const isActive = (tab: Tab) => tab.id === store.activeId;
</script>

<template>
  <div class="app">
    <div class="body">
      <TabSidebar :home="home" :style="{ width: sidebarWidth + 'px' }" />
      <div class="resizer" @pointerdown.prevent="resizeSidebar" />

      <div class="terminals">
        <!-- Abas inativas ficam escondidas (não desmontadas) pra manter o shell e o scrollback. -->
        <div v-for="tab in store.tabs" :key="tab.id" class="slot" :class="{ active: isActive(tab) }">
          <div class="pane" :style="{ flexGrow: tab.panelOpen ? 1 - tab.panelHeight : 1 }">
            <TerminalView
              :cwd="tab.initialCwd"
              :focused="isActive(tab) && tab.focusedPane === 'main'"
              @focus="tab.focusedPane = 'main'"
              @spawned="sessions.set(tab.id, $event)"
              @exit="onExit(tab.id)"
            />
          </div>

          <template v-if="tab.panelAlive">
            <div v-show="tab.panelOpen" class="hsplit" @pointerdown.prevent="resizePanel($event, tab)" />
            <div v-show="tab.panelOpen" class="pane bottom" :style="{ flexGrow: tab.panelHeight }">
              <div class="panel-header" @mousedown.prevent>
                <span>Terminal</span>
                <div class="panel-actions">
                  <button title="Esconder (Ctrl+')" @click="(tab.panelOpen = false), (tab.focusedPane = 'main')">
                    <svg viewBox="0 0 16 16" width="13" height="13"><path d="M4 6l4 4 4-4" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" /></svg>
                  </button>
                  <button title="Fechar terminal" @click="closePanel(tab.id)">
                    <svg viewBox="0 0 16 16" width="13" height="13"><path d="M4 4l8 8M12 4l-8 8" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" /></svg>
                  </button>
                </div>
              </div>
              <div class="panel-term">
                <TerminalView
                  :cwd="tab.panelCwd"
                  :focused="isActive(tab) && tab.panelOpen && tab.focusedPane === 'panel'"
                  @focus="tab.focusedPane = 'panel'"
                  @spawned="onPanelSpawned(tab.id, $event)"
                  @exit="panelSessions.delete(tab.id), closePanel(tab.id)"
                />
              </div>
            </div>
          </template>
        </div>
      </div>

      <template v-if="viewer.files.length">
        <div class="resizer" @pointerdown.prevent="resizeViewer" />
        <FileViewer :status="git" :style="{ width: viewerWidth + 'px', flexShrink: 0 }" />
      </template>

      <template v-if="diffOpen && git">
        <div class="resizer" @pointerdown.prevent="resizeDiff" />
        <DiffPanel :status="git" :style="{ width: diffWidth + 'px' }" @close="diffOpen = false" @refresh="refreshGit" />
      </template>
    </div>

    <StatusBar
      :cwd="activeTab?.cwd ?? null"
      :home="home"
      :git="git"
      :repo="repo"
      :github="github"
      :diff-open="diffOpen && !!git"
      :panel-open="!!activeTab?.panelOpen"
      :picker-open="pickerOpen"
      @toggle-diff="diffOpen = !diffOpen"
      @toggle-panel="togglePanel"
      @pick-branch="pickerOpen = !pickerOpen"
      @github-login="githubLogin"
    />

    <BranchPicker
      v-if="pickerOpen && git"
      :root="git.root"
      :current="git.branch"
      @close="pickerOpen = false"
      @switched="refreshGit"
    />
  </div>
</template>

<style scoped>
.app {
  display: flex;
  flex-direction: column;
  height: 100%;
}
.body {
  display: flex;
  flex: 1;
  min-height: 0;
}
.body > aside {
  flex-shrink: 0;
}
.terminals {
  position: relative;
  flex: 1;
  min-width: 0;
}
/* visibility em vez de display:none: o terminal escondido mantém o tamanho
   e não recalcula colunas/linhas ao voltar. */
.slot {
  position: absolute;
  inset: 0;
  display: flex;
  flex-direction: column;
  visibility: hidden;
}
.slot.active {
  visibility: visible;
}
.pane {
  flex-basis: 0;
  min-height: 0;
}
.pane.bottom {
  display: flex;
  flex-direction: column;
}
.panel-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  height: 26px;
  padding: 0 6px 0 12px;
  flex-shrink: 0;
  color: var(--fg-muted);
  font-size: 11px;
  font-weight: 600;
  letter-spacing: 0.06em;
  text-transform: uppercase;
  user-select: none;
}
.panel-actions {
  display: flex;
  gap: 2px;
}
.panel-actions button {
  display: grid;
  place-items: center;
  width: 22px;
  height: 22px;
  border: 0;
  border-radius: 4px;
  background: none;
  color: inherit;
  cursor: pointer;
}
.panel-actions button:hover {
  background: var(--hover);
  color: var(--fg);
}
.panel-term {
  flex: 1;
  min-height: 0;
}
.resizer {
  flex-shrink: 0;
  width: 4px;
  margin: 0 -2px;
  border-left: 1px solid var(--border);
  cursor: col-resize;
  z-index: 1;
}
.resizer:hover {
  background: var(--accent);
}
.hsplit {
  flex-shrink: 0;
  height: 4px;
  margin: -2px 0;
  border-top: 1px solid var(--border);
  cursor: row-resize;
  z-index: 1;
}
.hsplit:hover {
  background: var(--accent);
}
</style>
