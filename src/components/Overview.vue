<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, reactive, ref } from "vue";
import { claudeState } from "../lib/api";
import { accountLabel, childAgents, claudePeek, elapsed, sections, STATE_LABEL, stateSince, type CardState, type Peek } from "../lib/overview";
import { claudeAgents, type Agent } from "../lib/pty";
import type { Group, Tab } from "../state/tabs";
import OverviewPopup from "./OverviewPopup.vue";

/** Visão geral: um card por aba com Claude, por grupo, com os subagentes pendurados. */
const props = defineProps<{
  tabs: Tab[];
  groups: Group[];
  title: (tab: Tab) => string;
  ptyOf: (tab: Tab) => number | undefined;
  voice: "idle" | "recording" | "transcribing" | "speaking";
  voiceSecs: number;
}>();
const emit = defineEmits<{ close: []; go: [tabId: number]; send: [tabId: number, text: string]; voice: [] }>();

const root = ref<HTMLDivElement>();
const peeks = reactive(new Map<number, Peek | null>());
const agents = reactive(new Map<number, Agent[]>());
const selected = ref<number | null>(null);
const now = ref(Date.now());

const view = computed(() => sections(props.tabs, props.groups));
const selectedTab = computed(() => props.tabs.find((t) => t.id === selected.value) ?? null);
// Só vale mostrar a conta quando tem Claude de mais de uma.
const manyAccounts = computed(() => new Set([...peeks.values()].map((p) => p?.config_dir ?? null)).size > 1);

const state = (tab: Tab) => claudeState(tab) as CardState;

function since(tab: Tab) {
  const at = stateSince(tab.id);
  return at === null ? "" : ` há ${elapsed(now.value - at)}`;
}

function activityLine(tab: Tab) {
  const a = peeks.get(tab.id)?.activity;
  if (!a) return "";
  return a.kind === "text" ? a.text.split("\n").find((l) => l.trim()) ?? "" : `${a.name}${a.detail ? ` · ${a.detail}` : ""}`;
}

let busy = false;
async function poll() {
  if (busy) return;
  busy = true;
  try {
    await Promise.all(
      view.value.sections
        .flatMap((s) => s.tabs)
        .map(async (tab) => {
          const pty = props.ptyOf(tab);
          if (pty === undefined) return;
          const [peek, list] = await Promise.all([claudePeek(pty).catch(() => null), claudeAgents(pty).catch(() => [])]);
          peeks.set(tab.id, peek);
          agents.set(tab.id, list);
        }),
    );
  } finally {
    busy = false;
  }
}

// Ritmo de polling só enquanto a tela está aberta: fechada, nada roda.
const pollTimer = setInterval(poll, 2500);
const clock = setInterval(() => (now.value = Date.now()), 1000);

function onKey(e: KeyboardEvent) {
  if (e.key !== "Escape") return;
  e.preventDefault();
  e.stopPropagation();
  if (selected.value !== null) selected.value = null;
  else emit("close");
}

onMounted(() => {
  poll();
  window.addEventListener("keydown", onKey, true);
  root.value?.focus();
});
onBeforeUnmount(() => {
  clearInterval(pollTimer);
  clearInterval(clock);
  window.removeEventListener("keydown", onKey, true);
});
</script>

<template>
  <div ref="root" class="overview" tabindex="-1" role="dialog" aria-label="Visão geral das abas">
    <header @mousedown.prevent>
      <span class="title">Visão geral</span>
      <span class="muted">
        {{ view.sections.reduce((n, s) => n + s.tabs.length, 0) }} com Claude<template v-if="view.others"> · +{{ view.others }} {{ view.others === 1 ? "terminal" : "terminais" }}</template>
      </span>
      <button class="close" title="Fechar (Esc ou Ctrl+Shift+O)" @click="emit('close')">
        <svg viewBox="0 0 16 16" width="13" height="13"><path d="M4 4l8 8M12 4l-8 8" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" /></svg>
      </button>
    </header>

    <div class="scroll">
      <p v-if="!view.sections.length" class="empty">Nenhuma aba com o Claude aberto agora.</p>
      <section
        v-for="s in view.sections"
        :key="s.group?.id ?? 'soltas'"
        :style="{ '--group': s.group?.color ?? 'var(--border)' }"
      >
        <h3>{{ s.group?.name ?? "Sem grupo" }}</h3>
        <div class="cards">
          <div v-for="tab in s.tabs" :key="tab.id" class="node">
            <button class="card" :class="state(tab)" :style="tab.color ? { '--tab': tab.color } : {}" @click="selected = tab.id">
              <span class="name">{{ title(tab) }}</span>
              <span class="state">
                <span class="dot" />{{ STATE_LABEL[state(tab)] }}{{ since(tab) }}
                <span v-if="manyAccounts && accountLabel(peeks.get(tab.id)?.config_dir ?? null)" class="account">
                  {{ accountLabel(peeks.get(tab.id)?.config_dir ?? null) }}
                </span>
              </span>
              <span class="activity" :title="activityLine(tab)">{{ activityLine(tab) || " " }}</span>
            </button>
            <ul v-if="childAgents(agents.get(tab.id) ?? []).length" class="children">
              <li v-for="a in childAgents(agents.get(tab.id) ?? [])" :key="a.id" :class="a.status" :title="a.description">
                <span class="dot" />{{ a.description || a.kind }}
              </li>
            </ul>
          </div>
        </div>
      </section>
    </div>

    <OverviewPopup
      v-if="selectedTab"
      :key="selectedTab.id"
      :tab="selectedTab"
      :title="title(selectedTab)"
      :pty="ptyOf(selectedTab)"
      :peek="peeks.get(selectedTab.id) ?? null"
      :agents="agents.get(selectedTab.id) ?? []"
      :since="since(selectedTab)"
      @close="selected = null"
      @go="emit('go', selectedTab.id)"
      @send="(text) => emit('send', selectedTab!.id, text)"
      :voice="voice"
      :voice-secs="voiceSecs"
      @voice="emit('voice')"
    />
  </div>
