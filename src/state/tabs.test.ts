import { beforeEach, describe, expect, it } from "vitest";
import { activeTab, closeTab, goToTab, moveGroupTo, moveTabTo, newTab, restoreLayout, store, type Group } from "./tabs";

const group = (id: number, extra: Partial<Group> = {}): Group => ({ id, name: `g${id}`, color: null, tag: null, collapsed: false, ...extra }) as Group;

/** Ordem das abas como [nome, grupo]. */
const layout = () => store.tabs.map((t) => [t.name, t.groupId === null ? null : store.groups.find((g) => g.id === t.groupId)?.name]);

/** Regra do store: as abas de um grupo ficam sempre juntas. */
function expectContiguous() {
  const seen = new Set<number>();
  let last: number | null = null;
  for (const t of store.tabs) {
    if (t.groupId !== null && t.groupId !== last) {
      expect(seen.has(t.groupId), `grupo ${t.groupId} partido em dois`).toBe(false);
      seen.add(t.groupId);
    }
    last = t.groupId;
  }
}

beforeEach(() => {
  store.tabs = [];
  store.groups = [];
  store.activeId = null;
});

describe("newTab", () => {
  it("abre ao lado da atual, com a pasta e o grupo dela", () => {
    restoreLayout(
      [
        { cwd: "/a", groupId: 1, name: "a" },
        { cwd: "/b", groupId: null, name: "b" },
      ],
      [group(1)],
      0,
    );
    const tab = newTab();
    expect(store.tabs.indexOf(tab)).toBe(1);
    expect(tab.initialCwd).toBe("/a");
    expect(tab.groupId).toBe(store.tabs[0].groupId);
    expect(activeTab.value?.id).toBe(tab.id);
  });
});

describe("closeTab", () => {
  it("ativa a vizinha e remove o grupo que ficou vazio", () => {
    restoreLayout(
      [
        { cwd: "/a", groupId: 1, name: "a" },
        { cwd: "/b", groupId: null, name: "b" },
      ],
      [group(1)],
      0,
    );
    closeTab(store.tabs[0].id);
    expect(activeTab.value?.name).toBe("b");
    expect(store.groups).toHaveLength(0);
  });
});

describe("restoreLayout", () => {
  it("ignora tag desconhecida e aba de arquivo inválida", () => {
    restoreLayout([{ cwd: "/a", groupId: null, tag: "nao-existe" as never, view: 3, files: [] }], [], 0);
    expect(store.tabs[0].tag).toBeNull();
    expect(store.tabs[0].view).toBe(-1);
  });

  it("modo preview só fica em arquivo que dá pra visualizar", () => {
    const f = (path: string) => ({ root: "/r", path, origPath: null, mode: "preview" as const, line: null });
    restoreLayout([{ cwd: "/a", groupId: null, files: [f("README.md"), f("main.rs")] as never }], [], 0);
    expect(store.tabs[0].files.map((x) => x.mode)).toEqual(["preview", "file"]);
  });
});

describe("arrastar e soltar", () => {
  beforeEach(() => {
    restoreLayout(
      [
        { cwd: "/", groupId: 1, name: "a1" },
        { cwd: "/", groupId: 1, name: "a2" },
        { cwd: "/", groupId: null, name: "x" },
        { cwd: "/", groupId: 2, name: "b1" },
        { cwd: "/", groupId: 2, name: "b2" },
      ],
      [group(1, { name: "A" } as Partial<Group>), group(2, { name: "B" } as Partial<Group>)],
      0,
    );
  });
  const id = (name: string) => store.tabs.find((t) => t.name === name)!.id;
  const gid = (name: string) => store.groups.find((g) => g.name === name)!.id;

  it("soltar sobre aba de grupo coloca a aba no grupo", () => {
    moveTabTo(id("x"), { kind: "tab", id: id("b1"), pos: "after" });
    expect(layout()).toEqual([["a1", "A"], ["a2", "A"], ["b1", "B"], ["x", "B"], ["b2", "B"]]);
    expectContiguous();
  });

  it("soltar no fim tira a aba do grupo", () => {
    moveTabTo(id("a1"), { kind: "end" });
    expect(layout().at(-1)).toEqual(["a1", null]);
    expectContiguous();
  });

  it("grupo movido nunca cai no meio de outro grupo", () => {
    moveGroupTo(gid("A"), { kind: "tab", id: id("b2"), pos: "before" });
    expect(layout()).toEqual([["x", null], ["a1", "A"], ["a2", "A"], ["b1", "B"], ["b2", "B"]]);
    expectContiguous();
  });

  it("Alt+9 vai pra última aba visível", () => {
    store.groups.find((g) => g.name === "B")!.collapsed = true;
    goToTab(9);
    expect(activeTab.value?.name).toBe("x");
  });
});
