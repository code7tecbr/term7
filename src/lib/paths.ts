import { invoke } from "@tauri-apps/api/core";
import type { IBufferCellPosition, ILink, Terminal } from "@xterm/xterm";

/** Arquivo pedido por um clique num caminho da saída do terminal. */
export interface PathTarget {
  /** Caminho absoluto, já conferido no disco. */
  path: string;
  /** Linha (1 = primeira), quando o caminho veio como `arquivo:42`. */
  line?: number;
}

// Caminho com pelo menos uma "/": absoluto, ~/, ./, ../ ou relativo (src/x.ts),
// com linha e coluna opcionais (src/x.ts:42:7).
const PATH_RE = /(?:~\/|\.{1,2}\/|\/)?(?:[\w.@+-]+\/)+[\w.@+-]*[\w@+-](?::(\d+))?(?::\d+)?/g;
const PATH_CHAR = /[\w.@+\/~-]/;

/** Caminhos que já conferimos no disco (o hover repete muito a mesma linha). */
const cache = new Map<string, { path: string | null; at: number }>();
const CACHE_MS = 5000;

async function resolvePaths(cwd: string | null, candidates: string[]) {
  const now = Date.now();
  const key = (c: string) => `${cwd ?? ""}\0${c}`;
  const missing = candidates.filter((c) => {
    const hit = cache.get(key(c));
    return !hit || now - hit.at > CACHE_MS;
  });
  if (missing.length) {
    const found = await invoke<(string | null)[]>("resolve_paths", { cwd, candidates: missing }).catch(() => null);
    missing.forEach((c, i) => cache.set(key(c), { path: found?.[i] ?? null, at: now }));
  }
  return candidates.map((c) => cache.get(key(c))?.path ?? null);
}

/** Texto de uma linha do buffer, com a célula (x) de cada caractere. */
export function lineText(term: Terminal, y: number) {
  const line = term.buffer.active.getLine(y);
  let text = "";
  const xs: number[] = [];
  if (!line) return { text, xs };
  for (let x = 0; x < line.length; x++) {
    const cell = line.getCell(x);
    const chars = cell?.getChars() ?? "";
    // Segunda metade de caractere largo (emoji/CJK): não ocupa posição no texto.
    if (!chars && cell?.getWidth() === 0) continue;
    for (const ch of chars || " ") {
      text += ch;
      xs.push(x);
    }
  }
  return { text: text.trimEnd(), xs };
}

/**
 * O Claude CLI quebra caminhos longos em várias linhas (quebra de verdade, não
 * o wrap do terminal). Junta a linha com a seguinte quando ela termina num
 * caractere de caminho perto da borda e a seguinte continua o caminho.
 */
function continues(term: Terminal, a: string, b: string) {
  const next = b.trimStart();
  return (
    a.length >= term.cols - 20 &&
    PATH_CHAR.test(a[a.length - 1] ?? "") &&
    next.length > 0 &&
    PATH_CHAR.test(next[0]) &&
    // Linha seguinte começa com espaço só pela indentação (até 4).
    b.length - next.length <= 4
  );
}

/** Linhas y-2..y+2 juntadas quando continuam um caminho, com a posição de cada caractere. */
function logicalText(term: Terminal, y: number) {
  const ys = [y - 2, y - 1, y, y + 1, y + 2].filter((l) => l >= 0 && l < term.buffer.active.length);
  const parts = ys.map((l) => ({ y: l, ...lineText(term, l) }));
  let text = "";
  const pos: IBufferCellPosition[] = [];
  parts.forEach((p, i) => {
    let start = 0;
    if (i > 0) {
      if (continues(term, parts[i - 1].text, p.text)) {
        start = p.text.length - p.text.trimStart().length;
      } else {
        // Linhas que não continuam: um separador que nenhum caminho atravessa.
        text += "\n";
        pos.push({ x: 0, y: p.y + 1 });
      }
    }
    for (let k = start; k < p.text.length; k++) {
      text += p.text[k];
      pos.push({ x: p.xs[k] + 1, y: p.y + 1 });
    }
  });
  return { text, pos };
}

/**
 * Caminhos de arquivo na saída viram links (só os que existem no disco).
 * Clique abre o arquivo no term7, na linha indicada.
 */
export function attachPathLinks(term: Terminal, getCwd: () => Promise<string | null>, open: (t: PathTarget) => void) {
  term.registerLinkProvider({
    async provideLinks(bufferLine, callback) {
      const y = bufferLine - 1;
      const { text, pos } = logicalText(term, y);
      const matches = [...text.matchAll(PATH_RE)].filter((m) => {
        // Parte de uma URL ("https://x.com/a/b"): quem cuida é o addon de links.
        const before = text.slice(Math.max(0, m.index - 3), m.index);
        if (/:\/?\/?$/.test(before) && before.includes(":")) return false;
        // Só os que passam pela linha pedida.
        const first = pos[m.index].y;
        const last = pos[m.index + m[0].length - 1].y;
        return first <= bufferLine && last >= bufferLine;
      });
      if (!matches.length) return callback(undefined);

      const cwd = await getCwd().catch(() => null);
      const files = matches.map((m) => m[0].replace(/(?::\d+){1,2}$/, ""));
      const resolved = await resolvePaths(cwd, files);

      const links: ILink[] = [];
      matches.forEach((m, i) => {
        const path = resolved[i];
        if (!path) return;
        const line = m[1] ? Number(m[1]) : undefined;
        links.push({
          range: { start: pos[m.index], end: pos[m.index + m[0].length - 1] },
          text: m[0],
          decorations: { pointerCursor: true, underline: true },
          activate: (event) => {
            event.preventDefault();
            open({ path, line });
          },
        });
      });
      callback(links.length ? links : undefined);
    },
  });
}
