import { describe, expect, it } from "vitest";
import type { Group, Tab } from "../state/tabs";
import { accountLabel, childAgents, elapsed, noteState, sections, sortAgents, stateSince } from "./overview";
import type { Agent } from "./pty";

const tab = (id: number, fields: Partial<Tab> = {}) => ({ id, process: "claude", claude: null, groupId: null, ...fields }) as Tab;
const group = (id: number) => ({ id, name: `g${id}` }) as Group;
const agent = (id: string, status: Agent["status"], updated: string) => ({ id, status, started: updated, updated }) as Agent;

describe("sections", () => {
  it("agrupa só as abas com Claude, na ordem dos grupos, e soltas no fim", () => {
    const tabs = [tab(1), tab(2, { groupId: 7 }), tab(3, { process: "zsh" }), tab(4, { groupId: 7 }), tab(5, { groupId: 8 })];
    const { sections: list, others } = sections(tabs, [group(7), group(8)]);
    expect(list.map((s) => [s.group?.id ?? null, s.tabs.map((t) => t.id)])).toEqual([
      [7, [2, 4]],
      [8, [5]],
      [null, [1]],
    ]);
    expect(others).toBe(1);
  });

  it("grupo que não existe mais conta como aba solta", () => {
    expect(sections([tab(1, { groupId: 99 })], []).sections).toEqual([{ group: null, tabs: [tab(1, { groupId: 99 })] }]);
  });
});

describe("accountLabel", () => {
  it("dá nome curto à pasta de config", () => {
    expect(accountLabel("/home/x/.claude")).toBe("padrão");
    expect(accountLabel("/home/x/.claude-code7/")).toBe("code7");
    expect(accountLabel("/opt/trabalho")).toBe("trabalho");
    expect(accountLabel(null)).toBeNull();
  });
});

describe("agentes", () => {
  const list = [
    agent("velho", "done", "2026-11-18T11:00:00Z"),
    agent("recente", "done", "2026-11-18T11:58:00Z"),
    agent("rodando", "running", "2026-11-18T10:00:00Z"),
    agent("parado", "stalled", "2026-11-18T11:59:00Z"),
  ];

  it("embaixo do card ficam só os que estão rodando", () => {
    expect(childAgents(list).map((a) => a.id)).toEqual(["rodando"]);
  });

  it("no popup, rodando primeiro e depois os mais recentes", () => {
    expect(sortAgents(list).map((a) => a.id)).toEqual(["rodando", "parado", "recente", "velho"]);
  });
});

describe("tempo no estado", () => {
  it("só reinicia quando o estado muda", () => {
    noteState(1, "working", 1000);
    noteState(1, "working", 5000);
    expect(stateSince(1)).toBe(1000);
    noteState(1, "done", 9000);
    expect(stateSince(1)).toBe(9000);
    noteState(1, null);
    expect(stateSince(1)).toBeNull();
  });

  it("formata a duração", () => {
    expect(elapsed(45_000)).toBe("45s");
    expect(elapsed(3 * 60_000)).toBe("3min");
    expect(elapsed(62 * 60_000)).toBe("1h 2min");
  });
});
