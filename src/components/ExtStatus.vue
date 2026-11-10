<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from "vue";
import { openUrl } from "../lib/github";
import { ago, extStatus, isStale, LEVEL_LABEL, sorted, worst, type StatusSource } from "../state/extStatus";

/** Status publicado por extensões (ex.: statusbot): um botão por fonte, com a lista ao clicar. */

// Relógio pros "há X min" e pra perceber quando a extensão parou de mandar.
const now = ref(Date.now());
const timer = setInterval(() => (now.value = Date.now()), 30_000);

const sources = computed(() => Object.values(extStatus.sources).sort((a, b) => a.title.localeCompare(b.title)));
const openId = ref<string | null>(null);
const opened = computed(() => (openId.value ? extStatus.sources[openId.value] : undefined));
const root = ref<HTMLElement>();

const notUp = (s: StatusSource) => s.items.filter((i) => i.status !== "up").length;

function summary(s: StatusSource) {
  if (s.error) return s.error;
  if (isStale(s, now.value)) return "A extensão parou de mandar atualizações: o que aparece pode estar velho.";
  const n = notUp(s);
  return n ? `${n} de ${s.items.length} fora do normal` : `Tudo normal (${s.items.length})`;
}

const open = (url?: string | null) => url && openUrl(url).catch(() => {});

function onPointerDown(e: PointerEvent) {
  if (openId.value && !root.value?.contains(e.target as Node)) openId.value = null;
}
function onKey(e: KeyboardEvent) {
  if (e.key === "Escape" && openId.value) openId.value = null;
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
  <div v-if="sources.length" ref="root" class="ext-status">
    <button
      v-for="s in sources"
      :key="s.id"
      class="btn"
      :class="[worst(s, now), { open: openId === s.id }]"
      :title="`${s.title}: ${summary(s)}`"
      @click="openId = openId === s.id ? null : s.id"
    >
      <span class="dot" />
      {{ s.title }}
      <span v-if="notUp(s)" class="count">{{ notUp(s) }}</span>
    </button>

    <div v-if="opened" class="popover">
      <header>
        <button class="title" :disabled="!opened.link" :title="opened.link ? 'Abrir no navegador' : ''" @click="open(opened.link)">
          {{ opened.title }}
        </button>
        <span class="read">{{ opened.checkedAt ? `lido ${ago(opened.checkedAt, now)}` : "" }}</span>
      </header>
      <p v-if="opened.error || isStale(opened, now)" class="warn">{{ summary(opened) }}</p>
      <ul>
        <li v-for="(it, i) in sorted(opened.items)" :key="i" :class="it.status">
          <span class="dot" />
          <button class="label" :disabled="!it.link" :title="it.link ? 'Abrir no navegador' : ''" @click="open(it.link)">
            {{ it.label }}
          </button>
          <span class="state">
            {{ it.status === "up" ? "" : LEVEL_LABEL[it.status] }}{{ it.status !== "up" && it.since ? ` ${ago(it.since, now)}` : "" }}
          </span>
          <span v-if="it.detail && it.status !== 'up'" class="detail">{{ it.detail }}</span>
        </li>
        <li v-if="!opened.items.length" class="empty">Nada sendo monitorado.</li>
      </ul>
    </div>
  </div>
</template>

<style scoped>
.ext-status {
  position: relative;
  display: flex;
  align-items: center;
  gap: 4px;
}
.btn {
  display: flex;
  align-items: center;
  gap: 6px;
  height: 20px;
  padding: 0 8px;
  border: 0;
  border-radius: 5px;
  background: none;
  color: inherit;
  font: inherit;
  cursor: pointer;
}
.btn:hover,
.btn.open {
  background: var(--hover);
  color: var(--fg);
}
.dot {
  flex-shrink: 0;
  width: 7px;
  height: 7px;
  border-radius: 50%;
  background: var(--green);
}
.down .dot {
  background: var(--red);
}
.degraded .dot {
  background: var(--orange);
}
.maintenance .dot {
  background: var(--purple);
}
.unknown .dot {
  background: var(--fg-muted);
}
.count {
  color: var(--fg);
  font-weight: 600;
}
.popover {
  position: absolute;
  right: 0;
  bottom: calc(100% + 8px);
  z-index: 50;
  width: min(420px, 90vw);
  max-height: 60vh;
  overflow: auto;
  padding: 8px;
  border: 1px solid var(--border);
  border-radius: 8px;
  background: var(--bg-elevated);
  box-shadow: 0 8px 24px rgb(0 0 0 / 0.4);
  color: var(--fg);
  user-select: text;
}
header {
  display: flex;
  align-items: baseline;
  justify-content: space-between;
  gap: 8px;
  padding: 2px 4px 6px;
}
button.title,
button.label {
  padding: 0;
  border: 0;
  background: none;
  color: inherit;
  font: inherit;
  text-align: left;
  cursor: pointer;
}
button.title {
  font-weight: 600;
}
button.title:disabled,
button.label:disabled {
  cursor: default;
}
button.title:not(:disabled):hover,
button.label:not(:disabled):hover {
  color: var(--accent);
  text-decoration: underline;
}
.read,
.state,
.detail {
  color: var(--fg-muted);
  font-size: 11.5px;
}
.warn {
  margin: 0 4px 6px;
  color: var(--yellow);
}
ul {
  margin: 0;
  padding: 0;
  list-style: none;
}
li {
  display: grid;
  grid-template-columns: auto 1fr auto;
  align-items: center;
  column-gap: 8px;
  padding: 4px;
  border-radius: 5px;
}
li:hover {
  background: var(--hover);
}
.detail {
  grid-column: 2 / -1;
}
.state {
  text-align: right;
  white-space: nowrap;
}
.down .state {
  color: var(--red);
}
.empty {
  display: block;
  color: var(--fg-muted);
}
</style>
