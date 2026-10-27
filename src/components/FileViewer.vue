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
import { closeAllFiles, closeFile, viewer, type OpenFile } from "../state/viewer";

const props = defineProps<{ status: GitStatus | null }>();

const host = ref<HTMLDivElement>();
const note = ref<string | null>(null);
let view: EditorView | undefined;

const active = computed<OpenFile | undefined>(() => viewer.files[viewer.active]);

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
  const change = file && props.status?.files.find((f) => f.path === file.path);
  return change ? `${change.additions}|${change.deletions}|${change.kind}` : "clean";
});

let loadId = 0;

async function load() {
  const file = active.value;
  if (!view || !file) return;
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
  if (viewer.mode === "changes") {
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

watch([() => active.value?.path, () => active.value?.root, () => viewer.mode, changeKey], load);

onMounted(() => {
  view = new EditorView({ parent: host.value! });
  load();
});

onBeforeUnmount(() => view?.destroy());

const fileName = (path: string) => path.split("/").pop() ?? path;
</script>

<template>
  <section class="viewer">
    <header @mousedown.prevent>
      <div class="files">
        <div
          v-for="(file, i) in viewer.files"
          :key="file.root + file.path"
          class="file"
          :class="{ active: i === viewer.active }"
          :title="file.path"
          @click="viewer.active = i"
          @mousedown.middle.prevent="closeFile(i)"
        >
          <span>{{ fileName(file.path) }}</span>
          <button class="x" title="Fechar" @click.stop="closeFile(i)">
            <svg viewBox="0 0 16 16" width="11" height="11"><path d="M4 4l8 8M12 4l-8 8" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" /></svg>
          </button>
        </div>
      </div>
      <div class="modes">
        <button :class="{ on: viewer.mode === 'changes' }" title="Arquivo com as alterações marcadas" @click="viewer.mode = 'changes'">Alterações</button>
        <button :class="{ on: viewer.mode === 'file' }" title="Arquivo atual, sem marcações" @click="viewer.mode = 'file'">Arquivo</button>
      </div>
      <button class="icon" title="Fechar visualizador" @click="closeAllFiles">
        <svg viewBox="0 0 16 16" width="13" height="13"><path d="M4 4l8 8M12 4l-8 8" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" /></svg>
      </button>
    </header>
    <div v-if="active" class="path">{{ active.path }}</div>
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
header {
  display: flex;
  align-items: center;
  gap: 6px;
  height: 36px;
  padding: 0 6px;
  flex-shrink: 0;
  background: var(--bg-elevated);
  border-bottom: 1px solid var(--border);
  user-select: none;
}
.files {
  display: flex;
  gap: 2px;
  flex: 1;
  min-width: 0;
  overflow-x: auto;
  scrollbar-width: none;
}
.file {
  display: flex;
  align-items: center;
  gap: 4px;
  flex-shrink: 0;
  height: 26px;
  padding: 0 4px 0 10px;
  border-radius: 5px;
  color: var(--fg-muted);
  font-size: 12px;
  cursor: pointer;
}
.file:hover {
  background: var(--hover);
}
.file.active {
  background: var(--bg);
  color: var(--fg);
}
.x,
.icon {
  display: grid;
  place-items: center;
  border: 0;
  border-radius: 4px;
  background: none;
  color: var(--fg-muted);
  cursor: pointer;
}
.x {
  width: 18px;
  height: 18px;
}
.icon {
  width: 26px;
  height: 26px;
}
.x:hover,
.icon:hover {
  background: var(--hover);
  color: var(--fg);
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
