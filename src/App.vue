<script setup lang="ts">
import { computed, defineAsyncComponent, onBeforeUnmount, onMounted, reactive, ref, watch } from "vue";
import { homeDir } from "@tauri-apps/api/path";
import { getCurrentWindow, UserAttentionType } from "@tauri-apps/api/window";
import TerminalView from "./components/TerminalView.vue";
import TabSidebar from "./components/TabSidebar.vue";
import StatusBar from "./components/StatusBar.vue";
import FileExplorer from "./components/FileExplorer.vue";
import EditorTabs from "./components/EditorTabs.vue";
import BranchPicker from "./components/BranchPicker.vue";
import McpMenu from "./components/McpMenu.vue";
import { openFile, showTerminal, shownFile } from "./state/editor";
import { reviewPrompt } from "./state/review";
import type { PathTarget } from "./lib/paths";

// O visualizador (CodeMirror) é pesado: só carrega quando abrir o primeiro arquivo.
const FileView = defineAsyncComponent(() => import("./components/FileView.vue"));
import {
  claudeHooksInstall,
  claudeHooksStatus,
  claudeMcpList,
  type ClaudeHooksStatus,
  type ClaudeState,
  type McpServer,
  type PtySession,
} from "./lib/pty";
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

// Pasta atual, processo e estado do Claude de cada aba (do terminal principal).
// Roda mesmo com a janela escondida: é aí que o aviso do Claude mais importa.
async function pollSessions() {
  for (const tab of store.tabs) {
    const session = sessions.get(tab.id);
    if (!session) continue;
    const info = await session.info().catch(() => null);
    if (!info) continue;
    if (info.cwd) tab.cwd = info.cwd;
    tab.process = info.process;
    if (info.claude !== tab.claude) onClaudeChange(tab, info.claude);
  }
}
const pollTimer = setInterval(pollSessions, 1000);

// ---- caminhos clicados no terminal: abrem numa aba da área principal ----
function openPath(tab: Tab, { path, line }: PathTarget) {
  // Dentro do repositório atual: abre relativo à raiz, com o status do git (alterações).
  const root = git.value?.root;
  const inRepo = root && path.startsWith(root + "/");
  const cut = path.lastIndexOf("/");
  openFile(
    inRepo
      ? { root, path: path.slice(root.length + 1), origPath: null, line }
      : { root: path.slice(0, cut) || "/", path: path.slice(cut + 1), origPath: null, line },
    tab,
  );
}

// ---- avisos do Claude ----
const windowFocused = ref(document.hasFocus());
const isVisible = (tab: Tab) => isActive(tab) && windowFocused.value;

function onClaudeChange(tab: Tab, state: ClaudeState | null) {
  tab.claude = state;
  if (state === "waiting" || state === "done") {
    if (!isVisible(tab)) tab.attention = state;
    // Janela em segundo plano: pede atenção (pisca no lançador/barra de tarefas).
    if (!windowFocused.value) {
      const type = state === "waiting" ? UserAttentionType.Critical : UserAttentionType.Informational;
      getCurrentWindow().requestUserAttention(type).catch(() => {});
    }
  } else {
    tab.attention = null;
  }
}

// Viu a aba: o aviso some.
function clearVisibleAttention() {
  const tab = activeTab.value;
  if (tab && isVisible(tab)) tab.attention = null;
}
watch(() => store.activeId, clearVisibleAttention);
const onWindowFocus = () => {
  windowFocused.value = true;
  clearVisibleAttention();
  getCurrentWindow().requestUserAttention(null).catch(() => {});
};
const onWindowBlur = () => (windowFocused.value = false);

// Hooks do Claude Code: confere quando a aba ativa roda o Claude e oferece instalar.
const claudeHooks = ref<ClaudeHooksStatus | null>(null);
const claudeHooksNote = ref<string | null>(null);
watch(
  () => [activeTab.value?.id, activeTab.value?.process] as const,
  async ([id, process]) => {
    const session = id !== undefined ? sessions.get(id) : undefined;
    if (process !== "claude" || !session) return;
    claudeHooks.value = await claudeHooksStatus(session.id).catch(() => null);
    // Primeira vez (ou dado velho) com o Claude nesta aba: verifica os MCPs em segundo plano.
    if (id !== undefined) refreshMcp(id, MCP_STALE_MS);
  },
);

