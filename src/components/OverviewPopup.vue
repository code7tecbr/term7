<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref } from "vue";
import { canDeliver, claudeState } from "../lib/api";
import { accountLabel, sortAgents, STATE_LABEL, type CardState, type Peek } from "../lib/overview";
import type { Agent } from "../lib/pty";
import { readScreen } from "../lib/screens";
import { dictationSink } from "../state/dictation";
import type { Tab } from "../state/tabs";

/** Card aberto na visão geral: a tela do terminal ao vivo, só pra ler, e um atalho pra aba. */
const props = defineProps<{
  tab: Tab;
  title: string;
  pty: number | undefined;
  peek: Peek | null;
  agents: Agent[];
  since: string;
  voice: "idle" | "recording" | "transcribing" | "speaking";
  voiceSecs: number;
}>();
const emit = defineEmits<{ close: []; go: []; send: [text: string]; voice: [] }>();

const SCREEN_LINES = 80;
const lines = ref<string[]>([]);
const screen = ref<HTMLPreElement>();
const reply = ref("");
/** Lista de agentes fechada por padrão, como o menu da barra de cima. */
const agentsOpen = ref(false);
const running = computed(() => props.agents.filter((a) => a.status === "running").length);
const AGENT_LABEL = { running: "rodando", done: "terminou", stalled: "parado" } as const;

const state = computed(() => claudeState(props.tab) as CardState);
// Esperando permissão, o texto cairia no menu do pedido: aí só indo pra aba.
const canReply = computed(() => canDeliver(state.value));
const account = computed(() => accountLabel(props.peek?.config_dir ?? null));
const clock = computed(() => `${Math.floor(props.voiceSecs / 60)}:${String(props.voiceSecs % 60).padStart(2, "0")}`);
const placeholder = computed(() =>
  canReply.value
    ? "Responder pro Claude desta aba…"
    : state.value === "waiting"
      ? "Pedido de permissão aberto: responda na aba"
      : "O Claude está trabalhando: dá pra escrever e mandar quando ele parar",
);

async function refresh() {
  if (props.pty === undefined) return;
  const next = readScreen(props.pty, SCREEN_LINES);
  if (!next || next.join("\n") === lines.value.join("\n")) return;
  const el = screen.value;
  // Só acompanha o fim se o usuário não rolou pra cima pra ler.
  const atEnd = !el || el.scrollHeight - el.scrollTop - el.clientHeight < 24;
  lines.value = next;
  if (atEnd) {
    await nextTick();
    el?.scrollTo({ top: el.scrollHeight });
  }
}
const timer = setInterval(refresh, 500);

function send() {
  const text = reply.value.trim();
  if (!text || !canReply.value) return;
  emit("send", text);
  reply.value = "";
}

// Ditado com o popup aberto vem pra este campo; "term7 pronto" no fim manda.
const sink = {
  tabId: props.tab.id,
  put(text: string, send_: boolean) {
    if (text) reply.value = reply.value.trim() ? `${reply.value.trimEnd()} ${text}` : text;
    if (send_) send();
  },
};

onMounted(() => {
  refresh();
  dictationSink.value = sink;
});
onBeforeUnmount(() => {
  clearInterval(timer);
  if (dictationSink.value === sink) dictationSink.value = null;
});
</script>

<template>
  <div class="backdrop" @mousedown.self="emit('close')">
    <div class="popup" role="dialog" :aria-label="title">
      <header @mousedown.prevent>
        <span class="name">{{ title }}</span>
        <span v-if="account" class="account" title="Conta do Claude (pasta de config)">{{ account }}</span>
        <span class="state" :class="state">{{ STATE_LABEL[state] }}{{ since }}</span>
        <button
          v-if="agents.length"
          class="agents-toggle"
          :class="{ open: agentsOpen }"
          title="Subagentes do Claude desta aba"
          @click="agentsOpen = !agentsOpen"
        >
          <span>Agentes</span>
          <span v-if="running" class="running"><span class="dot" />{{ running }} rodando</span>
          <span v-else class="muted">{{ agents.length }}</span>
        </button>
        <button class="go" title="Fecha a visão geral e abre esta aba" @click="emit('go')">Ir pra aba</button>
        <button class="close" title="Fechar (Esc)" @click="emit('close')">
          <svg viewBox="0 0 16 16" width="13" height="13"><path d="M4 4l8 8M12 4l-8 8" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" /></svg>
        </button>
      </header>

      <pre ref="screen" class="screen">{{ lines.join("\n") || "A tela desta aba ainda não está disponível." }}</pre>

      <ul v-if="agentsOpen && agents.length" class="agents">
        <li v-for="a in sortAgents(agents)" :key="a.id" class="agent" :class="a.status" :title="a.description">
          <span class="dot" /><span class="desc">{{ a.description || a.kind }}</span>
          <span class="muted">{{ a.kind }} · {{ AGENT_LABEL[a.status] }}<template v-if="a.status === 'running' && a.last_tool"> · usando {{ a.last_tool }}</template></span>
        </li>
      </ul>

      <footer>
        <button
          class="icon mic"
          :class="voice"
          :disabled="voice === 'transcribing'"
          :title="voice === 'recording' ? 'Parar e transcrever' : 'Ditar (Ctrl+Espaço): o texto vem pra este campo; termine com “term7 pronto” pra mandar'"
          @mousedown.prevent
          @click="emit('voice')"
        >
          <svg viewBox="0 0 16 16" width="14" height="14"><rect x="5.5" y="1.75" width="5" height="8" rx="2.5" fill="none" stroke="currentColor" stroke-width="1.3" /><path d="M3.5 7.5a4.5 4.5 0 0 0 9 0M8 12v2.25" fill="none" stroke="currentColor" stroke-width="1.3" stroke-linecap="round" /></svg>
          <span v-if="voice === 'recording'">{{ clock }}</span>
          <span v-else-if="voice === 'transcribing'">Transcrevendo…</span>
        </button>
        <input v-model="reply" :placeholder="placeholder" @keydown.enter.prevent="send" />
        <button
          class="icon send"
          :disabled="!canReply || !reply.trim()"
          :title="canReply ? 'Mandar (Enter)' : 'Espere o Claude parar pra mandar'"
          @mousedown.prevent
          @click="send"
        >
          <svg viewBox="0 0 16 16" width="14" height="14"><path d="M2.5 8h10M8.5 3.5 13 8l-4.5 4.5" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" /></svg>
        </button>
      </footer>
    </div>
  </div>