</template>

<style scoped>
.overview {
  position: fixed;
  inset: 0;
  z-index: 150;
  display: flex;
  flex-direction: column;
  background: color-mix(in srgb, var(--bg) 96%, transparent);
  outline: none;
}
header {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 12px 20px;
  border-bottom: 1px solid var(--border);
}
.title {
  font-weight: 600;
}
.muted,
.empty {
  color: var(--fg-muted);
  font-size: 12px;
}
.close {
  margin-left: auto;
  display: grid;
  place-items: center;
  width: 26px;
  height: 26px;
  border: 0;
  border-radius: 6px;
  background: none;
  color: var(--fg-muted);
  cursor: pointer;
}
.close:hover {
  background: var(--hover);
  color: var(--fg);
}
.scroll {
  flex: 1;
  overflow: auto;
  padding: 16px 20px 32px;
}
.empty {
  margin-top: 40px;
  text-align: center;
}
section {
  margin-bottom: 18px;
  padding: 10px 14px 14px;
  border: 1px solid var(--border);
  border-left: 3px solid var(--group);
  border-radius: 8px;
}
h3 {
  margin: 0 0 10px;
  font-size: 12px;
  font-weight: 600;
  color: var(--fg-muted);
}
.cards {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(230px, 1fr));
  gap: 14px;
  align-items: start;
}
.card {
  --tone: var(--fg-muted);
  display: flex;
  flex-direction: column;
  gap: 4px;
  width: 100%;
  padding: 9px 11px;
  border: 1px solid var(--border);
  border-top: 2px solid var(--tab, var(--border));
  border-radius: 7px;
  background: var(--bg-elevated);
  color: var(--fg);
  font: inherit;
  text-align: left;
  cursor: pointer;
}
.card:hover,
.card:focus-visible {
  border-color: var(--tone);
  outline: none;
}
.card.working {
  --tone: var(--claude);
}
.card.waiting {
  --tone: var(--yellow);
  animation: pulse 1.6s ease-in-out infinite;
}
.card.done {
  --tone: var(--green);
}
@keyframes pulse {
  50% {
    box-shadow: 0 0 0 3px color-mix(in srgb, var(--yellow) 30%, transparent);
  }
}
.name {
  overflow: hidden;
  font-weight: 600;
  white-space: nowrap;
  text-overflow: ellipsis;
}
.state {
  display: flex;
  align-items: center;
  gap: 6px;
  color: var(--tone);
  font-size: 12px;
}
.dot {
  flex: none;
  width: 7px;
  height: 7px;
  border-radius: 50%;
  background: currentColor;
}
.working .state .dot,
.children .running .dot {
  animation: blink 1s ease-in-out infinite;
}
@keyframes blink {
  50% {
    opacity: 0.3;
  }
}
.account {
  margin-left: auto;
  padding: 0 6px;
  border: 1px solid var(--border);
  border-radius: 9px;
  color: var(--fg-muted);
  font-size: 11px;
}
.activity {
  overflow: hidden;
  color: var(--fg-muted);
  font-size: 12px;
  white-space: nowrap;
  text-overflow: ellipsis;
}
/* Subagentes como filhos: uma linha descendo do card e um traço até cada um. */
.children {
  margin: 0 0 0 14px;
  padding: 6px 0 0 0;
  list-style: none;
  border-left: 1px solid var(--border);
}
.children li {
  position: relative;
  display: flex;
  align-items: center;
  gap: 6px;
  margin: 4px 0 0 12px;
  overflow: hidden;
  color: var(--fg-muted);
  font-size: 12px;
  white-space: nowrap;
  text-overflow: ellipsis;
}
.children li::before {
  content: "";
  position: absolute;
  left: -12px;
  width: 9px;
  border-top: 1px solid var(--border);
}
.children .running {
  color: var(--claude);
}
.children .done .dot {
  background: var(--green);
}
.children .stalled .dot {
  background: var(--red);
}
</style>