// ---- servidores MCP do Claude da aba (só os do projeto) ----
interface McpState {
  servers: McpServer[] | null;
  loading: boolean;
  error: string | null;
  at: number | null;
}
// O comando conecta em cada servidor (alguns segundos): resultado guardado por aba.
const MCP_STALE_MS = 5 * 60_000;
const mcpByTab = reactive(new Map<number, McpState>());
const mcpOpen = ref(false);
const activeMcp = computed(() => (activeTab.value ? mcpByTab.get(activeTab.value.id) ?? null : null));

async function refreshMcp(tabId: number, maxAge = 0) {
  const session = sessions.get(tabId);
  const current = mcpByTab.get(tabId);
  if (!session || current?.loading) return;
  if (current?.at && Date.now() - current.at < maxAge) return;
  mcpByTab.set(tabId, { servers: current?.servers ?? null, loading: true, error: null, at: current?.at ?? null });
  try {
    const servers = await claudeMcpList(session.id);
    mcpByTab.set(tabId, { servers, loading: false, error: null, at: Date.now() });
  } catch (err) {
    mcpByTab.set(tabId, { servers: current?.servers ?? null, loading: false, error: String(err), at: current?.at ?? null });
  }
}

function toggleMcp() {
  mcpOpen.value = !mcpOpen.value;
  if (mcpOpen.value && activeTab.value) refreshMcp(activeTab.value.id, 60_000);
}

/** Login de MCP é no próprio Claude: digita /mcp no prompt (o Enter fica com o usuário). */
function authenticateMcp() {
  const session = activeTab.value && sessions.get(activeTab.value.id);
  session?.write("/mcp").catch(() => {});
  mcpOpen.value = false;
  if (activeTab.value) {
    activeTab.value.view = -1;
    activeTab.value.focusedPane = "main";
  }
}
// Fechou o Claude ou trocou de aba: o menu não faz mais sentido.
watch(
  () => [activeTab.value?.id, activeTab.value?.process],
  () => activeTab.value?.process !== "claude" && (mcpOpen.value = false),
);

async function installClaudeHooks() {
  const session = activeTab.value && sessions.get(activeTab.value.id);
  if (!session) return;
  try {
    claudeHooks.value = await claudeHooksInstall(session.id);
    claudeHooksNote.value = "Avisos ativados: valem pras próximas sessões do Claude (reinicie a atual com /exit).";
  } catch (err) {
    claudeHooksNote.value = `Não deu pra ativar os avisos: ${err}`;
  }
  setTimeout(() => (claudeHooksNote.value = null), 8000);
}

// ---- revisão: comentários de arquivos mandados pro Claude da aba ----
async function sendReview(tab: Tab) {
  const session = sessions.get(tab.id);
  const fail = (msg: string) => {
    claudeHooksNote.value = msg;
    setTimeout(() => (claudeHooksNote.value = null), 8000);
  };
  if (!session || tab.process !== "claude") return fail("Abra o Claude no terminal desta aba pra mandar a revisão.");
  // Colagem "entre colchetes": o Claude recebe como texto colado e não envia sozinho.
  const text = reviewPrompt(tab).replace(/\x1b/g, "");
  try {
    await session.write(`\x1b[200~${text}\x1b[201~`);
  } catch (err) {
    return fail(`Não deu pra mandar a revisão: ${err}`);
  }
  tab.review.splice(0);
  showTerminal(tab);
  tab.focusedPane = "main";
}

// ---- git / painel de diff ----
const { status: git, refresh: refreshGit } = useGitStatus(computed(() => activeTab.value?.cwd ?? null));
const diffOpen = ref(false);
const pickerOpen = ref(false);

const { repo, github, refresh: refreshGithub } = useGithub(
  computed(() => git.value?.root ?? null),
  computed(() => git.value?.branch ?? null),
  computed(() => git.value?.ahead ?? 0),
);

