<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from "vue";
import { basicSetup } from "codemirror";
import { Compartment, EditorState, RangeSetBuilder, type Extension } from "@codemirror/state";
import { Decoration, EditorView, WidgetType } from "@codemirror/view";
import { LanguageDescription, syntaxHighlighting } from "@codemirror/language";
import { languages } from "@codemirror/language-data";
import { getChunks, getOriginalDoc, unifiedMergeView } from "@codemirror/merge";
import { oneDarkHighlightStyle } from "@codemirror/theme-one-dark";
import { marked } from "marked";
import DOMPurify from "dompurify";
import { gitShowHead, readTextFile, type GitStatus } from "../lib/git";
import { openUrl } from "../lib/github";
import { findDefinitions, gotoDefinition, resolveImportPath, type GotoTarget } from "../lib/goto";
import { openFile } from "../state/editor";
import ContextMenu, { type MenuItem } from "./ContextMenu.vue";
import { addComment, commentsFor, lines, removeComment } from "../state/review";
import type { OpenFile, ReviewComment, Tab } from "../state/tabs";

/** Um arquivo aberto na área principal: atual ou com as alterações marcadas. */
const props = defineProps<{ tab: Tab; file: OpenFile; status: GitStatus | null }>();

const host = ref<HTMLDivElement>();
const note = ref<string | null>(null);
/** Markdown renderizado (já sanitizado) ou HTML do arquivo, no modo preview. */
const preview = ref<{ kind: "md" | "html"; html: string } | null>(null);
let view: EditorView | undefined;

const active = computed(() => props.file);

// ---- comentários de revisão ----
/** Trecho apagado selecionado: linha atual onde ele ficava e o código removido. */
type Removed = { at: number; code: string };
/** Linhas selecionadas no editor (o botão "Comentar" aparece com seleção). */
const selection = ref<{ from: number; to: number; removed?: Removed } | null>(null);
/** Caixa de comentário aberta pras linhas selecionadas. */
const draft = ref<{ from: number; to: number; removed?: Removed; text: string } | null>(null);
const draftBox = ref<HTMLTextAreaElement>();
const comments = computed(() => commentsFor(props.tab, props.file.root, props.file.path));
const commentsSlot = new Compartment();

const trackSelection = EditorView.updateListener.of((u) => {
  if (!u.selectionSet && !u.docChanged) return;
  const sel = u.state.selection.main;
  if (sel.empty) {
    selection.value = removedSelection();
    return;
  }
  // Seleção que termina no começo de uma linha não inclui essa linha.
  const end = sel.to > sel.from && u.state.doc.lineAt(sel.to).from === sel.to ? sel.to - 1 : sel.to;
  selection.value = { from: u.state.doc.lineAt(sel.from).number, to: u.state.doc.lineAt(end).number };
});

/**
 * Seleção dentro de um trecho apagado (modo alterações). Esse trecho é um widget
 * do merge view, fora do documento: o CodeMirror não vê a seleção, então ela é
 * lida do DOM e convertida pras linhas do último commit.
 */
function removedSelection(): typeof selection.value {
  const sel = document.getSelection();
  if (!view || !sel || sel.isCollapsed || !sel.anchorNode || !sel.focusNode) return null;
  const chunkOf = (n: Node) => (n instanceof Element ? n : n.parentElement)?.closest(".cm-deletedChunk") ?? null;
  const dom = chunkOf(sel.anchorNode);
  if (!dom || dom !== chunkOf(sel.focusNode) || !view.dom.contains(dom)) return null;
  const rows = [...dom.querySelectorAll(".cm-deletedLine")];
  const picked = rows.map((row, i) => (sel.containsNode(row, true) ? i : -1)).filter((i) => i >= 0);
  if (!picked.length) return null;
  const pos = view.posAtDOM(dom);
  const chunk = getChunks(view.state)?.chunks.find((c) => c.fromB === pos);
  if (!chunk) return null;
  const first = getOriginalDoc(view.state).lineAt(chunk.fromA).number;
  const [i0, i1] = [picked[0], picked[picked.length - 1]];
  const code = rows.slice(i0, i1 + 1).map((r) => r.textContent ?? "").join("\n");
  return { from: first + i0, to: first + i1, removed: { at: view.state.doc.lineAt(pos).number, code } };
}

