<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from "vue";
import { gitBranches, gitCheckout, gitFetch, repoName, type Branch } from "../lib/git";

const props = defineProps<{ root: string; current: string | null }>();
const emit = defineEmits<{ close: []; switched: [] }>();

const box = ref<HTMLDivElement>();
const branches = ref<Branch[]>([]);
const loading = ref(true);
const busy = ref(false);
const error = ref<string | null>(null);
const selected = ref(0);

const fetching = ref(false);
// As remotas só aparecem depois de atualizar (o backend já esconde as que têm branch local).
const showRemote = ref(false);

const options = computed(() => [
  ...branches.value.filter((b) => !b.remote),
  ...(showRemote.value ? branches.value.filter((b) => b.remote) : []),
]);
const remoteCount = computed(() => branches.value.filter((b) => b.remote).length);

async function refresh() {
  if (fetching.value) return;
  fetching.value = true;
  error.value = null;
  try {
    await gitFetch(props.root);
  } catch (err) {
    error.value = String(err);
  }
  branches.value = await gitBranches(props.root).catch(() => branches.value);
  showRemote.value = true;
  fetching.value = false;
}

const rtf = new Intl.RelativeTimeFormat("pt-BR", { numeric: "auto" });
function ago(unix: number) {
  const s = unix - Date.now() / 1000;
  const units: [Intl.RelativeTimeFormatUnit, number][] = [
    ["year", 31536000],
    ["month", 2592000],
    ["week", 604800],
    ["day", 86400],
    ["hour", 3600],
    ["minute", 60],
  ];
  for (const [unit, size] of units) if (Math.abs(s) >= size) return rtf.format(Math.round(s / size), unit);
  return "agora";
}

async function choose(branch: Branch | undefined) {
  if (!branch || busy.value) return;
  if (branch.current) return emit("close");
  busy.value = true;
  error.value = null;
  try {
    await gitCheckout(props.root, branch.name, { remote: branch.remote });
    emit("switched");
    emit("close");
  } catch (err) {
    error.value = String(err);
  } finally {
    busy.value = false;
  }
}

function onKey(e: KeyboardEvent) {
  if (e.key === "Escape") emit("close");
  else if (e.key === "ArrowDown") selected.value = Math.min(selected.value + 1, options.value.length - 1);
  else if (e.key === "ArrowUp") selected.value = Math.max(selected.value - 1, 0);
  else if (e.key === "Enter") choose(options.value[selected.value]);
  else return;
  e.preventDefault();
  e.stopPropagation();
}

function onPointerDown(e: PointerEvent) {
  if (!box.value?.contains(e.target as Node)) emit("close");
}

onMounted(async () => {
  window.addEventListener("pointerdown", onPointerDown, true);
  // Captura: as setas/Enter não vão pro terminal enquanto a lista está aberta.
  window.addEventListener("keydown", onKey, true);
  branches.value = await gitBranches(props.root).catch(() => []);
  selected.value = Math.max(options.value.findIndex((b) => b.current), 0);
  loading.value = false;
});

onBeforeUnmount(() => {
  window.removeEventListener("pointerdown", onPointerDown, true);
  window.removeEventListener("keydown", onKey, true);
});
</script>

<template>
  <div ref="box" class="picker">
    <div class="repo">
      <span class="name">{{ repoName(root) }}</span>
      <span class="root">{{ root }}</span>
      <button
        class="refresh"
        :class="{ spinning: fetching }"
        title="Buscar branches remotas (git fetch)"
        :disabled="fetching"
        @mousedown.prevent
        @click="refresh"
      >
        <svg viewBox="0 0 16 16" width="14" height="14"><path d="M13 8a5 5 0 1 1-1.5-3.5M13 3v2.5h-2.5" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" /></svg>
      </button>
    </div>
    <div v-if="error" class="error">{{ error }}</div>
    <ul>
      <li v-if="loading" class="muted">Carregando…</li>
      <li v-else-if="!options.length" class="muted">Nenhuma branch local.</li>
      <li
        v-for="(branch, i) in options"
        :key="branch.name"
        :class="{ selected: i === selected, current: branch.current }"
        @mouseenter="selected = i"
        @mousedown.prevent
        @click="choose(branch)"
      >
        <span class="check">{{ branch.current ? "✓" : "" }}</span>
        <span class="label">{{ branch.name }}</span>
        <span v-if="branch.remote" class="tag">remota</span>
        <span class="date">{{ ago(branch.updatedAt) }}</span>
      </li>
    </ul>
    <div v-if="fetching" class="muted footer">Buscando branches remotas…</div>
    <div v-else-if="showRemote && !remoteCount" class="muted footer">Nenhuma branch remota nova.</div>
    <div v-if="busy" class="muted footer">Trocando de branch…</div>
  </div>
</template>

<style scoped>
.picker {
  position: fixed;
  right: 10px;
  bottom: 32px;
  z-index: 150;
  display: flex;
  flex-direction: column;
  width: min(420px, calc(100vw - 20px));
  max-height: min(460px, 70vh);
  background: var(--bg-elevated);
  border: 1px solid var(--border);
  border-radius: 10px;
  box-shadow: 0 12px 32px rgb(0 0 0 / 0.45);
  font-size: 12.5px;
  overflow: hidden;
}
.repo {
  display: flex;
  align-items: baseline;
  gap: 8px;
  padding: 10px 12px 6px;
  min-width: 0;
}
.repo .name {
  font-weight: 600;
}
.repo .root {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  color: var(--fg-muted);
  font-size: 11px;
  white-space: nowrap;
  text-overflow: ellipsis;
}
.refresh {
  display: grid;
  place-items: center;
  flex-shrink: 0;
  align-self: center;
  width: 26px;
  height: 26px;
  border: 0;
  border-radius: 5px;
  background: none;
  color: var(--fg-muted);
  cursor: pointer;
}
.refresh:hover:not(:disabled) {
  background: var(--hover);
  color: var(--fg);
}
.refresh.spinning svg {
  animation: spin 0.9s linear infinite;
}
@keyframes spin {
  to {
    transform: rotate(360deg);
  }
}
.tag {
  padding: 0 6px;
  border-radius: 8px;
  background: var(--active);
  color: var(--fg-muted);
  font-size: 10.5px;
}
.error {
  margin: 0 8px 6px;
  padding: 6px 10px;
  border-radius: 6px;
  background: var(--del-bg);
  color: var(--red);
  font-family: var(--font-mono);
  font-size: 11.5px;
  white-space: pre-wrap;
}
ul {
  flex: 1;
  margin: 0;
  padding: 0 4px 6px;
  list-style: none;
  overflow-y: auto;
}
li {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 6px 8px;
  border-radius: 5px;
  cursor: pointer;
}
li.selected {
  background: var(--hover);
}
li.current .label {
  color: var(--accent);
}
.check {
  width: 12px;
  color: var(--accent);
  text-align: center;
}
.label {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  white-space: nowrap;
  text-overflow: ellipsis;
  font-family: var(--font-mono);
  font-size: 12px;
}
.date {
  flex-shrink: 0;
  color: var(--fg-muted);
  font-size: 11px;
}
.muted {
  color: var(--fg-muted);
  cursor: default;
}
.footer {
  padding: 6px 12px 10px;
}
</style>
