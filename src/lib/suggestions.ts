import { invoke } from "@tauri-apps/api/core";
import type { IDecoration, IMarker, Terminal } from "@xterm/xterm";
import { lineText } from "./paths";

/** Pra onde o botão manda o comando. */
export type RunTarget = "claude" | "panel";

/** Comandos das últimas respostas do Claude da sessão (vazio se não for o Claude). */
const fetchSuggestions = (ptyId: number) => invoke<string[]>("claude_suggestions", { id: ptyId });

// Prefixo mínimo pra reconhecer um comando quebrado na borda da tela.
const MIN_WRAPPED = 16;
// Dois botões de 3 colunas e um respiro entre eles.
const WIDTH = 7;

const PLAY = '<svg viewBox="0 0 16 16" width="11" height="11"><path d="M5 3.5v9l7-4.5z" fill="currentColor"/></svg>';
const TERMINAL =
  '<svg viewBox="0 0 16 16" width="12" height="12"><path d="M3 4.5l3.5 3.5L3 11.5M8.5 12h4.5" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round"/></svg>';

/** Onde o comando aparece numa linha da tela: coluna logo depois do texto. */
function findOnLine(text: string, xs: number[], first: string): number | null {
  const at = text.indexOf(first);
  if (at >= 0) {
    // Dentro de uma palavra maior (ex.: "ls -la" em "tools -la") não conta.
    const before = text[at - 1];
    if (before && /[\w./-]/.test(before)) return null;
    return xs[at + first.length - 1] + 1;
  }
  // Comando longo quebrado pelo Claude: a linha termina num pedaço do começo dele.
  if (first.length < MIN_WRAPPED) return null;
  const head = first.slice(0, MIN_WRAPPED);
  for (let p = text.indexOf(head); p >= 0; p = text.indexOf(head, p + 1)) {
    if (first.startsWith(text.slice(p))) return xs[text.length - 1] + 1;
  }
  return null;
}

/**
 * Botões ▶ (manda pro Claude com `!`) e ⤓ (manda pro terminal inferior) ao lado
 * dos comandos que o Claude sugeriu, na parte visível do terminal.
 */
export function attachSuggestions(term: Terminal, ptyId: () => number | undefined, run: (cmd: string, target: RunTarget) => void) {
  const shown = new Map<string, { marker: IMarker; deco: IDecoration }>();
  let timer: ReturnType<typeof setTimeout> | undefined;
  let busy = false;

  function clear(keep = new Set<string>()) {
    for (const [key, d] of shown) {
      if (keep.has(key) && !d.marker.isDisposed) continue;
      d.deco.dispose();
      d.marker.dispose();
      shown.delete(key);
    }
  }

  function button(icon: string, kind: string, title: string, onClick: () => void) {
    const b = document.createElement("button");
    b.className = `t7-run ${kind}`;
    b.innerHTML = icon;
    b.title = title;
    // Sem isso o xterm começa uma seleção e o terminal perde o clique.
    b.addEventListener("mousedown", (e) => {
      e.preventDefault();
      e.stopPropagation();
    });
    b.addEventListener("click", (e) => {
      e.stopPropagation();
      onClick();
    });
    return b;
  }

  async function scan() {
    const id = ptyId();
    if (id === undefined || busy) return;
    busy = true;
    try {
      const cmds = await fetchSuggestions(id).catch(() => [] as string[]);
      if (!cmds.length) return clear();
      const buf = term.buffer.active;
      const keep = new Set<string>();
      for (let y = buf.viewportY; y < buf.viewportY + term.rows && y < buf.length; y++) {
        const { text, xs } = lineText(term, y);
        if (!text.trim()) continue;
        for (const cmd of cmds) {
          const first = cmd.split("\n")[0].trim();
          if (first.length < 3) continue;
          const col = findOnLine(text, xs, first);
          if (col === null) continue;
          const key = `${y}:${col}:${cmd}`;
          keep.add(key);
          if (shown.has(key)) break;
          const marker = term.registerMarker(y - (buf.baseY + buf.cursorY));
          if (!marker) break;
          // Depois do fim do texto da linha (nunca por cima de uma frase que
          // continua depois do comando); sem espaço, na borda direita.
          const end = Math.max(col, xs[text.length - 1] + 1);
          const x = Math.min(end + 1, term.cols - WIDTH);
          const deco = term.registerDecoration({ marker, x, width: WIDTH, layer: "top" });
          if (!deco) {
            marker.dispose();
            break;
          }
          deco.onRender((el) => {
            if (el.dataset.t7) return;
            el.dataset.t7 = "1";
            el.classList.add("t7-run-box");
            el.append(
              button(PLAY, "claude", `Rodar no Claude: ! ${first}`, () => run(cmd, "claude")),
              button(TERMINAL, "panel", "Digitar no terminal inferior (sem Enter)", () => run(cmd, "panel")),
            );
          });
          shown.set(key, { marker, deco });
          break; // um botão por linha
        }
      }
      clear(keep);
    } finally {
      busy = false;
    }
  }

  const schedule = (ms: number) => {
    clearTimeout(timer);
    timer = setTimeout(scan, ms);
  };
  // Saída do Claude chega em rajadas: espera assentar antes de procurar. O
  // Claude grava a resposta no histórico um pouco depois de desenhar a tela,
  // então procura de novo mais tarde (o backend só relê se o arquivo mudou).
  let late: ReturnType<typeof setTimeout>[] = [];
  term.onWriteParsed(() => {
    schedule(600);
    late.forEach(clearTimeout);
    late = [2500, 6000].map((ms) => setTimeout(scan, ms));
  });
  term.onScroll(() => schedule(150));
  term.onResize(() => schedule(300));
}
