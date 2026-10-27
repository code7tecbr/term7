<script setup lang="ts">
import { computed, reactive, ref, shallowRef, watch } from "vue";
import { gitLsFiles, repoName, type ChangeKind, type FileChange, type GitStatus } from "../lib/git";
import { openFile } from "../state/editor";

const props = defineProps<{ status: GitStatus; activePath: string | null }>();
defineEmits<{ close: []; refresh: [] }>();

// Letras e cores iguais às do VS Code.
const BADGE: Record<ChangeKind, { letter: string; label: string }> = {
  added: { letter: "A", label: "Adicionado" },
  modified: { letter: "M", label: "Modificado" },
  deleted: { letter: "D", label: "Removido" },
  renamed: { letter: "R", label: "Renomeado" },
  untracked: { letter: "U", label: "Novo (não rastreado)" },
  conflict: { letter: "!", label: "Conflito" },
};

type Filter = "all" | "changed";
const FILTER_KEY = "term7.explorerFilter";
const filter = ref<Filter>(readFilter());

function readFilter(): Filter {
  try {
    return localStorage.getItem(FILTER_KEY) === "all" ? "all" : "changed";
  } catch {
    return "changed";
  }
}
watch(filter, (f) => {
  try {
    localStorage.setItem(FILTER_KEY, f);
  } catch {
    // sem storage: só não lembra
  }
});

// ---- lista de arquivos do repositório ----
const allPaths = shallowRef<string[]>([]);
const truncated = ref(false);
let loadId = 0;

async function loadFiles() {
  const root = props.status.root;
  const id = ++loadId;
  const list = await gitLsFiles(root).catch(() => null);
  if (id !== loadId || !list) return;
  allPaths.value = list.paths;
  truncated.value = list.truncated;
}

// Muda quando aparece ou some arquivo alterado (ex.: arquivo novo): aí relista.
const changedKey = computed(() => props.status.files.map((f) => f.path).join("\0"));
watch([() => props.status.root, changedKey], loadFiles, { immediate: true });

// ---- árvore ----
const changes = computed(() => new Map(props.status.files.map((f) => [f.path, f])));

/** Cor de uma pasta pelo que tem dentro: conflito > alterado > só arquivos novos. */
const dirKinds = computed(() => {
  const kinds = new Map<string, ChangeKind>();
  const rank = (k: ChangeKind) => (k === "conflict" ? 3 : k === "added" || k === "untracked" ? 1 : 2);
  for (const f of props.status.files) {
    const parts = f.path.split("/");
    for (let i = 1; i < parts.length; i++) {
      const dir = parts.slice(0, i).join("/");
      const prev = kinds.get(dir);
      if (!prev || rank(f.kind) > rank(prev)) kinds.set(dir, f.kind);
    }
  }
  return kinds;
});

interface Node {
  dirs: string[];
  files: string[];
}

/** Índice pasta -> filhos, montado uma vez por lista. */
function buildIndex(paths: string[]) {
  const index = new Map<string, Node>([["", { dirs: [], files: [] }]]);
  const node = (dir: string) => {
    let n = index.get(dir);
    if (!n) {
      n = { dirs: [], files: [] };
      index.set(dir, n);
      const i = dir.lastIndexOf("/");
      node(i < 0 ? "" : dir.slice(0, i)).dirs.push(dir);
    }
    return n;
  };
  for (const path of paths) {
    const i = path.lastIndexOf("/");
    node(i < 0 ? "" : path.slice(0, i)).files.push(path);
  }
  const byName = (a: string, b: string) => baseName(a).localeCompare(baseName(b), undefined, { sensitivity: "base" });
  for (const n of index.values()) {
    n.dirs.sort(byName);
    n.files.sort(byName);
  }
  return index;
}

const index = computed(() => {
  if (filter.value === "changed") return buildIndex(props.status.files.map((f) => f.path));
  // Apagados do disco podem não estar no ls-files: entram pelo status.
  const set = new Set(allPaths.value);
  for (const f of props.status.files) set.add(f.path);
  return buildIndex([...set]);
});

// Pastas abertas, por repositório. No filtro "alterados" tudo começa aberto,
// então lá se guarda o contrário: as pastas fechadas.
const expanded = reactive(new Map<string, Set<string>>());
const collapsedChanged = reactive(new Map<string, Set<string>>());
const setFor = (map: Map<string, Set<string>>) => {
  const root = props.status.root;
  if (!map.has(root)) map.set(root, new Set());
  return map.get(root)!;
};

