<script setup lang="ts">
import { computed, reactive, watch } from "vue";
import { gitDiff, parseDiff, repoName, type DiffLine, type FileChange, type GitStatus } from "../lib/git";
import { openFile } from "../state/viewer";

const props = defineProps<{ status: GitStatus }>();
defineEmits<{ close: []; refresh: [] }>();

const BADGE: Record<FileChange["kind"], { letter: string; label: string }> = {
  added: { letter: "A", label: "Adicionado" },
  modified: { letter: "M", label: "Modificado" },
  deleted: { letter: "D", label: "Removido" },
  renamed: { letter: "R", label: "Renomeado" },
  untracked: { letter: "U", label: "Novo (não rastreado)" },
  conflict: { letter: "!", label: "Conflito" },
};

const expanded = reactive(new Set<string>());
const diffs = reactive(new Map<string, { key: string; lines: DiffLine[]; loading: boolean }>());

// Muda quando o arquivo muda: aí o diff aberto é recarregado.
const fileKey = (f: FileChange) => `${f.kind}|${f.staged}|${f.unstaged}|${f.additions}|${f.deletions}`;

async function load(file: FileChange) {
  const key = fileKey(file);
  const current = diffs.get(file.path);
  if (current?.key === key) return;
  diffs.set(file.path, { key, lines: current?.lines ?? [], loading: true });
  try {
    const text = await gitDiff(props.status.root, file);
    diffs.set(file.path, { key, lines: parseDiff(text), loading: false });
  } catch (err) {
    diffs.set(file.path, { key, lines: [{ kind: "meta", text: String(err) }], loading: false });
  }
}

function toggle(file: FileChange) {
  if (expanded.has(file.path)) {
    expanded.delete(file.path);
  } else {
    expanded.add(file.path);
    load(file);
  }
}

const anyExpanded = computed(() => expanded.size > 0);

function toggleAll() {
  if (anyExpanded.value) {
    expanded.clear();
  } else {
    props.status.files.forEach((f) => {
      expanded.add(f.path);
      load(f);
    });
  }
}

// Trocou de repositório: começa do zero.
watch(
  () => props.status.root,
  () => {
    expanded.clear();
    diffs.clear();
  },
);

// Atualizou o status: recarrega os diffs abertos que mudaram e esquece os que sumiram.
watch(
  () => props.status,
  (status) => {
    const paths = new Set(status.files.map((f) => f.path));
    for (const path of [...expanded]) if (!paths.has(path)) expanded.delete(path);
    for (const path of [...diffs.keys()]) if (!paths.has(path)) diffs.delete(path);
    status.files.filter((f) => expanded.has(f.path)).forEach(load);
  },
);

function split(path: string) {
  const i = path.lastIndexOf("/");
  return i < 0 ? { dir: "", name: path } : { dir: path.slice(0, i + 1), name: path.slice(i + 1) };
}
</script>

