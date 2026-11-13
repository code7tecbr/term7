import { describe, expect, it } from "vitest";
import { matchShortcut } from "./shortcuts";

// Os testes rodam no Node, sem DOM: só os campos que o matchShortcut lê.
const key = (init: Partial<KeyboardEvent>) =>
  ({ key: "", code: "", ctrlKey: false, shiftKey: false, altKey: false, metaKey: false, ...init }) as KeyboardEvent;

describe("atalhos de voz", () => {
  it("Ctrl+Espaço dita e Ctrl+Shift+Espaço ouve a resposta", () => {
    expect(matchShortcut(key({ key: " ", code: "Space", ctrlKey: true }))).toEqual({ type: "toggleVoice" });
    expect(matchShortcut(key({ key: " ", code: "Space", ctrlKey: true, shiftKey: true }))).toEqual({ type: "listenReply" });
  });

  it("espaço sozinho ou com Alt vai pro terminal", () => {
    expect(matchShortcut(key({ key: " ", code: "Space" }))).toBeNull();
    expect(matchShortcut(key({ key: " ", code: "Space", ctrlKey: true, altKey: true }))).toBeNull();
  });
});