// O CodeMirror nem sempre gera update quando a seleção fica só no widget.
function onSelectionChange() {
  const removed = removedSelection();
  if (removed) selection.value = removed;
  else if (selection.value?.removed) selection.value = null;
}

class CommentWidget extends WidgetType {
  constructor(readonly comment: ReviewComment) {
    super();
  }
  eq(other: CommentWidget) {
    return other.comment.id === this.comment.id && other.comment.text === this.comment.text;
  }
  toDOM() {
    const box = document.createElement("div");
    box.className = "cm-review-comment";
    const text = document.createElement("span");
    text.textContent = this.comment.text;
    const remove = document.createElement("button");
    remove.textContent = "Remover";
    remove.title = "Tira o comentário da revisão";
    remove.onmousedown = (e) => e.preventDefault();
    remove.onclick = () => removeComment(props.tab, this.comment.id);
    box.append(text, remove);
    return box;
  }
  ignoreEvent() {
    return true;
  }
}

/** Linhas comentadas marcadas e o texto do comentário logo abaixo do trecho. */
function commentDecorations(): Extension {
  if (!view) return [];
  const doc = view.state.doc;
  const marks: { pos: number; deco: Decoration }[] = [];
  for (const c of comments.value) {
    if (c.removed) {
      // Fica logo acima do trecho apagado, que é desenhado antes da linha `at`.
      const at = Math.min(c.removed.at, doc.lines);
      const deco = Decoration.widget({ widget: new CommentWidget(c), block: true, side: at > 1 ? 1 : -1 });
      marks.push({ pos: at > 1 ? doc.line(at - 1).to : 0, deco });
      continue;
    }
    const from = Math.min(c.from, doc.lines);
    const to = Math.min(c.to, doc.lines);
    for (let n = from; n <= to; n++) marks.push({ pos: doc.line(n).from, deco: Decoration.line({ class: "cm-commented" }) });
    marks.push({ pos: doc.line(to).to, deco: Decoration.widget({ widget: new CommentWidget(c), block: true, side: 1 }) });
  }
  marks.sort((a, b) => a.pos - b.pos || a.deco.startSide - b.deco.startSide);
  const builder = new RangeSetBuilder<Decoration>();
  for (const m of marks) builder.add(m.pos, m.pos, m.deco);
  return EditorView.decorations.of(builder.finish());
}

function refreshComments() {
  view?.dispatch({ effects: commentsSlot.reconfigure(commentDecorations()) });
}
watch(comments, refreshComments, { deep: true });

function startComment() {
  if (!selection.value) return;
  draft.value = { ...selection.value, text: "" };
  requestAnimationFrame(() => draftBox.value?.focus());
}

function saveComment() {
  const d = draft.value;
  if (!d || !view || !d.text.trim()) return;
  const doc = view.state.doc;
  const base = { root: props.file.root, path: props.file.path, from: d.from, to: d.to, text: d.text.trim() };
  if (d.removed) addComment(props.tab, { ...base, code: d.removed.code, removed: { at: d.removed.at } });
  else addComment(props.tab, { ...base, code: doc.sliceString(doc.line(d.from).from, doc.line(Math.min(d.to, doc.lines)).to) });
  draft.value = null;
  document.getSelection()?.removeAllRanges();
  selection.value = null;
  view.dispatch({ selection: { anchor: view.state.selection.main.head } });
}

function onDraftKey(e: KeyboardEvent) {
  if (e.key === "Enter" && (e.ctrlKey || e.metaKey)) {
    e.preventDefault();
    saveComment();
  } else if (e.key === "Escape") {
    e.preventDefault();
    draft.value = null;
  }
}

// ---- Ctrl+clique: ir pra definição ----
/** Várias definições possíveis: lista pra escolher, onde foi o clique. */
const gotoMenu = ref<{ x: number; y: number; items: MenuItem[] } | null>(null);
/** Aviso rápido quando não achou nada. */
const gotoMiss = ref<string | null>(null);
let missTimer: ReturnType<typeof setTimeout> | undefined;

