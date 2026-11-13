/** Frase padrão que, no fim do ditado, manda com Enter. */
export const DEFAULT_SEND_PHRASE = "term7 pronto";

// O Whisper escreve "term7" de vários jeitos: "Term 7", "Terme 7", "Termo 7", "Term-7", "Term set", "Terms set", "termsete"...
const TERM7_HEAD = /^ter?m[aeo]?s?$/;
const TERM7_NUMBER = /^(7|sete|set|seth|seven)$/;
const TERM7_JOINED = /^ter?m[aeo]?s?-?(7|sete|set|seth|seven)$/;

/** Palavra comparável: minúscula, sem acento e sem pontuação em volta. */
function normalize(word: string) {
  return word
    .normalize("NFD")
    .replace(/\p{M}/gu, "")
    .toLowerCase()
    .replace(/^[^\p{L}\p{N}]+|[^\p{L}\p{N}]+$/gu, "");
}

/** Palavras do texto já com as variações de "term7" juntadas, e onde cada uma começa no original. */
function words(text: string) {
  const raw = [...text.matchAll(/\S+/g)].map((m) => ({ word: normalize(m[0]), start: m.index! })).filter((w) => w.word);
  const out: { word: string; start: number }[] = [];
  for (let i = 0; i < raw.length; i++) {
    const { word, start } = raw[i];
    if (TERM7_JOINED.test(word)) out.push({ word: "term7", start });
    else if (TERM7_HEAD.test(word) && TERM7_NUMBER.test(raw[i + 1]?.word ?? "")) {
      out.push({ word: "term7", start });
      i++;
    } else out.push({ word, start });
  }
  return out;
}

/** Ditado terminado na frase de enviar: tira a frase do texto e pede o Enter. Frase vazia desliga. */
export function voiceCommand(text: string, phrase = DEFAULT_SEND_PHRASE): { text: string; send: boolean } {
  const trimmed = text.trim();
  const want = words(phrase).map((w) => w.word);
  const got = words(trimmed);
  if (!want.length || got.length < want.length) return { text: trimmed, send: false };
  const tail = got.slice(got.length - want.length);
  if (tail.some((w, i) => w.word !== want[i])) return { text: trimmed, send: false };
  // O que vem antes, sem a pontuação que separava da frase ("…login. Term 7, pronto.").
  const before = trimmed.slice(0, tail[0].start).replace(/[\s.,;:!?…-]+$/u, "");
  return { text: before, send: true };
}
