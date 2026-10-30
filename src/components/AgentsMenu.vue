<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from "vue";
import type { Agent } from "../lib/pty";

/** Subagentes do Claude da aba: rodando, terminados e interrompidos. */
const props = defineProps<{ agents: Agent[] }>();
const emit = defineEmits<{ close: []; open: [id: string] }>();

const box = ref<HTMLDivElement>();
// Relógio pro tempo dos que estão rodando andar entre uma leitura e outra.
const now = ref(Date.now());
const timer = setInterval(() => (now.value = Date.now()), 1000);

const LABEL = { running: "Rodando", done: "Terminou", stalled: "Parado" } as const;

const running = computed(() => props.agents.filter((a) => a.status === "running").length);

/** "claude-sonnet-5-5" -> "sonnet 5.5"; apelido ("sonnet") fica como está. */
function model(m: string | null) {
  if (!m) return null;
  const match = /^claude-([a-z]+)-(\d+)-(\d+)/.exec(m);
  return match ? `${match[1]} ${match[2]}.${match[3]}` : m;
}

function duration(a: Agent) {
  if (!a.started) return "";
  const end = a.status === "running" ? now.value : Date.parse(a.updated ?? a.started);
  const s = Math.max(0, Math.round((end - Date.parse(a.started)) / 1000));
  if (s < 60) return `${s}s`;
  const m = Math.floor(s / 60);
  return m < 60 ? `${m}min ${s % 60}s` : `${Math.floor(m / 60)}h ${m % 60}min`;
}

function tokens(n: number) {
  return n >= 1000 ? `${(n / 1000).toFixed(n >= 100_000 ? 0 : 1)}k` : String(n);
}

function onPointerDown(e: PointerEvent) {
  const target = e.target as HTMLElement;
  // O botão da barra já alterna o menu: fechar aqui faria ele reabrir no click.
  if (box.value?.contains(target) || target.closest("[data-agents-toggle]")) return;
  emit("close");
}
function onKey(e: KeyboardEvent) {
  if (e.key !== "Escape") return;
  e.preventDefault();
  e.stopPropagation();
  emit("close");
}

onMounted(() => {
  window.addEventListener("pointerdown", onPointerDown, true);
  window.addEventListener("keydown", onKey, true);
});
onBeforeUnmount(() => {
  clearInterval(timer);
  window.removeEventListener("pointerdown", onPointerDown, true);
  window.removeEventListener("keydown", onKey, true);
});
</script>

<template>
  <div ref="box" class="agents" @mousedown.prevent>
    <div class="head">
      <span class="title">Agentes</span>
      <span class="muted">{{ running ? `${running} rodando · ` : "" }}{{ agents.length }} na sessão</span>
    </div>
    <ul>
      <li v-for="a in agents" :key="a.id" :class="a.status" :title="`${a.description}\nClique pra ver o que ele está fazendo`" @click="emit('open', a.id)">
        <span class="dot" />
        <div class="text">
          <span class="name">{{ a.description || a.kind }}</span>
          <span class="sub">
            {{ a.kind }}<template v-if="model(a.model)"> · {{ model(a.model) }}</template
            ><template v-if="a.background"> · em segundo plano</template>
            <template v-if="a.status === 'running' && a.last_tool"> · usando {{ a.last_tool }}</template>
          </span>
        </div>
        <div class="stats">
          <span class="status">{{ LABEL[a.status] }} · {{ duration(a) }}</span>
          <span class="muted" :title="`Contexto: ${a.context_tokens} tokens · saída total: ${a.output_tokens}`">
            {{ a.tool_calls }} {{ a.tool_calls === 1 ? "ferramenta" : "ferramentas" }} · {{ tokens(a.context_tokens) }} ctx
          </span>
        </div>
      </li>
    </ul>
  </div>
</template>

<style scoped>
.agents {
  position: fixed;
  top: 32px;
  right: 10px;
  z-index: 150;
  display: flex;
  flex-direction: column;
  width: min(520px, calc(100vw - 20px));
  max-height: min(520px, 75vh);
  background: var(--bg-elevated);
  border: 1px solid var(--border);
  border-radius: 10px;
  box-shadow: 0 12px 32px rgb(0 0 0 / 0.45);
  font-size: 12.5px;
  overflow: hidden;
  user-select: none;
}
.head {
  display: flex;
  align-items: baseline;
  justify-content: space-between;
  gap: 8px;
  padding: 10px 12px 6px;
}
.title {
  font-weight: 600;
}
ul {
  flex: 1;
  margin: 0;
  padding: 2px 6px 6px;
  list-style: none;
  overflow-y: auto;
}
li {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 6px 8px;
  border-radius: 6px;
}
li {
  cursor: pointer;
}
li:hover {
  background: var(--hover);
}
.dot {
  flex-shrink: 0;
  width: 8px;
  height: 8px;
  border-radius: 50%;
  background: var(--fg-muted);
}
li.running .dot {
  background: var(--accent);
  animation: pulse 1.2s ease-in-out infinite;
}
li.done .dot {
  background: var(--green);
}
.text {
  display: flex;
  flex-direction: column;
  flex: 1;
  min-width: 0;
}
.name,
.sub {
  overflow: hidden;
  white-space: nowrap;
  text-overflow: ellipsis;
}
.sub {
  color: var(--fg-muted);
  font-size: 11px;
}
.stats {
  display: flex;
  flex-direction: column;
  align-items: flex-end;
  flex-shrink: 0;
}
.status {
  color: var(--fg-muted);
  font-size: 11px;
}
li.running .status {
  color: var(--accent);
}
li.stalled .status {
  color: var(--yellow);
}
.muted {
  color: var(--fg-muted);
  font-size: 11px;
}
@keyframes pulse {
  50% {
    opacity: 0.3;
  }
}
@media (prefers-reduced-motion: reduce) {
  li.running .dot {
    animation: none;
  }
}
</style>