function miss(text: string) {
  gotoMiss.value = text;
  clearTimeout(missTimer);
  missTimer = setTimeout(() => (gotoMiss.value = null), 2500);
}

function jump(root: string, path: string, line: number) {
  if (root === props.file.root && path === props.file.path) goToLine(line);
  else openFile({ root, path, origPath: null, line }, props.tab);
}

async function onGoto(target: GotoTarget, e: MouseEvent) {
  const { root, path } = props.file;
  if (target.kind === "import") {
    const full = `${root}/${path}`;
    const abs = await resolveImportPath(full.slice(0, full.lastIndexOf("/")), target.spec).catch(() => null);
    if (!abs) return miss(`Não achei o arquivo de "${target.spec}".`);
    // Fora da raiz atual (ex.: outro pacote): a pasta do arquivo vira a raiz.
    if (abs.startsWith(root + "/")) return openFile({ root, path: abs.slice(root.length + 1), origPath: null }, props.tab);
    const cut = abs.lastIndexOf("/");
    return openFile({ root: abs.slice(0, cut) || "/", path: abs.slice(cut + 1), origPath: null }, props.tab);
  }
  const defs = await findDefinitions(root, path, target.name).catch(() => []);
  if (!defs.length) return miss(`Não achei a definição de ${target.name}.`);
  if (defs.length === 1) return jump(root, defs[0].path, defs[0].line);
  gotoMenu.value = {
    x: e.clientX,
    y: e.clientY + 12,
    items: defs.map((d) => ({ label: `${d.path}:${d.line}  ${d.text}`, action: () => jump(root, d.path, d.line) })),
  };
}

const theme = EditorView.theme(
  {
    "&": { height: "100%", backgroundColor: "var(--bg)", color: "var(--fg)", fontSize: "13px" },
    "&.cm-focused": { outline: "none" },
    ".cm-scroller": { fontFamily: "var(--font-mono)", lineHeight: "1.55" },
    ".cm-gutters": { backgroundColor: "var(--bg)", color: "var(--fg-muted)", border: "none" },
    ".cm-activeLine": { backgroundColor: "rgb(255 255 255 / 0.03)" },
    ".cm-activeLineGutter": { backgroundColor: "transparent", color: "var(--fg)" },
    ".cm-cursor": { borderLeftColor: "var(--accent)" },
    "&.cm-focused > .cm-scroller > .cm-selectionLayer .cm-selectionBackground, .cm-selectionBackground": {
      backgroundColor: "#2e3a5c",
    },
    ".cm-panels": { backgroundColor: "var(--bg-elevated)", color: "var(--fg)", borderColor: "var(--border)" },
    ".cm-searchMatch": { backgroundColor: "rgb(224 175 104 / 0.3)" },
    // Alterações em relação ao último commit.
    "&.cm-merge-b .cm-changedLine": { backgroundColor: "var(--add-bg) !important" },
    "&.cm-merge-b .cm-changedText": { background: "rgb(158 206 106 / 0.28) !important" },
    ".cm-deletedChunk": { backgroundColor: "var(--del-bg) !important" },
    ".cm-deletedChunk .cm-deletedText": { background: "rgb(247 118 142 / 0.3) !important" },
    "&.cm-merge-b .cm-changedLineGutter": { background: "var(--green) !important" },
    ".cm-deletedLineGutter": { background: "var(--red) !important" },
    ".cm-collapsedLines": { background: "var(--bg-elevated) !important", color: "var(--fg-muted)" },
    // Trechos com comentário de revisão.
    ".cm-commented": { boxShadow: "inset 3px 0 0 var(--yellow)", backgroundColor: "rgb(224 175 104 / 0.08)" },
    ".cm-review-comment": {
      display: "flex",
      alignItems: "flex-start",
      gap: "10px",
      margin: "4px 12px 6px 4px",
      padding: "6px 10px",
      border: "1px solid rgb(224 175 104 / 0.4)",
      borderRadius: "6px",
      background: "var(--bg-elevated)",
      fontFamily: "var(--font-ui, system-ui)",
      fontSize: "12.5px",
      whiteSpace: "pre-wrap",
    },
    ".cm-review-comment span": { flex: "1" },
    ".cm-review-comment button": {
      border: "0",
      background: "none",
      color: "var(--fg-muted)",
      font: "inherit",
      fontSize: "11px",
      cursor: "pointer",
    },
    ".cm-review-comment button:hover": { color: "var(--red)" },
  },
  { dark: true },
);

