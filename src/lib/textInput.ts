import type { Terminal } from "@xterm/xterm";

/**
 * Tecla que produz texto (letra, número, símbolo, espaço) ou faz parte de um
 * acento composto (tecla morta, composição do método de entrada).
 *
 * No WebKitGTK a tecla morta chega como o próprio caractere ("´", "`", "'") e
 * o xterm enviava esse caractere solto antes do acento composto. Essas teclas
 * passam a ser ignoradas no keydown: o texto vem só do que o sistema de fato
 * digitou (evento input / fim da composição), igual a um campo de texto.
 */
export function isTextKey(e: KeyboardEvent) {
  if (e.isComposing || e.keyCode === 229 || e.key === "Dead") return true;
  // AltGr (ex.: AltGr+q = "/") digita texto; Ctrl/Alt/Meta são atalhos e sequências.
  const altGr = e.getModifierState?.("AltGraph");
  if (!altGr && (e.ctrlKey || e.altKey || e.metaKey)) return false;
  return e.key.length === 1;
}

/**
 * Manda pro shell o texto digitado no campo escondido do xterm. Os eventos são
 * pegos na fase de captura no container, antes de chegarem no xterm, que assim
 * não manda nada em dobro.
 */
export function routeTextInput(term: Terminal, container: HTMLElement, send: (text: string) => void) {
  const textarea = term.textarea!;
  let composing = false;

  const clear = () => {
    textarea.value = "";
  };

  container.addEventListener(
    "compositionstart",
    (e) => {
      if (e.target !== textarea) return;
      e.stopPropagation();
      composing = true;
    },
    true,
  );
  container.addEventListener(
    "compositionupdate",
    (e) => {
      if (e.target === textarea) e.stopPropagation();
    },
    true,
  );
  container.addEventListener(
    "compositionend",
    (e) => {
      if (e.target !== textarea) return;
      e.stopPropagation();
      composing = false;
      const text = (e as CompositionEvent).data;
      if (text) send(text);
      clear();
    },
    true,
  );
  container.addEventListener(
    "input",
    (e) => {
      if (e.target !== textarea) return;
      e.stopPropagation();
      const ev = e as InputEvent;
      // Durante a composição o texto ainda é provisório: só vale o do compositionend.
      if (composing || ev.isComposing) return;
      if (ev.inputType === "insertText" && ev.data) send(ev.data);
      clear();
    },
    true,
  );
}
