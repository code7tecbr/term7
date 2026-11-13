<script setup lang="ts">
import { nextTick, onBeforeUnmount, onMounted, ref, watch } from "vue";
import { claudeTaskLog, clipboardWrite, type BgTask } from "../lib/pty";
import { logSize, taskDuration, taskState } from "../lib/tasks";

/** Log de um comando em segundo plano do Claude, acompanhando enquanto roda. */
const props = defineProps<{ task: BgTask; session: number }>();
const emit = defineEmits<{ close: [] }>();

const text = ref("");
const truncated = ref(false);
const error = ref("");
const copied = ref(false);
const box = ref<HTMLPreElement>();
/** Quem rolou pra cima está lendo: o log só acompanha o fim se já estava lá. */
let follow = true;

async function load() {
  try {
    const log = await claudeTaskLog(props.session, props.task.id);
    error.value = "";
    truncated.value = log.truncated;
    if (log.text === text.value) return;
    text.value = log.text;
    if (follow) {
      await nextTick();
      box.value?.scrollTo({ top: box.value.scrollHeight });
    }
  } catch (err) {
    error.value = String(err);
  }
}

function onScroll() {
  const el = box.value!;
  follow = el.scrollHeight - el.scrollTop - el.clientHeight < 40;
}

// Só a que está rodando muda; terminada basta ler uma vez.
let timer: ReturnType<typeof setInterval> | undefined;
watch(
  () => props.task.status,
  (status) => {
    clearInterval(timer);
    if (status === "running") timer = setInterval(load, 1500);
    load();
  },
  { immediate: true },
);

const now = ref(Date.now());
const clock = setInterval(() => (now.value = Date.now()), 1000);

async function copyCommand() {
  await clipboardWrite(props.task.command).catch(() => {});
  copied.value = true;
  setTimeout(() => (copied.value = false), 1500);
}

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
    <div class="popup" role="dialog" :aria-label="task.description || task.command">
      <header @mousedown.prevent>
        <span class="dot" :class="task.status" />
        <div class="title">
          <span class="name">{{ task.description || task.command }}</span>
          <span class="sub">{{ taskState(task) }} · {{ taskDuration(task, now) }} · log {{ logSize(task.size) }}</span>
        </div>
        <button class="close" title="Fechar (Esc)" @click="emit('close')">
          <svg viewBox="0 0 16 16" width="13" height="13"><path d="M4 4l8 8M12 4l-8 8" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" /></svg>
        </button>
      </header>
      <div class="command" @mousedown.prevent>
        <code :title="task.command">{{ task.command }}</code>
        <button class="copy" title="Copiar o comando" @click="copyCommand">{{ copied ? "Copiado" : "Copiar" }}</button>
      </div>
      <p v-if="error" class="empty error">{{ error }}</p>
      <p v-else-if="!text" class="empty">{{ task.status === "running" ? "Sem saída ainda." : "O comando não escreveu nada." }}</p>
      <pre v-else ref="box" class="log" @scroll="onScroll"><span v-if="truncated" class="cut">… só o fim do log (128 KB)
</span>{{ text }}</pre>
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
  width: min(900px, calc(100vw - 48px));
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
.dot.failed {
  background: var(--red);
}
.dot.stopped {
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
.command {
  display: flex;
  align-items: center;
  gap: 8px;
  flex-shrink: 0;
  padding: 6px 8px 6px 14px;
  border-bottom: 1px solid var(--border);
  font-size: 12px;
}
.command code {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  color: var(--fg-muted);
  font-family: var(--font-mono);
  white-space: nowrap;
  text-overflow: ellipsis;
}
.copy {
  flex-shrink: 0;
  padding: 2px 8px;
  border: 1px solid var(--border);
  border-radius: 5px;
  background: none;
  color: var(--fg-muted);
  font: inherit;
  font-size: 11px;
  cursor: pointer;
}
.copy:hover {
  background: var(--hover);
  color: var(--fg);
}
.log {
  flex: 1;
  margin: 0;
  padding: 10px 14px 14px;
  overflow: auto;
  font-family: var(--font-mono);
  font-size: 12px;
  line-height: 1.45;
  white-space: pre-wrap;
  overflow-wrap: anywhere;
  user-select: text;
}
.cut {
  color: var(--fg-muted);
  font-style: italic;
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
  .dot.running {
    animation: none;
  }
}
</style>
