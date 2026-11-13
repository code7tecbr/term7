<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from "vue";
import type { BgTask } from "../lib/pty";
import { logSize, taskDuration, taskState } from "../lib/tasks";

/** Comandos que o Claude da aba rodou em segundo plano. */
const props = defineProps<{ tasks: BgTask[] }>();
const emit = defineEmits<{ close: []; open: [id: string] }>();

const box = ref<HTMLDivElement>();
// Relógio pro tempo das que estão rodando andar entre uma leitura e outra.
const now = ref(Date.now());
const timer = setInterval(() => (now.value = Date.now()), 1000);

const running = computed(() => props.tasks.filter((t) => t.status === "running").length);

function onPointerDown(e: PointerEvent) {
  const target = e.target as HTMLElement;
  // O botão da barra já alterna o menu: fechar aqui faria ele reabrir no click.
  if (box.value?.contains(target) || target.closest("[data-tasks-toggle]")) return;
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
  <div ref="box" class="tasks" @mousedown.prevent>
    <div class="head">
      <span class="title">Tarefas em segundo plano</span>
      <span class="muted">{{ running ? `${running} rodando · ` : "" }}{{ tasks.length }} na sessão</span>
    </div>
    <ul>
      <li v-for="t in tasks" :key="t.id" :class="t.status" :title="`${t.command}\nClique pra ver o log`" @click="emit('open', t.id)">
        <span class="dot" />
        <div class="text">
          <span class="name">{{ t.description || t.command }}</span>
          <code class="sub">{{ t.command }}</code>
        </div>
        <div class="stats">
          <span class="status">{{ taskState(t) }} · {{ taskDuration(t, now) }}</span>
          <span class="muted">log {{ logSize(t.size) }}</span>
        </div>
      </li>
    </ul>
  </div>
</template>

<style scoped>
.tasks {
  position: fixed;
  top: 32px;
  right: 10px;
  z-index: 150;
  display: flex;
  flex-direction: column;
  width: min(560px, calc(100vw - 20px));
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
li.failed .dot {
  background: var(--red);
}
li.stopped .dot {
  background: var(--yellow);
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
  font-family: var(--font-mono);
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
li.failed .status {
  color: var(--red);
}
li.stopped .status {
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
