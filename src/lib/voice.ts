// O Whisper costuma pontuar e capitalizar a última palavra ("... Enviar."), então a pontuação em volta é ignorada.
const SEND_WORD = /(?:^|[\s.,;:!?…-]+)enviar[\s.,;:!?…]*$/i;

/** Ditado terminado em "enviar": tira a palavra do texto e pede o Enter. */
export function voiceCommand(text: string): { text: string; send: boolean } {
  const trimmed = text.trim();
  const match = SEND_WORD.exec(trimmed);
  if (!match) return { text: trimmed, send: false };
  return { text: trimmed.slice(0, match.index), send: true };
}
