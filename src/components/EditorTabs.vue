<script setup lang="ts">
import { computed, ref } from "vue";
import type { ChangeKind, GitStatus } from "../lib/git";
import { closeFile, openFile, showTerminal } from "../state/editor";
import { removeComment, where } from "../state/review";
import { canPreview, tabTitle, type FileMode, type OpenFile, type ReviewComment, type Tab } from "../state/tabs";

/** Abas da área principal: o terminal e os arquivos abertos, como no VS Code. */
const props = defineProps<{ tab: Tab; home: string | null; status: GitStatus | null }>();
const emit = defineEmits<{ sendReview: [] }>();

const reviewOpen = ref(false);

function goToComment(c: ReviewComment) {
  openFile({ root: c.root, path: c.path, origPath: null, line: c.removed?.at ?? c.from }, props.tab);
  reviewOpen.value = false;
}

function send() {
  reviewOpen.value = false;
  emit("sendReview");
}

function discard() {
  props.tab.review.splice(0);
  reviewOpen.value = false;
}

const LETTER: Record<ChangeKind, string> = {
  added: "A",
  modified: "M",
  deleted: "D",
  renamed: "R",
  untracked: "U",
  conflict: "!",
};

function change(file: OpenFile) {
  if (props.status?.root !== file.root) return null;
  return props.status.files.find((f) => f.path === file.path) ?? null;
}

const fileName = (path: string) => path.slice(path.lastIndexOf("/") + 1);

/** Nomes repetidos (ex.: dois index.ts) ganham a pasta, pra diferenciar. */
const labels = computed(() =>
  props.tab.files.map((f) => {
    const name = fileName(f.path);
    const twins = props.tab.files.filter((o) => fileName(o.path) === name).length > 1;
    const dir = f.path.slice(0, f.path.lastIndexOf("/") + 1);
    return { name, dir: twins ? dir.split("/").slice(-2, -1)[0] ?? "" : "" };
  }),
);

const shown = computed(() => (props.tab.view >= 0 ? props.tab.files[props.tab.view] : null));
/** O toggle só faz sentido em arquivo com alteração (ou já no modo alterações). */
/** Modos que fazem sentido pro arquivo mostrado; o toggle aparece com 2 ou mais. */
const modes = computed(() => {
  const file = shown.value;
  if (!file) return [];
  const list: { mode: FileMode; label: string; title: string }[] = [];
  if (canPreview(file.path)) list.push({ mode: "preview", label: "Visualizar", title: "Markdown/HTML renderizado" });
  list.push({ mode: "file", label: canPreview(file.path) ? "Código" : "Arquivo", title: "Arquivo atual, sem marcações" });
  if (change(file) || file.mode === "changes") {
    list.push({ mode: "changes", label: "Alterações", title: "Arquivo com as alterações em relação ao último commit" });
  }
  return list;
});
</script>

<template>
  <div class="editor-tabs" @mousedown.prevent>
    <div class="list">
      <div class="etab" :class="{ active: tab.view === -1 }" title="Terminal" @click="showTerminal(tab)">
        <svg viewBox="0 0 16 16" width="13" height="13"><path d="M3 4.5l3.5 3.5L3 11.5M8.5 12h4.5" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" /></svg>
        <span class="name">{{ tabTitle(tab, home) }}</span>
      </div>

      <div
        v-for="(file, i) in tab.files"
        :key="file.root + '/' + file.path"
        class="etab"
        :class="[{ active: tab.view === i }, change(file)?.kind]"
        :title="file.path"
        @click="tab.view = i"
        @mousedown.middle.prevent="closeFile(tab, i)"
      >
        <span class="name">{{ labels[i].name }}</span>
        <span v-if="labels[i].dir" class="dir">{{ labels[i].dir }}</span>
        <span v-if="file.mode === 'changes'" class="mode" title="Mostrando as alterações">±</span>
        <span v-if="change(file)" class="letter">{{ LETTER[change(file)!.kind] }}</span>
        <button class="x" title="Fechar (clique do meio)" @click.stop="closeFile(tab, i)">
          <svg viewBox="0 0 16 16" width="11" height="11"><path d="M4 4l8 8M12 4l-8 8" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" /></svg>
        </button>
      </div>
    </div>

    <div v-if="tab.review.length" class="review">
      <button class="review-btn" :class="{ open: reviewOpen }" title="Comentários pra mandar pro Claude desta aba" @click="reviewOpen = !reviewOpen">
        <svg viewBox="0 0 16 16" width="13" height="13"><path d="M2.5 3.5h11v7h-6l-3 2.5v-2.5h-2z" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linejoin="round" /></svg>
        Revisão · {{ tab.review.length }}
      </button>
      <div v-if="reviewOpen" class="review-menu">
        <ul>
          <li v-for="c in tab.review" :key="c.id" @click="goToComment(c)">
            <div class="where">{{ where(c, c.path) }}</div>
            <div class="text">{{ c.text }}</div>
            <button class="x" title="Remover" @click.stop="removeComment(tab, c.id)">
              <svg viewBox="0 0 16 16" width="11" height="11"><path d="M4 4l8 8M12 4l-8 8" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" /></svg>
            </button>
          </li>
        </ul>
        <div class="review-foot">
          <button @click="discard">Descartar</button>
          <button class="primary" title="Cola a revisão no prompt do Claude desta aba (sem Enter)" @click="send">Enviar pro Claude</button>
        </div>
      </div>
    </div>

    <div v-if="shown && modes.length > 1" class="modes">
      <button v-for="m in modes" :key="m.mode" :class="{ on: shown.mode === m.mode }" :title="m.title" @click="shown.mode = m.mode">
        {{ m.label }}
      </button>
    </div>
  </div>
