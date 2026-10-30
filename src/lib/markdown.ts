import DOMPurify from "dompurify";
import { marked } from "marked";
import { openUrl } from "./github";

// As listas são relidas a cada poucos segundos: guarda o HTML de cada texto pra não converter de novo.
const cache = new Map<string, string>();
const MAX_CACHE = 2000;

const render = (text: string) => DOMPurify.sanitize(marked.parse(text, { async: false, gfm: true, breaks: true }));

/** Markdown do Claude em HTML, passando pelo DOMPurify: o texto pode vir de qualquer arquivo lido. */
export function md(text: string) {
  let html = cache.get(text);
  if (html === undefined) {
    html = render(text);
    if (cache.size >= MAX_CACHE) cache.clear();
    cache.set(text, html);
  }
  return html;
}

/** Texto que ainda está chegando: cada pedaço é diferente, guardar só enche o cache. */
export const mdLive = render;

/** Links dentro do Markdown abrem no navegador, nunca dentro do app. */
export function onLinkClick(e: MouseEvent) {
  const a = (e.target as HTMLElement).closest("a");
  if (!a) return;
  e.preventDefault();
  const href = a.getAttribute("href") ?? "";
  if (/^https?:\/\//.test(href)) openUrl(href).catch(() => {});
}
