<script setup lang="ts">
import { nextTick, onBeforeUnmount, onMounted, ref } from "vue";

export type MenuItem =
  | { label: string; action: () => void; danger?: boolean; dot?: string }
  | { separator: true }
  | { colors: string[]; selected: string | null; pick: (color: string | null) => void; allowNone?: boolean };

const props = defineProps<{ x: number; y: number; items: MenuItem[] }>();
const emit = defineEmits<{ close: [] }>();

const menu = ref<HTMLDivElement>();
const pos = ref({ left: props.x, top: props.y });

function run(fn: () => void) {
  emit("close");
  fn();
}

function onPointerDown(e: PointerEvent) {
  if (!menu.value?.contains(e.target as Node)) emit("close");
}

function onKey(e: KeyboardEvent) {
  if (e.key === "Escape") emit("close");
}

onMounted(async () => {
  // Não deixa o menu sair da janela.
  await nextTick();
  const rect = menu.value!.getBoundingClientRect();
  pos.value = {
    left: Math.min(props.x, window.innerWidth - rect.width - 8),
    top: Math.min(props.y, window.innerHeight - rect.height - 8),
  };
  window.addEventListener("pointerdown", onPointerDown, true);
  window.addEventListener("keydown", onKey, true);
  window.addEventListener("blur", () => emit("close"), { once: true });
});

onBeforeUnmount(() => {
  window.removeEventListener("pointerdown", onPointerDown, true);
  window.removeEventListener("keydown", onKey, true);
});
</script>

<template>
  <div ref="menu" class="menu" :style="{ left: pos.left + 'px', top: pos.top + 'px' }" @mousedown.prevent>
    <template v-for="(item, i) in items" :key="i">
      <hr v-if="'separator' in item" />
      <div v-else-if="'colors' in item" class="colors">
        <button
          v-if="item.allowNone"
          class="swatch none"
          :class="{ selected: item.selected === null }"
          title="Sem cor"
          @click="item.pick(null)"
        />
        <button
          v-for="color in item.colors"
          :key="color"
          class="swatch"
          :class="{ selected: color === item.selected }"
          :style="{ background: color }"
          @click="item.pick(color)"
        />
      </div>
      <button v-else class="item" :class="{ danger: item.danger }" @click="run(item.action)">
        <span v-if="item.dot" class="dot" :style="{ background: item.dot }" />
        {{ item.label }}
      </button>
    </template>
  </div>
</template>

<style scoped>
.menu {
  position: fixed;
  z-index: 100;
  min-width: 200px;
  padding: 4px;
  background: var(--bg-elevated);
  border: 1px solid var(--border);
  border-radius: 8px;
  box-shadow: 0 8px 24px rgb(0 0 0 / 0.4);
  font-size: 13px;
}
.item {
  display: flex;
  align-items: center;
  gap: 8px;
  width: 100%;
  padding: 6px 10px;
  border: 0;
  border-radius: 5px;
  background: none;
  color: var(--fg);
  text-align: left;
  cursor: pointer;
  font: inherit;
}
.item:hover {
  background: var(--hover);
}
.item.danger {
  color: var(--red);
}
.dot {
  width: 8px;
  height: 8px;
  border-radius: 50%;
}
hr {
  border: 0;
  border-top: 1px solid var(--border);
  margin: 4px 0;
}
.colors {
  display: flex;
  gap: 6px;
  padding: 6px 10px;
}
.swatch {
  width: 18px;
  height: 18px;
  border-radius: 50%;
  border: 2px solid transparent;
  cursor: pointer;
}
.swatch.none {
  background: linear-gradient(135deg, transparent 45%, var(--fg-muted) 45% 55%, transparent 55%);
  border-color: var(--border);
}
.swatch.selected {
  border-color: var(--fg);
}
</style>
