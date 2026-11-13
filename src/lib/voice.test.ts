import { describe, expect, it } from "vitest";
import { voiceCommand } from "./voice";

describe("frase de enviar do ditado", () => {
  it("aceita os jeitos que o Whisper escreve term7", () => {
    // Variações tiradas de ditados reais.
    for (const end of ["term7 pronto", "Term 7, pronto.", "Terme 7 pronto!", "Term set pronto", "term sete, pronto", "Termsete pronto.", "TERM7 PRONTO"]) {
      expect(voiceCommand(`Roda os testes do login. ${end}`)).toEqual({ text: "Roda os testes do login", send: true });
    }
  });

  it("só a frase manda o que já estava digitado", () => {
    expect(voiceCommand("Term 7, pronto.")).toEqual({ text: "", send: true });
  });

  it("frase no meio ou incompleta não manda", () => {
    expect(voiceCommand("o term7 pronto pra release, roda o build")).toEqual({ text: "o term7 pronto pra release, roda o build", send: false });
    expect(voiceCommand("ficou pronto")).toEqual({ text: "ficou pronto", send: false });
    expect(voiceCommand("")).toEqual({ text: "", send: false });
  });

  it("frase configurável, com acento e pontuação indiferentes", () => {
    expect(voiceCommand("faz o commit, enviar", "enviar")).toEqual({ text: "faz o commit", send: true });
    expect(voiceCommand("faz o commit. Terminei, Terme 7.", "terminei term7")).toEqual({ text: "faz o commit", send: true });
    expect(voiceCommand("abre o PR, term7 ok", "Term7 OK")).toEqual({ text: "abre o PR", send: true });
    expect(voiceCommand("arquivos para reenviar", "enviar")).toEqual({ text: "arquivos para reenviar", send: false });
  });

  it("frase vazia desliga", () => {
    expect(voiceCommand("faz o commit term7 pronto", "")).toEqual({ text: "faz o commit term7 pronto", send: false });
  });
});