const isOpen = (dir: string) =>
  filter.value === "changed" ? !setFor(collapsedChanged).has(dir) : setFor(expanded).has(dir);

function toggleDir(dir: string) {
  const set = setFor(filter.value === "changed" ? collapsedChanged : expanded);
  if (set.has(dir)) set.delete(dir);
  else set.add(dir);
}

function collapseAll() {
  if (filter.value === "changed") {
    const set = setFor(collapsedChanged);
    for (const dir of index.value.keys()) if (dir) set.add(dir);
  } else {
    setFor(expanded).clear();
  }
}

interface Row {
  path: string;
  name: string;
  depth: number;
  dir: boolean;
  kind: ChangeKind | null;
  change?: FileChange;
}

const rows = computed<Row[]>(() => {
  const out: Row[] = [];
  const walk = (dir: string, depth: number) => {
    const node = index.value.get(dir);
    if (!node) return;
    for (const d of node.dirs) {
      out.push({ path: d, name: baseName(d), depth, dir: true, kind: dirKinds.value.get(d) ?? null });
      if (isOpen(d)) walk(d, depth + 1);
    }
    for (const f of node.files) {
      const change = changes.value.get(f);
      out.push({ path: f, name: baseName(f), depth, dir: false, kind: change?.kind ?? null, change });
    }
  };
  walk("", 0);
  return out;
});

function baseName(path: string) {
  return path.slice(path.lastIndexOf("/") + 1);
}

function open(row: Row) {
  if (row.dir) return toggleDir(row.path);
  openFile({
    root: props.status.root,
    path: row.path,
    origPath: row.change?.origPath ?? null,
    // Vindo da lista de alterados, o interesse é a mudança; da árvore toda, o arquivo
    // (sem modo: o openFile escolhe, ex.: Markdown abre renderizado).
    mode: filter.value === "changed" && row.change && row.kind !== "deleted" ? "changes" : undefined,
  });
}

// Revela o arquivo aberto na área principal: abre as pastas até ele.
watch(
  () => props.activePath,
  (path) => {
    if (!path || filter.value !== "all") return;
    const parts = path.split("/");
    const set = setFor(expanded);
    for (let i = 1; i < parts.length; i++) set.add(parts.slice(0, i).join("/"));
  },
  { immediate: true },
);
</script>

<template>
  <aside class="panel">
    <header>
      <div class="heading">
        <span>Explorador</span>
        <span class="repo" :title="status.root">{{ repoName(status.root) }}<template v-if="status.branch"> / {{ status.branch }}</template></span>
      </div>
      <div class="actions" @mousedown.prevent>
        <button title="Recolher pastas" @click="collapseAll">
          <svg viewBox="0 0 16 16" width="14" height="14"><path d="M4 7l4-4 4 4M4 13l4-4 4 4" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round" /></svg>
        </button>
        <button title="Atualizar" @click="$emit('refresh'), loadFiles()">
          <svg viewBox="0 0 16 16" width="14" height="14"><path d="M13 8a5 5 0 1 1-1.5-3.5M13 3v2.5h-2.5" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" /></svg>
        </button>
        <button title="Fechar (Ctrl+Shift+G)" @click="$emit('close')">
          <svg viewBox="0 0 16 16" width="14" height="14"><path d="M4 4l8 8M12 4l-8 8" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" /></svg>
        </button>
      </div>
    </header>

    <div class="filters" @mousedown.prevent>
      <button :class="{ on: filter === 'changed' }" @click="filter = 'changed'">
        Alterados <span class="count">{{ status.files.length }}</span>
      </button>
      <button :class="{ on: filter === 'all' }" @click="filter = 'all'">Todos</button>
    </div>

    <div v-if="filter === 'changed' && !status.files.length" class="empty">Nenhuma alteração. A árvore de trabalho está limpa.</div>

    <ul v-else class="tree">
      <li
        v-for="row in rows"
        :key="row.path"
        class="row"
        :class="[row.kind, { dir: row.dir, active: !row.dir && row.path === activePath }]"
        :style="{ paddingLeft: 8 + row.depth * 12 + 'px' }"
        :title="row.change?.origPath ? `${row.change.origPath} → ${row.path}` : row.path"
        @mousedown.prevent
        @click="open(row)"
      >
        <svg v-if="row.dir" class="chevron" :class="{ open: isOpen(row.path) }" viewBox="0 0 16 16" width="12" height="12"><path d="M6 4l4 4-4 4" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" /></svg>
        <span v-else class="chevron" />
        <span class="name">{{ row.name }}</span>
        <template v-if="row.change">
          <span v-if="!row.change.binary && row.change.additions" class="stat add">+{{ row.change.additions }}</span>
          <span v-if="!row.change.binary && row.change.deletions" class="stat del">−{{ row.change.deletions }}</span>
          <span class="badge" :title="BADGE[row.change.kind].label">{{ BADGE[row.change.kind].letter }}</span>
        </template>
        <span v-else-if="row.dir && row.kind" class="dot" />
      </li>
      <li v-if="truncated && filter === 'all'" class="note">Repositório muito grande: mostrando só os primeiros 50 mil arquivos.</li>
    </ul>
  </aside>
