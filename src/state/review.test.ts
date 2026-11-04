import { describe, expect, it } from "vitest";
import { reviewPrompt, where } from "./review";
import type { ReviewComment, Tab } from "./tabs";

const comment = (c: Partial<ReviewComment>): ReviewComment => ({ id: 1, root: "/repo", path: "src/a.ts", from: 3, to: 4, code: "x\ny", text: "trocar", ...c });
const tab = (review: ReviewComment[], cwd: string | null = "/repo") => ({ review, cwd }) as unknown as Tab;

describe("reviewPrompt", () => {
  it("usa o caminho relativo à pasta do Claude e cita o comentário", () => {
    expect(reviewPrompt(tab([comment({})]))).toBe("Comentários de revisão:\n\nsrc/a.ts:3-4\n```\nx\ny\n```\n> trocar\n");
  });

  it("fora da pasta do Claude, usa o caminho absoluto", () => {
    expect(reviewPrompt(tab([comment({})], "/outro"))).toContain("/repo/src/a.ts:3-4");
  });

  it("troca a cerca quando o código já tem ```", () => {
    expect(reviewPrompt(tab([comment({ code: "```js\n```" })]))).toContain("~~~~\n```js\n```\n~~~~");
  });

  it("corta trechos longos em 40 linhas", () => {
    const code = Array.from({ length: 60 }, (_, i) => `l${i}`).join("\n");
    const out = reviewPrompt(tab([comment({ code })]));
    expect(out).toContain("l39\n…");
    expect(out).not.toContain("l40");
  });

  it("trecho removido diz a linha atual e as linhas do último commit", () => {
    expect(where(comment({ from: 10, to: 11, removed: { at: 12 } }), "a.ts")).toBe("a.ts:12 (trecho removido; linhas 10-11 no último commit)");
    expect(where(comment({ from: 5, to: 5 }), "a.ts")).toBe("a.ts:5");
  });
});
