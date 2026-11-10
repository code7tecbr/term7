import type { Terminal } from "@xterm/xterm";

/** Leitura da tela de cada terminal, por id do PTY (a API das extensões usa). */
const readers = new Map<number, (lines: number) => string[]>();

/** Últimas `n` linhas, sem as vazias do fim (o cursor costuma estar abaixo do texto). */
export function lastLines(all: string[], n: number) {
  let end = all.length;
  while (end > 0 && !all[end - 1].trim()) end--;
  return all.slice(Math.max(0, end - n), end);
}

export function registerScreen(ptyId: number, term: Terminal) {
  readers.set(ptyId, (n) => {
    const buf = term.buffer.active;
    const all: string[] = [];
    for (let i = 0; i < buf.length; i++) all.push(buf.getLine(i)?.translateToString(true) ?? "");
    return lastLines(all, n);
  });
  return () => readers.delete(ptyId);
}

export const readScreen = (ptyId: number, lines: number) => readers.get(ptyId)?.(lines) ?? null;
