<script setup lang="ts">
import { defineAsyncComponent, onBeforeUnmount, onMounted } from "vue";
import type { GitStatus } from "../lib/git";
import type { OpenFile, Tab } from "../state/tabs";

/** Markdown/HTML clicado na saída do terminal: visualização rápida, sem abrir aba. */
const props = defineProps<{ tab: Tab; file: OpenFile; status: GitStatus | null }>();
const emit = defineEmits<{ close: []; openTab: [] }>();

const FileView = defineAsyncComponent(() => import("./FileView.vue"));

function onKey(e: KeyboardEvent) {
  if (e.key !== "Escape") return;
  e.preventDefault();
  e.stopPropagation();
  emit("close");
}

onMounted(() => window.addEventListener("keydown", onKey, true));
onBeforeUnmount(() => window.removeEventListener("keydown", onKey, true));
</script>

<template>
  <div class="backdrop" @mousedown.self="emit('close')">
    <div class="popup" role="dialog" :aria-label="file.path">
      <header @mousedown.prevent>
        <span class="name" :title="`${file.root}/${file.path}`">{{ file.path.slice(file.path.lastIndexOf("/") + 1) }}</span>
        <div class="modes">
          <button :class="{ on: file.mode === 'preview' }" @click="props.file.mode = 'preview'">Visualizar</button>
          <button :class="{ on: file.mode === 'file' }" @click="props.file.mode = 'file'">Código</button>
        </div>
        <button class="action" title="Mantém o arquivo aberto numa aba da área principal" @click="emit('openTab')">Abrir em aba</button>
        <button class="close" title="Fechar (Esc)" @click="emit('close')">
          <svg viewBox="0 0 16 16" width="13" height="13"><path d="M4 4l8 8M12 4l-8 8" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" /></svg>
        </button>
      </header>
      <div class="body">
        <FileView :tab="tab" :file="file" :status="status" />
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
  width: min(1000px, calc(100vw - 48px));
  height: min(86vh, calc(100vh - 48px));
  /* Arrastar o canto ajusta o tamanho, dependendo do arquivo. */
  min-width: 420px;
  min-height: 240px;
  max-width: calc(100vw - 24px);
  max-height: calc(100vh - 24px);
  resize: both;
  overflow: hidden;
  border: 1px solid var(--border);
  border-radius: 10px;
  background: var(--bg);
  box-shadow: 0 20px 48px rgb(0 0 0 / 0.5);
}
header {
  display: flex;
  align-items: center;
  gap: 8px;
  flex-shrink: 0;
  padding: 6px 8px 6px 14px;
  background: var(--bg-elevated);
  border-bottom: 1px solid var(--border);
  user-select: none;
}
.name {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  font-size: 13px;
  font-weight: 600;
  white-space: nowrap;
  text-overflow: ellipsis;
}
.modes {
  display: flex;
  padding: 2px;
  border-radius: 6px;
  background: var(--bg);
}
.modes button {
  padding: 3px 9px;
  border: 0;
  border-radius: 4px;
  background: none;
  color: var(--fg-muted);
  font: inherit;
  font-size: 11.5px;
  cursor: pointer;
}
.modes button.on {
  background: var(--active);
  color: var(--fg);
}
.action {
  padding: 4px 10px;
  border: 1px solid var(--border);
  border-radius: 6px;
  background: none;
  color: var(--fg);
  font: inherit;
  font-size: 12px;
  cursor: pointer;
}
.action:hover {
  background: var(--hover);
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
/* O arquivo rola por dentro: o pop-up tem altura fixa. */
.body {
  display: flex;
  flex-direction: column;
  flex: 1;
  min-height: 0;
  overflow: hidden;
}
.body > :deep(.viewer) {
  flex: 1;
  min-height: 0;
  height: auto;
}
</style>