</template>

<style scoped>
.editor-tabs {
  display: flex;
  align-items: center;
  gap: 6px;
  height: 34px;
  padding: 0 6px 0 4px;
  flex-shrink: 0;
  background: var(--bg-elevated);
  border-bottom: 1px solid var(--border);
  user-select: none;
}
.list {
  display: flex;
  align-items: stretch;
  flex: 1;
  min-width: 0;
  height: 100%;
  overflow-x: auto;
  scrollbar-width: none;
}
.etab {
  position: relative;
  display: flex;
  align-items: center;
  gap: 6px;
  flex-shrink: 0;
  max-width: 240px;
  padding: 0 6px 0 12px;
  color: var(--fg-muted);
  font-size: 12px;
  cursor: pointer;
}
.etab:first-child {
  padding-right: 12px;
}
.etab:hover {
  background: var(--hover);
}
.etab.active {
  background: var(--bg);
  color: var(--fg);
}
.etab.active::after {
  content: "";
  position: absolute;
  left: 0;
  right: 0;
  top: 0;
  height: 2px;
  background: var(--accent);
}
.name {
  overflow: hidden;
  white-space: nowrap;
  text-overflow: ellipsis;
}
.dir {
  color: var(--fg-muted);
  font-size: 11px;
  opacity: 0.8;
}
/* Cores do VS Code (gitDecoration.*). */
.etab.modified .name,
.etab.renamed .name,
.etab.modified .letter,
.etab.renamed .letter {
  color: #e2c08d;
}
.etab.added .name,
.etab.untracked .name,
.etab.added .letter,
.etab.untracked .letter {
  color: #73c991;
}
.etab.deleted .name,
.etab.deleted .letter {
  color: #c74e39;
}
.etab.deleted .name {
  text-decoration: line-through;
}
.etab.conflict .name,
.etab.conflict .letter {
  color: #e4676b;
}
.letter {
  font-family: var(--font-mono);
  font-size: 11px;
  font-weight: 700;
}
.mode {
  color: var(--accent);
  font-weight: 700;
}
.x {
  display: grid;
  place-items: center;
  width: 18px;
  height: 18px;
  border: 0;
  border-radius: 4px;
  background: none;
  color: var(--fg-muted);
  cursor: pointer;
  opacity: 0;
}
.etab:hover .x,
.etab.active .x {
  opacity: 1;
}
.x:hover {
  background: var(--active);
  color: var(--fg);
}
.review {
  position: relative;
  flex-shrink: 0;
}
.review-btn {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 3px 9px;
  border: 1px solid color-mix(in srgb, var(--yellow) 45%, transparent);
  border-radius: 6px;
  background: color-mix(in srgb, var(--yellow) 10%, transparent);
  color: var(--yellow);
  font: inherit;
  font-size: 11.5px;
  cursor: pointer;
}
.review-btn:hover,
.review-btn.open {
  background: color-mix(in srgb, var(--yellow) 20%, transparent);
}
.review-menu {
  position: absolute;
  top: calc(100% + 6px);
  right: 0;
  z-index: 50;
  width: min(420px, 80vw);
  border: 1px solid var(--border);
  border-radius: 8px;
  background: var(--bg-elevated);
  box-shadow: 0 12px 32px rgb(0 0 0 / 0.45);
  font-size: 12px;
}
.review-menu ul {
  max-height: 320px;
  margin: 0;
  padding: 6px;
  list-style: none;
  overflow-y: auto;
}
.review-menu li {
  position: relative;
  padding: 6px 28px 6px 8px;
  border-radius: 6px;
  cursor: pointer;
}
.review-menu li:hover {
  background: var(--hover);
}
.review-menu li:hover .x {
  opacity: 1;
}
.review-menu .x {
  position: absolute;
  top: 6px;
  right: 4px;
}
.where {
  color: var(--fg-muted);
  font-family: var(--font-mono);
  font-size: 11px;
}
.text {
  display: -webkit-box;
  -webkit-line-clamp: 2;
  -webkit-box-orient: vertical;
  overflow: hidden;
  color: var(--fg);
}
.review-foot {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
  padding: 8px;
  border-top: 1px solid var(--border);
}
.review-foot button {
  padding: 4px 10px;
  border: 1px solid var(--border);
  border-radius: 6px;
  background: none;
  color: var(--fg);
  font: inherit;
  font-size: 12px;
  cursor: pointer;
}
.review-foot button.primary {
  border-color: var(--accent);
  background: color-mix(in srgb, var(--accent) 20%, transparent);
}
.modes {
  display: flex;
  flex-shrink: 0;
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
</style>