</template>

<style scoped>
.panel {
  display: flex;
  flex-direction: column;
  height: 100%;
  min-width: 0;
  background: var(--bg-elevated);
  font-size: 12.5px;
  user-select: none;
}
header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  height: 36px;
  padding: 0 8px 0 14px;
  border-bottom: 1px solid var(--border);
  flex-shrink: 0;
}
.heading {
  display: flex;
  align-items: center;
  gap: 8px;
  min-width: 0;
  font-weight: 600;
}
.repo {
  overflow: hidden;
  color: var(--fg-muted);
  font-weight: 400;
  font-size: 11.5px;
  white-space: nowrap;
  text-overflow: ellipsis;
}
.actions {
  display: flex;
  gap: 2px;
}
.actions button {
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
.actions button:hover {
  background: var(--hover);
  color: var(--fg);
}
.filters {
  display: flex;
  gap: 2px;
  margin: 8px 10px 4px;
  padding: 2px;
  border-radius: 6px;
  background: var(--bg);
  flex-shrink: 0;
}
.filters button {
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 6px;
  flex: 1;
  padding: 3px 0;
  border: 0;
  border-radius: 4px;
  background: none;
  color: var(--fg-muted);
  font: inherit;
  font-size: 11.5px;
  cursor: pointer;
}
.filters button.on {
  background: var(--active);
  color: var(--fg);
}
.count {
  padding: 0 5px;
  border-radius: 7px;
  background: var(--hover);
  font-size: 10.5px;
}
.empty {
  padding: 24px 16px;
  color: var(--fg-muted);
  text-align: center;
}
.tree {
  flex: 1;
  margin: 0;
  padding: 2px 0 8px;
  list-style: none;
  overflow-y: auto;
}
.row {
  display: flex;
  align-items: center;
  gap: 4px;
  height: 22px;
  padding-right: 10px;
  color: var(--fg);
  cursor: pointer;
}
.row:hover {
  background: var(--hover);
}
.row.active {
  background: var(--active);
}
.chevron {
  flex-shrink: 0;
  width: 12px;
  color: var(--fg-muted);
  transition: transform 0.12s;
}
.chevron.open {
  transform: rotate(90deg);
}
.name {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  white-space: nowrap;
  text-overflow: ellipsis;
}
.row.dir .name {
  color: var(--fg-muted);
}
/* Cores do VS Code (gitDecoration.*). */
.row.modified,
.row.renamed,
.row.dir.modified .name,
.row.dir.renamed .name,
.row.dir.deleted .name {
  color: #e2c08d;
}
.row.added,
.row.untracked,
.row.dir.added .name,
.row.dir.untracked .name {
  color: #73c991;
}
.row.deleted {
  color: #c74e39;
}
.row.deleted .name {
  text-decoration: line-through;
}
.row.conflict,
.row.dir.conflict .name {
  color: #e4676b;
}
.row.dir .name {
  font-weight: 400;
}
.badge {
  flex-shrink: 0;
  width: 12px;
  font-family: var(--font-mono);
  font-weight: 700;
  font-size: 11px;
  text-align: center;
}
.dot {
  flex-shrink: 0;
  width: 6px;
  height: 6px;
  margin-right: 3px;
  border-radius: 50%;
  background: currentColor;
  opacity: 0.8;
}
.row.dir .dot {
  color: inherit;
}
.row.dir.modified .dot,
.row.dir.renamed .dot,
.row.dir.deleted .dot {
  color: #e2c08d;
}
.row.dir.added .dot,
.row.dir.untracked .dot {
  color: #73c991;
}
.row.dir.conflict .dot {
  color: #e4676b;
}
.stat {
  flex-shrink: 0;
  font-family: var(--font-mono);
  font-size: 10.5px;
  opacity: 0.85;
}
.stat.add {
  color: var(--green);
}
.stat.del {
  color: var(--red);
}
.note {
  padding: 8px 14px;
  color: var(--fg-muted);
  font-size: 11.5px;
}
</style>