</template>

<style scoped>
.backdrop {
  position: fixed;
  inset: 0;
  z-index: 160;
  display: grid;
  place-items: center;
  background: rgb(0 0 0 / 0.45);
}
.popup {
  display: flex;
  flex-direction: column;
  width: min(1000px, calc(100vw - 48px));
  height: min(82vh, calc(100vh - 48px));
  border: 1px solid var(--border);
  border-radius: 10px;
  background: var(--bg-elevated);
  overflow: hidden;
}
header {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 10px 14px;
  border-bottom: 1px solid var(--border);
}
.name {
  font-weight: 600;
}
.account {
  padding: 0 6px;
  border: 1px solid var(--border);
  border-radius: 9px;
  color: var(--fg-muted);
  font-size: 11px;
}
.state {
  margin-right: auto;
  color: var(--fg-muted);
  font-size: 12px;
}
.state.working {
  color: var(--claude);
}
.state.waiting {
  color: var(--yellow);
}
.state.done {
  color: var(--green);
}
.close {
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
.screen {
  flex: 1;
  margin: 0;
  padding: 10px 14px;
  overflow: auto;
  background: var(--bg);
  color: var(--fg);
  font-family: var(--font-mono);
  font-size: 12.5px;
  line-height: 1.35;
  white-space: pre;
}
.agents-toggle {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 4px 10px;
  border: 1px solid var(--border);
  border-radius: 6px;
  background: none;
  color: var(--fg);
  font: inherit;
  font-size: 12px;
  cursor: pointer;
}
.agents-toggle:hover,
.agents-toggle.open {
  background: var(--hover);
}
.running {
  display: flex;
  align-items: center;
  gap: 5px;
  color: var(--claude);
}
.running .dot {
  background: var(--claude);
  animation: blink 1s ease-in-out infinite;
}
@keyframes blink {
  50% {
    opacity: 0.3;
  }
}
.muted {
  color: var(--fg-muted);
}
.agents {
  max-height: 30%;
  margin: 0;
  padding: 6px 14px;
  overflow: auto;
  list-style: none;
  border-top: 1px solid var(--border);
  font-size: 12px;
}
.agent {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 3px 0;
  white-space: nowrap;
}
.desc {
  overflow: hidden;
  text-overflow: ellipsis;
}
.agent .muted {
  margin-left: auto;
  flex-shrink: 0;
}
.dot {
  flex: none;
  width: 7px;
  height: 7px;
  border-radius: 50%;
  background: var(--fg-muted);
}
.agent.running .dot {
  background: var(--claude);
}
.agent.done .dot {
  background: var(--green);
}
.agent.stalled .dot {
  background: var(--red);
}
footer {
  display: flex;
  gap: 8px;
  padding: 10px 14px;
  border-top: 1px solid var(--border);
}
input {
  flex: 1;
  padding: 6px 10px;
  border: 1px solid var(--border);
  border-radius: 6px;
  background: var(--bg);
  color: var(--fg);
  font: inherit;
}
.icon {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 0 10px;
  border: 1px solid var(--border);
  border-radius: 6px;
  background: none;
  color: var(--fg-muted);
  font: inherit;
  font-size: 12px;
  cursor: pointer;
}
.icon:hover:not(:disabled) {
  background: var(--hover);
  color: var(--fg);
}
.icon:disabled {
  opacity: 0.45;
  cursor: default;
}
.send:not(:disabled) {
  border-color: var(--accent);
  color: var(--accent);
}
.mic.recording {
  border-color: var(--red);
  background: color-mix(in srgb, var(--red) 16%, transparent);
  color: var(--red);
}
.mic.transcribing {
  color: var(--accent);
}
.go {
  padding: 4px 12px;
  border: 0;
  border-radius: 6px;
  background: var(--accent);
  color: var(--bg);
  font: inherit;
  font-weight: 600;
  cursor: pointer;
}
</style>