async function languageFor(path: string): Promise<Extension> {
  const desc = LanguageDescription.matchFilename(languages, path.split("/").pop() ?? path);
  return desc ? await desc.load() : [];
}

// Chave do arquivo no status do git: muda quando o arquivo muda e aí recarrega.
const changeKey = computed(() => {
  const file = active.value;
  const change = props.status?.root === file.root && props.status.files.find((f) => f.path === file.path);
  return change ? `${change.additions}|${change.deletions}|${change.kind}` : "clean";
});

let loadId = 0;

async function load() {
  const file = active.value;
  if (!view) return;
  const id = ++loadId;

  const [current, original, language] = await Promise.all([
    readTextFile(`${file.root}/${file.path}`).catch(() => null),
    gitShowHead(file.root, file.origPath ?? file.path).catch(() => null),
    languageFor(file.path),
  ]);
  if (id !== loadId) return; // trocou de arquivo enquanto carregava

  if (current?.binary || original?.binary) {
    note.value = "Arquivo binário: não dá pra visualizar.";
  } else if (!current && !original) {
    note.value = "Não foi possível abrir o arquivo.";
  } else {
    note.value = !current ? "Arquivo removido. Mostrando a versão do último commit." : null;
  }

  const text = current?.text ?? "";

  if (file.mode === "preview") {
    preview.value = /\.html?$/i.test(file.path)
      ? { kind: "html", html: text }
      : // Markdown vira HTML e passa pelo DOMPurify: o arquivo pode vir de qualquer lugar.
        { kind: "md", html: DOMPurify.sanitize(marked.parse(text, { async: false })) };
    return;
  }
  preview.value = null;

  const extensions: Extension[] = [
    basicSetup,
    EditorState.readOnly.of(true),
    syntaxHighlighting(oneDarkHighlightStyle),
    theme,
    language,
    trackSelection,
    commentsSlot.of([]),
    gotoDefinition(onGoto),
  ];
  if (file.mode === "changes") {
    extensions.push(
      unifiedMergeView({
        original: original?.text ?? "",
        mergeControls: false,
        syntaxHighlightDeletions: true,
        collapseUnchanged: { margin: 4, minSize: 12 },
      }),
    );
  }

  // Recarregar o mesmo arquivo mantém a rolagem.
  const sameFile = view.dom.dataset.file === `${file.root}/${file.path}`;
  const top = view.scrollDOM.scrollTop;
  view.setState(EditorState.create({ doc: text, extensions }));
  view.dom.dataset.file = `${file.root}/${file.path}`;
  selection.value = null;
  refreshComments();
  if (file.line) goToLine(file.line);
  else if (sameFile) requestAnimationFrame(() => view && (view.scrollDOM.scrollTop = top));
}

/** Mostra a linha pedida (veio de um caminho `arquivo:42` no terminal). */
function goToLine(line: number) {
  if (!view) return;
  const doc = view.state.doc;
  const pos = doc.line(Math.min(Math.max(line, 1), doc.lines)).from;
  view.dispatch({ selection: { anchor: pos }, effects: EditorView.scrollIntoView(pos, { y: "center" }) });
}

