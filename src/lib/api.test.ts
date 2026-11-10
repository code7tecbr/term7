import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(async () => ({ branch: "feat/x" })) }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn() }));
const { handle, sanitize, claudeState, resetQueues } = await import("./api");
const { lastLines } = await import("./screens");

type FakeTab = { id: number; process: string | null; claude: "working" | "waiting" | "done" | null; cwd: string | null };

function setup(tabs: FakeTab[]) {
  const writes: Record<number, string[]> = {};
  const sessions = new Map(tabs.map((t) => [t.id, { id: t.id + 100, write: async (d: string) => void (writes[t.id] ??= []).push(d) }]));
  const deps = {
    tabs: () => tabs as never[],
    activeId: () => tabs[0]?.id ?? null,
    session: (t: FakeTab) => sessions.get(t.id) as never,
    title: (t: FakeTab) => `aba${t.id}`,
    notify: vi.fn(),
  };
  return { deps: deps as never, writes };
}

beforeEach(() => resetQueues());

describe("tabs.send", () => {
  it("manda com Enter quando o Claude está parado", async () => {
    const { deps, writes } = setup([{ id: 1, process: "claude", claude: "done", cwd: "/r" }]);
    expect(await handle(deps, "tabs.send", { tab: 1, text: "chegou review" })).toEqual({ status: "sent" });
    expect(writes[1]).toEqual(["\x1b[200~chegou review\x1b[201~", "\r"]);
  });

  it("enfileira enquanto trabalha ou espera permissão", async () => {
    const { deps, writes } = setup([{ id: 1, process: "claude", claude: "waiting", cwd: "/r" }]);
    expect(await handle(deps, "tabs.send", { tab: 1, text: "a" })).toEqual({ status: "queued", position: 1 });
    expect(await handle(deps, "tabs.send", { tab: 1, text: "b" })).toEqual({ status: "queued", position: 2 });
    expect(writes[1]).toBeUndefined();
  });

  it("nunca manda pro shell nem pra própria aba", async () => {
    const { deps } = setup([
      { id: 1, process: "zsh", claude: null, cwd: "/r" },
      { id: 2, process: "claude", claude: "done", cwd: "/r" },
    ]);
    await expect(handle(deps, "tabs.send", { tab: 1, text: "rm -rf x" })).rejects.toThrow(/não está com o Claude/);
    await expect(handle(deps, "tabs.send", { tab: 2, text: "oi", callerPty: 102 })).rejects.toThrow(/própria aba/);
    await expect(handle(deps, "tabs.send", { tab: 9, text: "oi" })).rejects.toThrow(/não existe/);
  });
});

describe("tabs.list", () => {
  it("mostra o estado do Claude e marca a aba de quem chamou", async () => {
    const { deps } = setup([
      { id: 1, process: "claude", claude: null, cwd: "/r" },
      { id: 2, process: "zsh", claude: null, cwd: null },
    ]);
    const list = (await handle(deps, "tabs.list", { callerPty: 101 })) as Record<string, unknown>[];
    expect(list.map((t) => [t.id, t.claude, t.self, t.branch])).toEqual([
      [1, "idle", true, "feat/x"],
      [2, null, false, null],
    ]);
  });
});

describe("auxiliares", () => {
  it("tira sequências de controle", () => {
    expect(sanitize(" oi\x1b[31m\x07\nlinha\t2 ")).toBe("oi[31m\nlinha\t2");
  });

  it("estado do Claude só com o Claude aberto", () => {
    expect(claudeState({ process: "zsh", claude: "done" })).toBeNull();
    expect(claudeState({ process: "claude", claude: "working" })).toBe("working");
  });

  it("últimas linhas sem as vazias do fim", () => {
    expect(lastLines(["a", "b", "c", "", "  "], 2)).toEqual(["b", "c"]);
  });
});
