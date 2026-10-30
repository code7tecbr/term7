<script setup lang="ts">
import { nextTick, onBeforeUnmount, onMounted, ref } from "vue";

/** Aviso antes de fechar o app com programas rodando nas abas. */
defineProps<{ items: { tab: string; what: string }[] }>();
const emit = defineEmits<{ cancel: []; confirm: [] }>();

const cancelBtn = ref<HTMLButtonElement>();

function onKey(e: KeyboardEvent) {
  if (e.key !== "Escape") return;
  e.preventDefault();
  e.stopPropagation();
  emit("cancel");
}
onMounted(() => {
  window.addEventListener("keydown", onKey, true);
  // Enter sem querer não fecha nada: o foco começa no Cancelar.
  nextTick(() => cancelBtn.value?.focus());
});
onBeforeUnmount(() => window.removeEventListener("keydown", onKey, true));
</script>

<template>
  <div class="backdrop" @mousedown.self="emit('cancel')">
    <div class="dialog" role="alertdialog" aria-labelledby="close-title">
      <h2 id="close-title">Ainda tem coisa rodando</h2>
      <p>Fechar o term7 encerra estes programas:</p>
      <ul>
        <li v-for="(it, i) in items" :key="i">
          <span class="what">{{ it.what }}</span>
          <span class="tab">{{ it.tab }}</span>
        </li>
      </ul>
      <div class="actions">
        <button ref="cancelBtn" @click="emit('cancel')">Cancelar</button>
        <button class="danger" @click="emit('confirm')">Fechar mesmo assim</button>
      </div>
    </div>
  </div>
</template>

<style scoped>
.backdrop {
  position: fixed;
  inset: 0;
  z-index: 300;
  display: grid;
  place-items: center;
  background: rgb(0 0 0 / 0.5);
}
.dialog {
  width: min(440px, calc(100vw - 48px));
  padding: 18px 20px 16px;
  border: 1px solid var(--border);
  border-radius: 10px;
  background: var(--bg-elevated);
  box-shadow: 0 20px 48px rgb(0 0 0 / 0.5);
  font-size: 13px;
}
h2 {
  margin: 0 0 6px;
  font-size: 15px;
}
p {
  margin: 0 0 10px;
  color: var(--fg-muted);
}
ul {
  max-height: 240px;
  margin: 0;
  padding: 0;
  overflow-y: auto;
  list-style: none;
}
li {
  display: flex;
  align-items: baseline;
  gap: 10px;
  padding: 5px 8px;
  border-radius: 6px;
  background: var(--bg);
}
li + li {
  margin-top: 4px;
}
.what {
  flex-shrink: 0;
  color: var(--yellow);
  font-family: var(--font-mono);
  font-size: 12px;
}
.tab {
  min-width: 0;
  overflow: hidden;
  color: var(--fg-muted);
  white-space: nowrap;
  text-overflow: ellipsis;
}
.actions {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
  margin-top: 16px;
}
button {
  height: 30px;
  padding: 0 14px;
  border: 1px solid var(--border);
  border-radius: 6px;
  background: none;
  color: var(--fg);
  font: inherit;
  cursor: pointer;
}
button:hover {
  background: var(--hover);
}
button:focus-visible {
  outline: 2px solid var(--accent);
  outline-offset: 1px;
}
button.danger {
  border-color: transparent;
  background: color-mix(in srgb, var(--red) 80%, transparent);
  color: #1a1b26;
  font-weight: 600;
}
button.danger:hover {
  background: var(--red);
}
</style>
