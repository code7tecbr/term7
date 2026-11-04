import { describe, expect, it, vi } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
const { PATH_RE } = await import("./paths");
const { shellQuote } = await import("./settings");

const matches = (text: string) => [...text.matchAll(PATH_RE)].map((m) => [m[0], m[1] ?? null]);

describe("PATH_RE", () => {
  it("reconhece caminhos com linha e coluna", () => {
    expect(matches("erro em src/lib/x.ts:42:7 aqui")).toEqual([["src/lib/x.ts:42:7", "42"]]);
  });

  it("reconhece absolutos, ~/ e ../", () => {
    expect(matches("/etc/hosts ~/a/b.md ../c/d").map((m) => m[0])).toEqual(["/etc/hosts", "~/a/b.md", "../c/d"]);
  });

  it("ignora palavra solta sem barra", () => {
    expect(matches("package.json e README")).toEqual([]);
  });

  it("não engole a pontuação do fim da frase", () => {
    expect(matches("veja src/a.ts.")[0][0]).toBe("src/a.ts");
  });
});

describe("shellQuote", () => {
  it("deixa caminho simples como está", () => {
    expect(shellQuote("/home/u/a-b_c.txt")).toBe("/home/u/a-b_c.txt");
  });

  it("põe aspas em espaço e escapa aspas simples", () => {
    expect(shellQuote("/tmp/meu arquivo's.txt")).toBe(`'/tmp/meu arquivo'\\''s.txt'`);
  });
});
