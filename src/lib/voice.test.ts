import { describe, expect, it } from "vitest";
import { voiceCommand } from "./voice";

describe("palavra-chave do ditado", () => {
  it("tira o enviar do fim e pede o Enter", () => {
    expect(voiceCommand("Corrige o teste do login. Enviar.")).toEqual({ text: "Corrige o teste do login", send: true });
    expect(voiceCommand("roda os testes, enviar")).toEqual({ text: "roda os testes", send: true });
    expect(voiceCommand(" Faz o commit ENVIAR! ")).toEqual({ text: "Faz o commit", send: true });
  });

  it("só enviar manda o que já estava digitado", () => {
    expect(voiceCommand("Enviar.")).toEqual({ text: "", send: true });
  });

  it("não confunde com enviar no meio ou dentro de outra palavra", () => {
    expect(voiceCommand("Vou enviar o arquivo amanhã.")).toEqual({ text: "Vou enviar o arquivo amanhã.", send: false });
    expect(voiceCommand("arquivos para reenviar")).toEqual({ text: "arquivos para reenviar", send: false });
    expect(voiceCommand("")).toEqual({ text: "", send: false });
  });
});
