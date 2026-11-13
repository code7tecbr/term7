import type { IDecoration, IMarker, Terminal } from "@xterm/xterm";
import { WebLinksAddon } from "@xterm/addon-web-links";
import { openUrl } from "./github";

// Mesma ideia do regex do addon: termina antes de pontuação final (ex.: "veja https://x.com.").
// file:/// entra pros relatórios HTML locais; o backend só abre os .html/.htm.
// Sem a flag "g": o addon acrescenta a dele e "gg" é regex inválido.
export const URL_PATTERN = /(?:https?:\/\/|file:\/\/\/)[^\s"'<>`]*[^\s"'<>`.,;:!?)\]}]/;
const URL_RE = new RegExp(URL_PATTERN.source, "g");
const LINK_COLOR = "#7aa2f7";

/**
 * Links na saída do terminal (inclusive do Claude CLI): cursor de mão e
 * sublinhado no hover, clique abre no navegador, e a cor fica sempre visível
 * nas URLs da tela.
 */
export function attachLinks(term: Terminal) {
  // Hyperlinks OSC 8 (o Claude CLI usa: o texto pode ser "#580" e o endereço fica escondido).
  // Sem handler o xterm não abre nada; com ele, hover sublinha e o clique abre.
  term.options.linkHandler = {
    activate: (event, uri) => {
      event.preventDefault();
      openUrl(uri).catch(() => {});
    },
    // Deixa passar o file:// (relatório HTML local); o backend recusa o resto.
    allowNonHttpProtocols: true,
  };

  // URLs em texto puro.
  term.loadAddon(
    new WebLinksAddon((event, uri) => {
      event.preventDefault();
      openUrl(uri).catch(() => {});
    }, { urlRegex: URL_PATTERN }),
  );

  // A cor é refeita a cada saída nova, só na área visível: programas como o
  // Claude redesenham a tela e uma cor fixa numa linha ficaria no texto errado.
  let painted: { marker: IMarker; deco?: IDecoration }[] = [];
  let timer: ReturnType<typeof setTimeout> | undefined;

  function paint() {
    for (const p of painted) {
      p.deco?.dispose();
      p.marker.dispose();
    }
    painted = [];
    const buf = term.buffer.active;
    const cursorLine = buf.baseY + buf.cursorY;
    for (let y = buf.viewportY; y < buf.viewportY + term.rows; y++) {
      const line = buf.getLine(y);
      if (!line) continue;
      // Coluna de cada caractere: emoji/CJK ocupam 2 células, então índice ≠ coluna.
      let text = "";
      const cols: number[] = [];
      for (let x = 0; x < line.length; x++) {
        const chars = line.getCell(x)?.getChars() ?? "";
        if (!chars && line.getCell(x)?.getWidth() === 0) continue;
        for (const ch of chars || " ") {
          text += ch;
          cols.push(x);
        }
      }
      for (const m of text.matchAll(URL_RE)) {
        const start = cols[m.index];
        const end = cols[m.index + m[0].length - 1];
        const marker = term.registerMarker(y - cursorLine);
        if (!marker) continue;
        const deco = term.registerDecoration({ marker, x: start, width: end - start + 1, foregroundColor: LINK_COLOR });
        painted.push({ marker, deco });
      }
    }
  }

  const schedule = () => {
    clearTimeout(timer);
    timer = setTimeout(paint, 80);
  };
  term.onWriteParsed(schedule);
  term.onScroll(schedule);
  term.onResize(schedule);
}
