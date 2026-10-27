<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from "vue";
import { basicSetup } from "codemirror";
import { EditorState, type Extension } from "@codemirror/state";
import { EditorView } from "@codemirror/view";
import { LanguageDescription, syntaxHighlighting } from "@codemirror/language";
import { languages } from "@codemirror/language-data";
import { unifiedMergeView } from "@codemirror/merge";
import { oneDarkHighlightStyle } from "@codemirror/theme-one-dark";
import { gitShowHead, readTextFile, type GitStatus } from "../lib/git";
import type { OpenFile } from "../state/tabs";

/** Um arquivo aberto na área principal: atual ou com as alterações marcadas. */
const props = defineProps<{ file: OpenFile; status: GitStatus | null }>();

const host = ref<HTMLDivElement>();
const note = ref<string | null>(null);
let view: EditorView | undefined;

const active = computed(() => props.file);

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
  const extensions: Extension[] = [
    basicSetup,
    EditorState.readOnly.of(true),
    syntaxHighlighting(oneDarkHighlightStyle),
    theme,
    language,
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
  if (sameFile) requestAnimationFrame(() => view && (view.scrollDOM.scrollTop = top));
}

watch([() => props.file.path, () => props.file.root, () => props.file.mode, changeKey], load);

onMounted(() => {
  view = new EditorView({ parent: host.value! });
  load();
});

onBeforeUnmount(() => view?.destroy());

</script>

<template>
  <section class="viewer">
    <div class="path">{{ file.path }}</div>
    <div v-if="note" class="note">{{ note }}</div>
    <div ref="host" class="editor" />
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
.note {
  padding: 8px 12px;
  flex-shrink: 0;
  color: var(--yellow);
  font-size: 12px;
}
.editor {
  flex: 1;
  min-height: 0;
}
</style>
