import { describe, expect, it } from "vitest";
import { URL_PATTERN } from "./links";

const urls = (text: string) => [...text.matchAll(new RegExp(URL_PATTERN.source, "g"))].map((m) => m[0]);

describe("URL_PATTERN", () => {
  it("acha links web sem a pontuação final", () => {
    expect(urls("veja https://github.com/org/repo/pull/12.")).toEqual(["https://github.com/org/repo/pull/12"]);
  });

  it("acha relatórios HTML locais em file:///", () => {
    expect(urls("abre file:///home/user/projeto/thoughts/board/triage/pr-626-review.html")).toEqual([
      "file:///home/user/projeto/thoughts/board/triage/pr-626-review.html",
    ]);
  });

  it("ignora caminho solto e file:// com host", () => {
    expect(urls("/home/user/x.html e file://host/x.html")).toEqual([]);
  });
});