// PR da aba mergeado: marca a aba com a tag close/merge. Uma vez por PR, pra
// não brigar com quem tirou a tag na mão depois.
const mergeTagged = new Map<number, string>();
watch(github, (status) => {
  const tab = activeTab.value;
  if (!tab || status?.kind !== "pr" || status.pr.state !== "MERGED") return;
  if (mergeTagged.get(tab.id) === status.pr.url) return;
  mergeTagged.set(tab.id, status.pr.url);
  if (tab.tag !== "merge") tab.tag = "merge";
});

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

onMounted(() => {
  window.addEventListener("keydown", onKey, true);
  window.addEventListener("focus", onWindowFocus);
  window.addEventListener("blur", onWindowBlur);
});
onBeforeUnmount(() => {
  window.removeEventListener("keydown", onKey, true);
  window.removeEventListener("focus", onWindowFocus);
  window.removeEventListener("blur", onWindowBlur);
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
          <div class="pane main" :style="{ flexGrow: tab.panelOpen ? 1 - tab.panelHeight : 1 }">
            <EditorTabs v-if="tab.files.length || tab.review.length" :tab="tab" :home="home" :status="git" @send-review="sendReview(tab)" />
            <div class="layers">
              <div class="layer" :class="{ shown: tab.view === -1 }">
                <TerminalView
                  :cwd="tab.initialCwd"
                  :focused="isActive(tab) && tab.focusedPane === 'main' && tab.view === -1"
                  @focus="tab.focusedPane = 'main'"
                  @spawned="sessions.set(tab.id, $event)"
                  @open-path="openPath(tab, $event)"
                  @exit="onExit(tab.id)"
                />
              </div>
              <!-- Arquivos ficam montados (escondidos) pra manter a rolagem ao trocar de aba. -->
              <div
                v-for="(file, i) in tab.files"
                :key="file.root + '/' + file.path"
                class="layer"
                :class="{ shown: tab.view === i }"
                @mousedown="tab.focusedPane = 'main'"
              >
                <FileView :tab="tab" :file="file" :status="git" />
              </div>
            </div>
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
                  @open-path="openPath(tab, $event)"
                  @exit="panelSessions.delete(tab.id), closePanel(tab.id)"
                />
              </div>
            </div>
          </template>
        </div>
      </div>

      <template v-if="diffOpen && git">
        <div class="resizer" @pointerdown.prevent="resizeDiff" />
        <FileExplorer
          :status="git"
          :active-path="(activeTab && shownFile(activeTab)?.path) ?? null"
          :style="{ width: diffWidth + 'px' }"
          @close="diffOpen = false"
          @refresh="refreshGit"
        />
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
      :claude-hooks-missing="activeTab?.process === 'claude' && claudeHooks?.installed === false"
      :claude-hooks-path="claudeHooks?.settingsPath ?? null"
      :note="claudeHooksNote"
      :mcp="activeTab?.process === 'claude' ? activeMcp : null"
      :mcp-available="activeTab?.process === 'claude'"
      :mcp-open="mcpOpen"
      @toggle-diff="diffOpen = !diffOpen"
      @toggle-panel="togglePanel"
      @pick-branch="pickerOpen = !pickerOpen"
      @github-login="githubLogin"
      @install-claude-hooks="installClaudeHooks"
      @toggle-mcp="toggleMcp"
    />

    <McpMenu
      v-if="mcpOpen && activeTab"
      :servers="activeMcp?.servers ?? null"
      :loading="!!activeMcp?.loading"
      :error="activeMcp?.error ?? null"
      :config-dir="claudeHooks?.settingsPath.replace(/\/settings\.json$/, '').replace(home ?? '\0', '~') ?? null"
      :checked-at="activeMcp?.at ?? null"
      @close="mcpOpen = false"
      @refresh="activeTab && refreshMcp(activeTab.id)"
      @authenticate="authenticateMcp"
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
.pane.main {
  display: flex;
  flex-direction: column;
}
/* Terminal e arquivos empilhados: só o da aba escolhida aparece. */
.layers {
  position: relative;
  flex: 1;
  min-height: 0;
}
.layer {
  position: absolute;
  inset: 0;
  visibility: hidden;
}
.slot.active .layer.shown {
  visibility: visible;
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
