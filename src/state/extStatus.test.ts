import { describe, expect, it } from "vitest";
import { ago, isStale, parseStatusSet, sorted, worst } from "./extStatus";

const NOW = Date.parse("2026-11-10T20:00:00Z");

describe("parseStatusSet", () => {
  it("aceita o formato do statusbot e descarta o que não serve", () => {
    const s = parseStatusSet(
      {
        source: "statusbot",
        title: "Serviços",
        checkedAt: "2026-11-10T19:58:00Z",
        ttlSeconds: 180,
        items: [
          { label: "My API", status: "down", since: "2026-11-10T19:53:02.000Z", detail: "HTTP 502", link: "javascript:alert(1)" },
          { label: "Cloudflare · D1", status: "explodiu", link: "https://stspg.io/abc" },
        ],
      },
      NOW,
    );
    expect(s.items).toEqual([
      { label: "My API", status: "down", since: "2026-11-10T19:53:02.000Z", detail: "HTTP 502", link: undefined },
      { label: "Cloudflare · D1", status: "unknown", since: undefined, detail: undefined, link: "https://stspg.io/abc" },
    ]);
    expect(s.ttlMs).toBe(180_000);
    expect(s.title).toBe("Serviços");
  });

  it("recusa source e items inválidos", () => {
    expect(() => parseStatusSet({ source: "a b", items: [] }, NOW)).toThrow(/source/);
    expect(() => parseStatusSet({ source: "x", items: "nada" }, NOW)).toThrow(/items/);
    expect(() => parseStatusSet({ source: "x", items: [{ status: "up" }] }, NOW)).toThrow(/label/);
  });
});

describe("worst e isStale", () => {
  const base = parseStatusSet({ source: "x", ttlSeconds: 60, items: [{ label: "a", status: "up" }, { label: "b", status: "maintenance" }] }, NOW);

  it("pega o pior estado", () => {
    expect(worst(base, NOW)).toBe("maintenance");
    expect(worst({ ...base, items: [] }, NOW)).toBe("up");
  });

  it("erro ou dado velho viram sem leitura", () => {
    expect(worst({ ...base, error: "fora" }, NOW)).toBe("unknown");
    expect(isStale(base, NOW + 61_000)).toBe(true);
    expect(worst(base, NOW + 61_000)).toBe("unknown");
  });

  it("ordena do pior pro melhor", () => {
    const items = sorted([
      { label: "a", status: "up" },
      { label: "b", status: "down" },
      { label: "c", status: "degraded" },
    ]);
    expect(items.map((i) => i.label)).toEqual(["b", "c", "a"]);
  });
});

describe("ago", () => {
  it("formata o tempo", () => {
    expect(ago("2026-11-10T19:59:40Z", NOW)).toBe("agora");
    expect(ago("2026-11-10T19:48:00Z", NOW)).toBe("há 12 min");
    expect(ago("2026-11-10T15:00:00Z", NOW)).toBe("há 5 h");
    expect(ago("2026-11-06T20:00:00Z", NOW)).toBe("há 4 d");
    expect(ago(null, NOW)).toBe("");
  });
});