/** Links no Markdown abrem no navegador; nunca navegam a janela do term7. */
function onPreviewClick(e: MouseEvent) {
  const a = (e.target as HTMLElement).closest("a");
  if (!a) return;
  e.preventDefault();
  const href = a.getAttribute("href") ?? "";
  if (/^https?:\/\//.test(href)) openUrl(href).catch(() => {});
}

watch([() => props.file.path, () => props.file.root, () => props.file.mode, changeKey], load);
watch(
  () => props.file.line,
  (line) => line && props.file.mode !== "preview" && goToLine(line),
);

onMounted(() => {
  view = new EditorView({ parent: host.value! });
  document.addEventListener("selectionchange", onSelectionChange);
  load();
});

onBeforeUnmount(() => {
  document.removeEventListener("selectionchange", onSelectionChange);
  clearTimeout(missTimer);
  view?.destroy();
});

</script>

<template>
  <section class="viewer">
    <!-- Fora do repositório (ex.: /tmp/...): mostra o caminho completo. -->
    <div class="path">{{ status?.root === file.root ? file.path : `${file.root}/${file.path}`.replace(/^\/\//, "/") }}</div>
    <div v-if="note" class="note">{{ note }}</div>
    <div v-show="!preview" class="editor-wrap">
      <div ref="host" class="editor" />
      <div v-if="gotoMiss" class="goto-miss">{{ gotoMiss }}</div>
      <button v-if="selection && !draft" class="comment-btn" title="Comentar o trecho pra mandar pro Claude desta aba" @mousedown.prevent @click="startComment">
        <svg viewBox="0 0 16 16" width="13" height="13"><path d="M2.5 3.5h11v7h-6l-3 2.5v-2.5h-2z" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linejoin="round" /></svg>
        Comentar {{ selection.removed ? "trecho removido" : selection.from === selection.to ? "linha" : "linhas" }} {{ selection.removed ? "" : lines(selection) }}
      </button>
      <div v-if="draft" class="draft">
        <div class="draft-head">Comentário · {{ draft.removed ? `trecho removido (linhas ${lines(draft)} no último commit)` : `${draft.from === draft.to ? "linha" : "linhas"} ${lines(draft)}` }}</div>
        <textarea ref="draftBox" v-model="draft.text" rows="3" placeholder="O que o Claude deve fazer aqui?" @keydown="onDraftKey" />
        <div class="draft-foot">
          <span class="hint">Ctrl+Enter adiciona · Esc cancela</span>
          <button @click="draft = null">Cancelar</button>
          <button class="primary" :disabled="!draft.text.trim()" @click="saveComment">Adicionar à revisão</button>
        </div>
      </div>
    </div>
    <!-- eslint-disable-next-line vue/no-v-html -- sanitizado com DOMPurify -->
    <div v-if="preview?.kind === 'md'" class="markdown" @click="onPreviewClick" v-html="preview.html" />
    <!-- HTML num iframe sem scripts e sem acesso ao app (sandbox vazio). -->
    <iframe v-else-if="preview?.kind === 'html'" class="html" sandbox="" :srcdoc="preview.html" />
    <ContextMenu v-if="gotoMenu" v-bind="gotoMenu" class="goto-menu" @close="gotoMenu = null" />
  </section>
</template>

<style scoped>
.viewer {
  display: flex;
  flex-direction: column;
  height: 100%;
  min-width: 0;
  background: var(--bg);
}
.path {
  padding: 4px 12px;
  flex-shrink: 0;
  color: var(--fg-muted);
  font-family: var(--font-mono);
  font-size: 11.5px;
  border-bottom: 1px solid var(--border);
}
.editor-wrap {
  position: relative;
  flex: 1;
  min-height: 0;
  display: flex;
  flex-direction: column;
}
.comment-btn {
  position: absolute;
  top: 8px;
  right: 16px;
  z-index: 5;
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 5px 10px;
  border: 1px solid color-mix(in srgb, var(--yellow) 50%, transparent);
  border-radius: 6px;
  background: var(--bg-elevated);
  color: var(--yellow);
  font: inherit;
  font-size: 12px;
  cursor: pointer;
  box-shadow: 0 4px 14px rgb(0 0 0 / 0.35);
}
.comment-btn:hover {
  background: color-mix(in srgb, var(--yellow) 14%, var(--bg-elevated));
}
.goto-miss {
  position: absolute;
  bottom: 12px;
  left: 50%;
  transform: translateX(-50%);
  z-index: 5;
  padding: 5px 12px;
  border: 1px solid var(--border);
  border-radius: 6px;
  background: var(--bg-elevated);
  color: var(--fg-muted);
  font-size: 12px;
  box-shadow: 0 4px 14px rgb(0 0 0 / 0.35);
  pointer-events: none;
}
.goto-menu {
  font-family: var(--font-mono);
  font-size: 12px;
  max-width: min(720px, 90vw);
}
.goto-menu :deep(.item) {
  display: block;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.draft {
  flex-shrink: 0;
  display: flex;
  flex-direction: column;
  gap: 6px;
  padding: 8px 12px 10px;
  border-top: 1px solid var(--border);
  background: var(--bg-elevated);
}
.draft-head {
  color: var(--yellow);
  font-size: 12px;
}
.draft textarea {
  resize: vertical;
  padding: 6px 8px;
  border: 1px solid var(--border);
  border-radius: 6px;
  background: var(--bg);
  color: var(--fg);
  font: inherit;
  font-size: 13px;
  outline: none;
}
.draft textarea:focus {
  border-color: var(--accent);
}
.draft-foot {
  display: flex;
  align-items: center;
  gap: 8px;
}
.draft-foot .hint {
  flex: 1;
  color: var(--fg-muted);
  font-size: 11px;
}
.draft-foot button {
  padding: 4px 10px;
  border: 1px solid var(--border);
  border-radius: 6px;
  background: none;
  color: var(--fg);
  font: inherit;
  font-size: 12px;
  cursor: pointer;
}
.draft-foot button.primary {
  border-color: var(--accent);
  background: color-mix(in srgb, var(--accent) 20%, transparent);
}
.draft-foot button:disabled {
  opacity: 0.5;
  cursor: default;
}
.note {
  padding: 8px 12px;
  flex-shrink: 0;
  color: var(--yellow);
  font-size: 12px;
}
.markdown {
  flex: 1;
  min-height: 0;
  min-width: 0;
  overflow: auto;
  padding: 20px 32px 40px;
  color: var(--fg);
  font-size: 14px;
  line-height: 1.65;
  user-select: text;
}
.markdown :deep(h1),
.markdown :deep(h2),
.markdown :deep(h3) {
  margin: 1.4em 0 0.5em;
  line-height: 1.3;
}
.markdown :deep(h1) {
  font-size: 1.6em;
  padding-bottom: 0.3em;
  border-bottom: 1px solid var(--border);
}
.markdown :deep(h2) {
  font-size: 1.3em;
  padding-bottom: 0.25em;
  border-bottom: 1px solid var(--border);
}
.markdown :deep(a) {
  color: var(--accent);
}
.markdown :deep(code) {
  padding: 0.1em 0.35em;
  border-radius: 4px;
  background: var(--bg-elevated);
  font-family: var(--font-mono);
  font-size: 0.9em;
}
.markdown :deep(pre) {
  padding: 12px 14px;
  border: 1px solid var(--border);
  border-radius: 6px;
  background: var(--bg-elevated);
  overflow-x: auto;
}
.markdown :deep(pre code) {
  padding: 0;
  background: none;
}
.markdown :deep(blockquote) {
  margin: 0;
  padding: 0 1em;
  border-left: 3px solid var(--border);
  color: var(--fg-muted);
}
/* Tabela larga rola sozinha, sem empurrar o resto da página. */
.markdown :deep(table) {
  display: block;
  max-width: 100%;
  overflow-x: auto;
  border-collapse: collapse;
}
.markdown :deep(th),
.markdown :deep(td) {
  padding: 5px 10px;
  border: 1px solid var(--border);
}
.markdown :deep(hr) {
  border: 0;
  border-top: 1px solid var(--border);
}
.markdown :deep(img) {
  max-width: 100%;
}
.html {
  flex: 1;
  min-height: 0;
  border: 0;
  /* Página HTML costuma assumir fundo claro. */
  background: #fff;
}
.editor {
  flex: 1;
  min-height: 0;
}
</style>
