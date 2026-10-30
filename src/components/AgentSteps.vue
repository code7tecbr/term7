<script setup lang="ts">
import { nextTick, onBeforeUnmount, onMounted, ref, watch } from "vue";
import { claudeAgentSteps, type Agent, type AgentStep } from "../lib/pty";

/** O que um subagente fez até agora: o pedido, o que escreveu e cada ferramenta. */
const props = defineProps<{ agent: Agent; session: number }>();
const emit = defineEmits<{ close: [] }>();

const steps = ref<AgentStep[]>([]);
const error = ref("");
const list = ref<HTMLDivElement>();
/** Quem rolou pra cima está lendo: a lista só acompanha o fim se já estava lá. */
let follow = true;
// Resultados abertos, pela posição do passo (a lista só cresce no fim).
const open = ref(new Set<number>());

async function load() {
  try {
    const next = await claudeAgentSteps(props.session, props.agent.id);
    error.value = "";
    if (next.length === steps.value.length && JSON.stringify(next) === JSON.stringify(steps.value)) return;
    steps.value = next;
    if (follow) {
      await nextTick();
      list.value?.scrollTo({ top: list.value.scrollHeight });
    }
  } catch (err) {
    error.value = String(err);
  }
}

function onScroll() {
  const el = list.value!;
  follow = el.scrollHeight - el.scrollTop - el.clientHeight < 40;
}

function toggle(i: number) {
  const s = new Set(open.value);
  if (!s.delete(i)) s.add(i);
  open.value = s;
}

// Só o que está rodando muda; terminado basta ler uma vez.
let timer: ReturnType<typeof setInterval> | undefined;
watch(
  () => props.agent.status,
  (status) => {
    clearInterval(timer);
    if (status === "running") timer = setInterval(load, 2000);
    load();
  },
  { immediate: true },
);

const LABEL = { running: "Rodando", done: "Terminou", stalled: "Parado" } as const;

function onKey(e: KeyboardEvent) {
  if (e.key !== "Escape") return;
  e.preventDefault();
  e.stopPropagation();
  emit("close");
}
onMounted(() => window.addEventListener("keydown", onKey, true));
onBeforeUnmount(() => {
  clearInterval(timer);
  window.removeEventListener("keydown", onKey, true);
});
</script>

<template>
  <div class="backdrop" @mousedown.self="emit('close')">
    <div class="popup" role="dialog" :aria-label="agent.description || agent.kind">
      <header @mousedown.prevent>
        <span class="dot" :class="agent.status" />
        <div class="title">
          <span class="name">{{ agent.description || agent.kind }}</span>
          <span class="sub">{{ agent.kind }} · {{ LABEL[agent.status] }} · {{ agent.tool_calls }} ferramentas</span>
        </div>
        <button class="close" title="Fechar (Esc)" @click="emit('close')">
          <svg viewBox="0 0 16 16" width="13" height="13"><path d="M4 4l8 8M12 4l-8 8" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" /></svg>
        </button>
      </header>
      <div ref="list" class="body" @scroll="onScroll">
        <p v-if="error" class="empty error">{{ error }}</p>
        <p v-else-if="!steps.length" class="empty">Nada registrado ainda.</p>
        <template v-for="(s, i) in steps" :key="i">
          <div v-if="s.kind === 'prompt'" class="prompt">
            <span class="label">Pedido</span>
            <div class="text">{{ s.text }}</div>
          </div>
          <div v-else-if="s.kind === 'text'" class="text say">{{ s.text }}</div>
          <div v-else class="tool" :class="{ error: s.error, pending: s.result === null }">
            <button class="call" :disabled="s.result === null" @click="toggle(i)">
              <span class="tname">{{ s.name }}</span>
              <span class="summary">{{ s.summary }}</span>
              <span class="state">{{ s.result === null ? "rodando…" : s.error ? "erro" : open.has(i) ? "▾" : "▸" }}</span>
            </button>
            <pre v-if="s.result !== null && open.has(i)" class="result">{{ s.result || "(sem saída)" }}</pre>
          </div>
        </template>
      </div>
    </div>
  </div>
