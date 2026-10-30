<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from "vue";
import StepItems from "./StepItems.vue";
import { claudeAgentSteps, type Agent, type AgentStep } from "../lib/pty";

/** O que um subagente fez até agora: o pedido, o que escreveu e cada ferramenta. */
const props = defineProps<{ agent: Agent; session: number }>();
const emit = defineEmits<{ close: [] }>();

const steps = ref<AgentStep[]>([]);
const error = ref("");
const list = ref<HTMLDivElement>();
/** Quem rolou pra cima está lendo: a lista só acompanha o fim se já estava lá. */
let follow = true;
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

// Relógio da linha "ao vivo": anda entre uma leitura e outra do histórico.
const now = ref(Date.now());
const clock = setInterval(() => (now.value = Date.now()), 1000);

/**
 * O que o agente está fazendo agora, como a linha piscando do terminal. O histórico
 * só ganha um passo quando ele termina: ferramenta sem resultado = rodando; senão, pensando.
 */
const live = computed(() => {
  if (props.agent.status !== "running") return null;
  const last = steps.value[steps.value.length - 1];
  const what = last?.kind === "tool" && last.result === null ? `${last.name}: ${last.summary}` : "Pensando…";
  const since = props.agent.updated ? Math.max(0, Math.round((now.value - Date.parse(props.agent.updated)) / 1000)) : null;
  const secs = since === null ? "" : since < 60 ? `${since}s` : `${Math.floor(since / 60)}min ${since % 60}s`;
  return { what, secs };
});

function onKey(e: KeyboardEvent) {
  if (e.key !== "Escape") return;
  e.preventDefault();
  e.stopPropagation();
  emit("close");
}
onMounted(() => window.addEventListener("keydown", onKey, true));
onBeforeUnmount(() => {
  clearInterval(timer);
  clearInterval(clock);
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
        <StepItems :steps="steps" prompt-label="Pedido" />
      </div>
      <div v-if="live" class="live" :title="live.what">
        <span class="spark" />
        <span class="what">{{ live.what }}</span>
        <span v-if="live.secs" class="secs">{{ live.secs }}</span>
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
/* Linha "ao vivo" fixa embaixo, como o status do Claude no terminal. */
.live {
  display: flex;
  align-items: center;
  gap: 8px;
  flex-shrink: 0;
  padding: 7px 14px;
  border-top: 1px solid var(--border);
  background: var(--bg-elevated);
  font-size: 12px;
}
.spark {
  flex-shrink: 0;
  width: 8px;
  height: 8px;
  border-radius: 50%;
  background: var(--claude);
  box-shadow: 0 0 6px var(--claude);
  animation: pulse 1s ease-in-out infinite;
}
.live .what {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  color: var(--claude);
  white-space: nowrap;
  text-overflow: ellipsis;
}
.secs {
  flex-shrink: 0;
  color: var(--fg-muted);
  font-variant-numeric: tabular-nums;
}
.empty {
  margin: auto;
  color: var(--fg-muted);
}
.empty.error {
  color: var(--red);
}
@keyframes pulse {
  50% {
    opacity: 0.3;
  }
}
@media (prefers-reduced-motion: reduce) {
  .spark,
  .dot.running {
    animation: none;
  }
}
</style>