<template>
  <aside class="panel">
    <header>
      <div class="heading">
        <span>Alterações</span>
        <span class="count">{{ status.files.length }}</span>
        <span class="repo" :title="status.root">{{ repoName(status.root) }}<template v-if="status.branch"> / {{ status.branch }}</template></span>
      </div>
      <div class="actions" @mousedown.prevent>
        <button
          class="text-btn"
          :title="anyExpanded ? 'Recolher todos os arquivos' : 'Expandir todos os arquivos'"
          :disabled="!status.files.length"
          @click="toggleAll"
        >
          {{ anyExpanded ? "Recolher tudo" : "Expandir tudo" }}
        </button>
        <button title="Atualizar" @click="$emit('refresh')">
          <svg viewBox="0 0 16 16" width="14" height="14"><path d="M13 8a5 5 0 1 1-1.5-3.5M13 3v2.5h-2.5" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" /></svg>
        </button>
        <button title="Fechar (Ctrl+Shift+G)" @click="$emit('close')">
          <svg viewBox="0 0 16 16" width="14" height="14"><path d="M4 4l8 8M12 4l-8 8" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" /></svg>
        </button>
      </div>
    </header>

    <div v-if="!status.files.length" class="empty">Nenhuma alteração. A árvore de trabalho está limpa.</div>

    <ul v-else class="files">
      <li v-for="file in status.files" :key="file.path">
        <button
          class="file"
          :class="{ open: expanded.has(file.path) }"
          title="Clique para ver o diff. Clique duplo para abrir o arquivo."
          @mousedown.prevent
          @click="toggle(file)"
          @dblclick="file.kind !== 'deleted' && openFile({ root: status.root, path: file.path, origPath: file.origPath })"
        >
          <svg class="chevron" viewBox="0 0 16 16" width="12" height="12"><path d="M6 4l4 4-4 4" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" /></svg>
          <span class="badge" :class="file.kind" :title="BADGE[file.kind].label">{{ BADGE[file.kind].letter }}</span>
          <span class="path" :title="file.origPath ? `${file.origPath} → ${file.path}` : file.path">
            <span class="dir">{{ split(file.path).dir }}</span>{{ split(file.path).name }}
          </span>
          <span
            v-if="file.kind !== 'deleted'"
            class="open"
            role="button"
            title="Abrir no visualizador"
            @click.stop="openFile({ root: status.root, path: file.path, origPath: file.origPath })"
          >
            <svg viewBox="0 0 16 16" width="13" height="13"><path d="M9 3h4v4M13 3L7.5 8.5M11 9.5V12a1 1 0 0 1-1 1H4a1 1 0 0 1-1-1V6a1 1 0 0 1 1-1h2.5" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round" /></svg>
          </span>
          <span v-if="file.binary" class="stat">bin</span>
          <template v-else>
            <span v-if="file.additions" class="stat add">+{{ file.additions }}</span>
            <span v-if="file.deletions" class="stat del">−{{ file.deletions }}</span>
          </template>
        </button>

        <div v-if="expanded.has(file.path)" class="diff">
          <div v-if="diffs.get(file.path)?.loading && !diffs.get(file.path)?.lines.length" class="note">Carregando…</div>
          <div v-else-if="!diffs.get(file.path)?.lines.length" class="note">Sem diferenças de texto.</div>
          <div v-else class="lines">
            <div v-for="(line, i) in diffs.get(file.path)!.lines" :key="i" class="line" :class="line.kind">
              <span class="no">{{ line.oldNo ?? "" }}</span>
              <span class="no">{{ line.newNo ?? "" }}</span>
              <span class="sign">{{ line.kind === "add" ? "+" : line.kind === "del" ? "−" : "" }}</span>
              <span class="code">{{ line.text }}</span>
            </div>
          </div>
          <button class="collapse" @mousedown.prevent @click="toggle(file)">
            <svg viewBox="0 0 16 16" width="12" height="12"><path d="M4 10l4-4 4 4" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" /></svg>
            Recolher {{ split(file.path).name }}
          </button>
        </div>
      </li>
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
  font-weight: 600;
}
.count {
  padding: 0 6px;
  border-radius: 8px;
  background: var(--hover);
  color: var(--fg-muted);
  font-size: 11px;
  font-weight: 500;
}
.repo {
  overflow: hidden;
  color: var(--fg-muted);
  font-weight: 400;
  font-size: 11.5px;
  white-space: nowrap;
  text-overflow: ellipsis;
}
.heading {
  min-width: 0;
}
.open {
  display: grid;
  place-items: center;
  flex-shrink: 0;
  width: 20px;
  height: 20px;
  border-radius: 4px;
  color: var(--fg-muted);
  opacity: 0;
}
.file:hover .open {
  opacity: 1;
}
.open:hover {
  background: var(--active);
  color: var(--fg);
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
.actions button:hover:not(:disabled) {
  background: var(--hover);
  color: var(--fg);
}
.actions button:disabled {
  opacity: 0.4;
  cursor: default;
}
.empty {
  padding: 24px 16px;
  color: var(--fg-muted);
  text-align: center;
}
.files {
  flex: 1;
  margin: 0;
  padding: 4px 0;
  list-style: none;
  overflow-y: auto;
}
.file {
  position: sticky;
  top: 0;
  z-index: 1;
  display: flex;
  align-items: center;
  gap: 8px;
  width: 100%;
  padding: 5px 12px 5px 8px;
  border: 0;
  background: var(--bg-elevated);
  color: var(--fg);
  font: inherit;
  text-align: left;
  cursor: pointer;
}
.file:hover {
  background: color-mix(in srgb, var(--fg) 6%, var(--bg-elevated));
}
.file.open {
  box-shadow: 0 1px 0 var(--border);
}
.collapse {
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 6px;
  width: 100%;
  padding: 5px;
  border: 0;
  border-top: 1px solid var(--border);
  background: none;
  color: var(--fg-muted);
  font: inherit;
  font-size: 11.5px;
  cursor: pointer;
}
.collapse:hover {
  background: var(--hover);
  color: var(--fg);
}
.actions .text-btn {
  width: auto;
  padding: 0 8px;
  font: inherit;
  font-size: 11.5px;
}
.chevron {
  flex-shrink: 0;
  color: var(--fg-muted);
  transition: transform 0.12s;
}
.file.open .chevron {
  transform: rotate(90deg);
}
.badge {
  flex-shrink: 0;
  width: 16px;
  font-family: var(--font-mono);
  font-weight: 700;
  font-size: 11px;
  text-align: center;
}
.badge.added,
.badge.untracked {
  color: var(--green);
}
.badge.modified {
  color: var(--yellow);
}
.badge.deleted,
.badge.conflict {
  color: var(--red);
}
.badge.renamed {
  color: var(--accent);
}
.path {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  white-space: nowrap;
  text-overflow: ellipsis;
}
.dir {
  color: var(--fg-muted);
}
.stat {
  flex-shrink: 0;
  font-family: var(--font-mono);
  font-size: 11.5px;
  color: var(--fg-muted);
}
.stat.add {
  color: var(--green);
}
.stat.del {
  color: var(--red);
}
.diff {
  margin: 2px 8px 8px;
  border: 1px solid var(--border);
  border-radius: 6px;
  background: var(--bg);
  overflow-x: auto;
}
.note {
  padding: 10px 12px;
  color: var(--fg-muted);
}
.lines {
  display: table;
  min-width: 100%;
  font-family: var(--font-mono);
  font-size: 12px;
  line-height: 1.55;
}
.line {
  display: table-row;
}
.line > span {
  display: table-cell;
  white-space: pre;
}
.no {
  min-width: 3ch;
  padding: 0 6px;
  color: var(--fg-muted);
  text-align: right;
  opacity: 0.6;
  user-select: none;
}
.sign {
  width: 1ch;
  padding-right: 6px;
  user-select: none;
}
.code {
  padding-right: 12px;
  width: 100%;
}
.line.add {
  background: var(--add-bg);
}
.line.add .sign {
  color: var(--green);
}
.line.del {
  background: var(--del-bg);
}
.line.del .sign {
  color: var(--red);
}
.line.hunk {
  background: var(--hunk-bg);
  color: var(--accent);
}
.line.hunk .code {
  padding: 2px 12px 2px 0;
}
.line.meta {
  color: var(--fg-muted);
  font-style: italic;
}
</style>