</template>

<style scoped>
.backdrop {
  position: fixed;
  inset: 0;
  z-index: 200;
  display: grid;
  place-items: center;
  background: rgb(0 0 0 / 0.45);
}
.popup {
  display: flex;
  flex-direction: column;
  width: min(820px, calc(100vw - 48px));
  height: min(80vh, calc(100vh - 48px));
  border: 1px solid var(--border);
  border-radius: 10px;
  background: var(--bg);
  box-shadow: 0 20px 48px rgb(0 0 0 / 0.5);
  overflow: hidden;
}
header {
  display: flex;
  align-items: center;
  gap: 10px;
  flex-shrink: 0;
  padding: 8px 8px 8px 14px;
  background: var(--bg-elevated);
  border-bottom: 1px solid var(--border);
  user-select: none;
}
.dot {
  flex-shrink: 0;
  width: 8px;
  height: 8px;
  border-radius: 50%;
  background: var(--fg-muted);
}
.dot.running {
  background: var(--accent);
  animation: pulse 1.2s ease-in-out infinite;
}
.dot.done {
  background: var(--green);
}
.dot.stalled {
  background: var(--yellow);
}
.title {
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
.name {
  font-size: 13px;
  font-weight: 600;
}
.sub {
  color: var(--fg-muted);
  font-size: 11px;
}
.close {
  display: grid;
  place-items: center;
  width: 26px;
  height: 26px;
  border: 0;
  border-radius: 5px;
  background: none;
  color: var(--fg-muted);
  cursor: pointer;
}
.close:hover {
  background: var(--hover);
  color: var(--fg);
}
.body {
  flex: 1;
  display: flex;
  flex-direction: column;
  gap: 8px;
  padding: 12px 14px 16px;
  overflow-y: auto;
  font-size: 13px;
}
.empty {
  margin: auto;
  color: var(--fg-muted);
}
.empty.error {
  color: var(--red);
}
.text {
  white-space: pre-wrap;
  overflow-wrap: anywhere;
  line-height: 1.5;
}
.prompt {
  padding: 8px 10px;
  border-left: 2px solid var(--claude);
  border-radius: 4px;
  background: var(--bg-elevated);
}
.prompt .text {
  max-height: 180px;
  overflow-y: auto;
  color: var(--fg-muted);
}
.label {
  display: block;
  margin-bottom: 4px;
  color: var(--claude);
  font-size: 11px;
  font-weight: 600;
}
.tool {
  border: 1px solid var(--border);
  border-radius: 6px;
}
.call {
  display: flex;
  align-items: baseline;
  gap: 8px;
  width: 100%;
  padding: 5px 9px;
  border: 0;
  background: none;
  color: var(--fg);
  font: inherit;
  font-size: 12px;
  text-align: left;
  cursor: pointer;
}
.call:hover:not(:disabled) {
  background: var(--hover);
}
.call:disabled {
  cursor: default;
}
.tname {
  flex-shrink: 0;
  color: var(--purple);
  font-weight: 600;
}
.summary {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  color: var(--fg-muted);
  font-family: var(--font-mono);
  white-space: nowrap;
  text-overflow: ellipsis;
}
.state {
  flex-shrink: 0;
  color: var(--fg-muted);
  font-size: 11px;
}
.tool.pending .state {
  color: var(--accent);
}
.tool.error {
  border-color: rgb(247 118 142 / 0.4);
}
.tool.error .state {
  color: var(--red);
}
.result {
  max-height: 240px;
  margin: 0;
  padding: 8px 10px;
  overflow: auto;
  border-top: 1px solid var(--border);
  color: var(--fg-muted);
  font-family: var(--font-mono);
  font-size: 11.5px;
  white-space: pre-wrap;
  overflow-wrap: anywhere;
}
@keyframes pulse {
  50% {
    opacity: 0.3;
  }
}
@media (prefers-reduced-motion: reduce) {
  .dot.running {
    animation: none;
  }
}
</style>
