export type Action =
  | { type: "newTab" }
  | { type: "closeTab" }
  | { type: "nextTab" }
  | { type: "prevTab" }
  | { type: "goTo"; index: number }
  | { type: "toggleDiff" }
  | { type: "togglePanel" }
  | { type: "toggleVoice" }
  | { type: "listenReply" };

/** Atalhos globais do app. Retorna null quando a tecla deve ir pro shell. */
export function matchShortcut(e: KeyboardEvent): Action | null {
  const key = e.key.toLowerCase();
  const ctrlShift = e.ctrlKey && e.shiftKey && !e.altKey && !e.metaKey;
  const ctrlOnly = e.ctrlKey && !e.shiftKey && !e.altKey && !e.metaKey;

  if (ctrlShift && key === "t") return { type: "newTab" };
  if (ctrlShift && key === "w") return { type: "closeTab" };
  if (ctrlShift && key === "g") return { type: "toggleDiff" };
  // Ditado: mesmo que clicar no microfone (começa, para ou corta a leitura em voz alta).
  if (ctrlOnly && (e.code === "Space" || e.key === " ")) return { type: "toggleVoice" };
  // Ouvir a última resposta do Claude da aba (ou parar a leitura).
  if (ctrlShift && (e.code === "Space" || e.key === " ")) return { type: "listenReply" };
  // Ctrl+' (tecla à esquerda do 1 no ABNT2) ou Ctrl+` (layout US).
  if (e.ctrlKey && !e.altKey && !e.metaKey && (e.code === "Backquote" || e.key === "`")) {
    return { type: "togglePanel" };
  }
  if (e.ctrlKey && !e.altKey && key === "tab") return { type: e.shiftKey ? "prevTab" : "nextTab" };
  if (ctrlOnly && e.key === "PageDown") return { type: "nextTab" };
  if (ctrlOnly && e.key === "PageUp") return { type: "prevTab" };
  if (e.altKey && !e.ctrlKey && !e.shiftKey && !e.metaKey && /^[1-9]$/.test(e.key)) {
    return { type: "goTo", index: Number(e.key) };
  }
  return null;
}
