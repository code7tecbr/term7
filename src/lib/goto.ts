import { invoke } from "@tauri-apps/api/core";
import { StateEffect, StateField, type Extension } from "@codemirror/state";
import { Decoration, EditorView, ViewPlugin } from "@codemirror/view";

// ---- Ctrl+clique: ir pra definição ----

export interface Definition {
  /** Relativo à raiz pesquisada. */
  path: string;
  line: number;
  text: string;
}

/** Linhas que definem `symbol` no repositório (busca por texto, sem servidor de linguagem). */
export const findDefinitions = (root: string, file: string, symbol: string) =>
  invoke<Definition[]>("find_definitions", { root, file, symbol });

/** Caminho absoluto de um import relativo (`./x`), ou null. */
export const resolveImportPath = (dir: string, spec: string) =>
  invoke<string | null>("resolve_import_path", { dir, spec });

/**
 * Import relativo sob a coluna `col` da linha: o texto entre aspas, se começar
 * com `./` ou `../` (`import x from "./foo"`, `require('../bar')`).
 */
export function importAt(line: string, col: number): string | null {
  for (const m of line.matchAll(/(["'`])(\.\.?\/[^"'`]*)\1/g)) {
    const start = m.index!;
    if (col >= start && col <= start + m[0].length) return m[2];
  }
  return null;
}

/** O que foi clicado: um nome ou um caminho de import. */
export type GotoTarget = { kind: "symbol"; name: string } | { kind: "import"; spec: string };

function targetAt(view: EditorView, pos: number): { target: GotoTarget; from: number; to: number } | null {
  const line = view.state.doc.lineAt(pos);
  const spec = importAt(line.text, pos - line.from);
  if (spec) {
    const at = line.text.indexOf(spec);
    return { target: { kind: "import", spec }, from: line.from + at, to: line.from + at + spec.length };
  }
  const word = view.state.wordAt(pos);
  if (!word) return null;
  const name = view.state.sliceDoc(word.from, word.to);
  if (!/^[A-Za-z_$][\w$]*$/.test(name)) return null;
  return { target: { kind: "symbol", name }, ...word };
}

// Sublinha o nome sob o mouse enquanto o Ctrl está apertado, como no VS Code.
const setHover = StateEffect.define<{ from: number; to: number } | null>();
const linkMark = Decoration.mark({ class: "cm-goto-link" });
const hoverField = StateField.define({
  create: () => Decoration.none,
  update(deco, tr) {
    for (const e of tr.effects) {
      if (e.is(setHover)) return e.value ? Decoration.set([linkMark.range(e.value.from, e.value.to)]) : Decoration.none;
    }
    return deco.map(tr.changes);
  },
  provide: (f) => EditorView.decorations.from(f),
});

/** Ctrl+clique num nome ou caminho de import chama `go` (com a posição do clique). */
export function gotoDefinition(go: (target: GotoTarget, e: MouseEvent) => void): Extension {
  let last: { from: number; to: number } | null = null;
  const hover = (view: EditorView, range: { from: number; to: number } | null) => {
    if (last?.from === range?.from && last?.to === range?.to) return;
    last = range;
    view.dispatch({ effects: setHover.of(range) });
  };
  const update = (view: EditorView, e: MouseEvent) => {
    const pos = e.ctrlKey ? view.posAtCoords({ x: e.clientX, y: e.clientY }) : null;
    const t = pos == null ? null : targetAt(view, pos);
    hover(view, t && { from: t.from, to: t.to });
  };
  const keys = ViewPlugin.define((view) => {
    // O Ctrl solto fora do editor também tira o sublinhado.
    const up = (e: KeyboardEvent) => e.key === "Control" && hover(view, null);
    window.addEventListener("keyup", up);
    return { destroy: () => window.removeEventListener("keyup", up) };
  });
  return [
    hoverField,
    keys,
    // Ctrl+clique não cria cursor extra (o padrão do CodeMirror); Alt+clique continua criando.
    EditorView.clickAddsSelectionRange.of((e) => e.altKey),
    EditorView.domEventHandlers({
      mousemove: (e, view) => update(view, e),
      mouseleave: (_e, view) => hover(view, null),
      mousedown(e, view) {
        if (!e.ctrlKey || e.button !== 0) return false;
        const pos = view.posAtCoords({ x: e.clientX, y: e.clientY });
        const t = pos == null ? null : targetAt(view, pos);
        if (!t) return false;
        e.preventDefault();
        go(t.target, e);
        return true;
      },
    }),
    EditorView.theme({
      ".cm-goto-link": { textDecoration: "underline", cursor: "pointer", color: "var(--accent)" },
    }),
  ];
}
