import type { ReviewComment, Tab } from "./tabs";

let nextId = 1;

export function addComment(tab: Tab, comment: Omit<ReviewComment, "id">) {
  tab.review.push({ ...comment, id: nextId++ });
}

export function removeComment(tab: Tab, id: number) {
  const i = tab.review.findIndex((c) => c.id === id);
  if (i >= 0) tab.review.splice(i, 1);
}

export const commentsFor = (tab: Tab, root: string, path: string) =>
  tab.review.filter((c) => c.root === root && c.path === path);

export const lines = (c: Pick<ReviewComment, "from" | "to">) => (c.from === c.to ? `${c.from}` : `${c.from}-${c.to}`);

/** Onde o comentário aponta, como aparece na lista e no prompt. */
export const where = (c: Pick<ReviewComment, "from" | "to" | "removed">, path: string) =>
  c.removed ? `${path}:${c.removed.at} (trecho removido; linhas ${lines(c)} no último commit)` : `${path}:${lines(c)}`;

/** Caminho como o Claude da aba enxerga: relativo à pasta dele quando dá, senão absoluto. */
function displayPath(c: ReviewComment, cwd: string | null) {
  const abs = `${c.root}/${c.path}`.replace(/^\/\//, "/");
  if (cwd && abs.startsWith(cwd.replace(/\/$/, "") + "/")) return abs.slice(cwd.replace(/\/$/, "").length + 1);
  return abs;
}

const MAX_CODE_LINES = 40;

/** Texto da revisão, em Markdown, do jeito que entra no prompt do Claude. */
export function reviewPrompt(tab: Tab) {
  const parts = tab.review.map((c) => {
    const code = c.code.split("\n");
    const shown = code.length > MAX_CODE_LINES ? [...code.slice(0, MAX_CODE_LINES), "…"] : code;
    const fence = c.code.includes("```") ? "~~~~" : "```";
    return [where(c, displayPath(c, tab.cwd)), fence, ...shown, fence, ...c.text.trim().split("\n").map((l) => `> ${l}`)].join("\n");
  });
  return `Comentários de revisão:\n\n${parts.join("\n\n")}\n`;
}
