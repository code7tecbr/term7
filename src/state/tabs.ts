import { computed, reactive } from "vue";
import type { ClaudeState } from "../lib/pty";

export const COLORS = ["#7aa2f7", "#9ece6a", "#e0af68", "#f7768e", "#bb9af7", "#7dcfff", "#ff9e64"];

/** Etapas do fluxo de trabalho, na ordem em que acontecem. */
export const TAGS = [
  { id: "discovery", label: "discovery", color: "#ff9e64" },
  { id: "spec", label: "spec", color: "#7dcfff" },
  { id: "plan", label: "plan", color: "#73daca" },
  { id: "code", label: "code", color: "#7aa2f7" },
  { id: "review", label: "review", color: "#e0af68" },
  { id: "test", label: "test", color: "#9ece6a" },
  { id: "merge", label: "close/merge", color: "#bb9af7" },
] as const;

export type TagId = (typeof TAGS)[number]["id"];
export type Tag = (typeof TAGS)[number];

export const findTag = (id: string | null) => TAGS.find((t) => t.id === id) ?? null;

export interface Group {
  id: number;
  name: string;
  color: string;
  tag: TagId | null;
  collapsed: boolean;
}

/** file = código; changes = com as alterações; preview = Markdown/HTML renderizado. */
export type FileMode = "file" | "changes" | "preview";

/** Arquivos que dá pra ver renderizados (Markdown e HTML). */
export const canPreview = (path: string) => /\.(md|markdown|html?)$/i.test(path);

/** Arquivo aberto numa aba da área principal. */
export interface OpenFile {
  root: string;
  /** Caminho relativo à raiz do repositório. */
  path: string;
  /** Nome antigo, quando o arquivo foi renomeado (pra comparar com o HEAD). */
  origPath: string | null;
  mode: FileMode;
  /** Linha pra mostrar ao abrir (1 = primeira), quando veio de `arquivo:42`. */
  line?: number;
}

export interface Tab {
  id: number;
  /** Pasta onde o shell nasce. */
  initialCwd: string | null;
  /** Pasta atual, atualizada por polling. */
  cwd: string | null;
  /** Processo em primeiro plano. */
  process: string | null;
  /** Estado do Claude CLI rodando na aba (via hooks). */
  claude: ClaudeState | null;
  /** O Claude pediu algo ou terminou enquanto a aba não estava à vista. */
  attention: "waiting" | "done" | null;
  groupId: number | null;
  /** Nome dado pelo usuário. null = título automático (pasta/processo). */
  name: string | null;
  color: string | null;
  tag: TagId | null;
  /** Arquivos abertos em abas ao lado do terminal. */
  files: OpenFile[];
  /** O que a área principal mostra: -1 = terminal, senão o índice em files. */
  view: number;
  /** Terminal inferior (como no VS Code). */
  panelAlive: boolean;
  panelOpen: boolean;
  /** Fração da altura ocupada pelo terminal inferior. */
  panelHeight: number;
  panelCwd: string | null;
  focusedPane: "main" | "panel";
}

export type SavedTab = Pick<Tab, "cwd" | "groupId"> &
  Partial<Pick<Tab, "name" | "color" | "tag" | "panelOpen" | "panelHeight" | "files" | "view">>;

function makeTab(fields: Partial<Tab>): Tab {
  return {
    id: nextTabId++,
    initialCwd: null,
    cwd: null,
    process: null,
    claude: null,
    attention: null,
    groupId: null,
    name: null,
    color: null,
    tag: null,
    files: [],
    view: -1,
    panelAlive: false,
    panelOpen: false,
    panelHeight: 0.35,
    panelCwd: null,
    focusedPane: "main",
    ...fields,
  };
}

// Regra: as abas de um grupo ficam sempre juntas (contíguas) na lista.
export const store = reactive({
  tabs: [] as Tab[],
  groups: [] as Group[],
  activeId: null as number | null,
});

let nextTabId = 1;
let nextGroupId = 1;

export const activeTab = computed(() => store.tabs.find((t) => t.id === store.activeId) ?? null);

export const findGroup = (id: number | null) =>
  id === null ? undefined : store.groups.find((g) => g.id === id);

const findTab = (id: number) => store.tabs.find((t) => t.id === id);

export const isVisible = (tab: Tab) => !findGroup(tab.groupId)?.collapsed;

export const visibleTabs = () => store.tabs.filter(isVisible);

function lastIndexOfGroup(groupId: number) {
  let last = -1;
  store.tabs.forEach((t, i) => t.groupId === groupId && (last = i));
  return last;
}

/** Move a aba pra posição `index` (contada antes de remover a aba). */
function moveTo(tab: Tab, index: number) {
  const from = store.tabs.indexOf(tab);
  store.tabs.splice(from, 1);
  store.tabs.splice(from < index ? index - 1 : index, 0, tab);
}

function pruneGroups() {
  store.groups = store.groups.filter((g) => store.tabs.some((t) => t.groupId === g.id));
}

export function activate(id: number) {
  const tab = findTab(id);
  if (!tab) return;
  const group = findGroup(tab.groupId);
  if (group) group.collapsed = false;
  store.activeId = id;
}

/** Abre uma aba ao lado da atual, herdando a pasta e o grupo dela. */
export function newTab(cwd?: string | null) {
  const current = activeTab.value;
  const tab = makeTab({
    initialCwd: cwd ?? current?.cwd ?? null,
    groupId: current?.groupId ?? null,
  });
  const at = current ? store.tabs.indexOf(current) + 1 : store.tabs.length;
  store.tabs.splice(at, 0, tab);
  activate(tab.id);
  return tab;
}

export function closeTab(id: number) {
  const index = store.tabs.findIndex((t) => t.id === id);
  if (index < 0) return;
  store.tabs.splice(index, 1);
  if (store.activeId === id) {
    const next = store.tabs[index] ?? store.tabs[index - 1];
    store.activeId = null;
    if (next) activate(next.id);
  }
  pruneGroups();
}

export function closeOtherTabs(id: number) {
  store.tabs.filter((t) => t.id !== id).forEach((t) => closeTab(t.id));
}

export function cycleTab(delta: number) {
  const tabs = visibleTabs();
  if (!tabs.length) return;
  const i = tabs.findIndex((t) => t.id === store.activeId);
  activate(tabs[(i + delta + tabs.length) % tabs.length].id);
}

/** Alt+1..8 vai pra aba N; Alt+9 vai pra última. */
export function goToTab(n: number) {
  const tabs = visibleTabs();
  const tab = n === 9 ? tabs[tabs.length - 1] : tabs[n - 1];
  if (tab) activate(tab.id);
}

/** Recria abas e grupos salvos. As abas nascem nas pastas onde estavam. */
export function restoreLayout(
  tabs: SavedTab[],
  groups: Group[],
  activeIndex: number,
) {
  const groupIds = new Map<number, number>();
  for (const g of groups) {
    const id = nextGroupId++;
    groupIds.set(g.id, id);
    store.groups.push({ ...g, id, tag: findTag(g.tag ?? null)?.id ?? null });
  }
  for (const t of tabs) {
    store.tabs.push(
      makeTab({
        initialCwd: t.cwd,
        cwd: t.cwd,
        groupId: t.groupId === null ? null : (groupIds.get(t.groupId) ?? null),
        name: t.name ?? null,
        color: t.color ?? null,
        // Ignora tag desconhecida (sessão de outra versão).
        tag: findTag(t.tag ?? null)?.id ?? null,
        files: (t.files ?? []).map((f) => ({
          ...f,
          mode: f.mode === "changes" || (f.mode === "preview" && canPreview(f.path)) ? f.mode : "file",
        })),
        view: t.view !== undefined && t.view < (t.files?.length ?? 0) ? t.view : -1,
        panelAlive: !!t.panelOpen,
        panelOpen: !!t.panelOpen,
        panelHeight: t.panelHeight ?? 0.35,
        panelCwd: t.panelOpen ? t.cwd : null,
      }),
    );
  }
  pruneGroups();
  const active = store.tabs[activeIndex] ?? store.tabs[0];
  if (active) activate(active.id);
}

// ---- arrastar e soltar ----

/** Onde um item arrastado vai cair. */
export type DropTarget =
  | { kind: "tab"; id: number; pos: "before" | "after" }
  | { kind: "group"; id: number; pos: "before" | "inside" | "after" }
  | { kind: "end" };

function idAfter(list: Tab[], tabId: number) {
  const i = list.findIndex((t) => t.id === tabId);
  return list[i + 1]?.id ?? null;
}

function insertBefore(list: Tab[], items: Tab[], beforeId: number | null) {
  const i = beforeId === null ? -1 : list.findIndex((t) => t.id === beforeId);
  list.splice(i < 0 ? list.length : i, 0, ...items);
  store.tabs = list;
}

/**
 * Solta uma aba: sobre uma aba de grupo ou dentro do grupo, ela entra nele;
 * em cima do grupo, numa aba solta ou no fim da lista, ela fica fora de grupos.
 */
export function moveTabTo(tabId: number, target: DropTarget) {
  const tab = findTab(tabId);
  if (!tab) return;
  const others = store.tabs.filter((t) => t.id !== tabId);
  let before: number | null = null;
  let groupId: number | null = null;

  if (target.kind === "tab") {
    const ref = findTab(target.id);
    if (!ref || ref.id === tabId) return;
    groupId = ref.groupId;
    before = target.pos === "before" ? ref.id : idAfter(others, ref.id);
  } else if (target.kind === "group") {
    const members = others.filter((t) => t.groupId === target.id);
    if (!members.length) {
      // Grupo só com a aba arrastada: fica onde está, só entra/sai do grupo.
      groupId = target.pos === "before" ? null : target.id;
      before = idAfter(store.tabs, tabId);
    } else if (target.pos === "before") {
      before = members[0].id;
    } else {
      groupId = target.id;
      before = idAfter(others, members[members.length - 1].id);
    }
  }

  tab.groupId = groupId;
  insertBefore(others, [tab], before);
  const group = findGroup(groupId);
  if (group && store.activeId === tabId) group.collapsed = false;
  pruneGroups();
}

/** Move um grupo inteiro. Ele nunca cai no meio de outro grupo. */
export function moveGroupTo(groupId: number, target: DropTarget) {
  const members = store.tabs.filter((t) => t.groupId === groupId);
  const others = store.tabs.filter((t) => t.groupId !== groupId);
  let before: number | null = null;

  if (target.kind === "tab") {
    const ref = findTab(target.id);
    if (!ref || ref.groupId === groupId) return;
    if (ref.groupId !== null) return moveGroupTo(groupId, { kind: "group", id: ref.groupId, pos: target.pos });
    before = target.pos === "before" ? ref.id : idAfter(others, ref.id);
  } else if (target.kind === "group") {
    if (target.id === groupId) return;
    const refMembers = others.filter((t) => t.groupId === target.id);
    if (!refMembers.length) return;
    before =
      target.pos === "before" ? refMembers[0].id : idAfter(others, refMembers[refMembers.length - 1].id);
  }
  insertBefore(others, members, before);
}

// ---- nome e cor da aba ----

export function renameTab(tabId: number, name: string | null) {
  const tab = findTab(tabId);
  if (tab) tab.name = name?.trim() || null;
}

export function setTabTag(tabId: number, tag: TagId | null) {
  const tab = findTab(tabId);
  if (tab) tab.tag = tag;
}

export function setTabColor(tabId: number, color: string | null) {
  const tab = findTab(tabId);
  if (tab) tab.color = color;
}

// ---- terminal inferior ----

/**
 * Ctrl+' (igual ao VS Code): abre o terminal inferior e foca nele;
 * se já está focado, esconde (o shell continua vivo) e volta pro principal.
 */
/** Mostra e foca o terminal inferior da aba ativa, criando se preciso. */
export function openPanel() {
  const tab = activeTab.value;
  if (!tab) return;
  if (!tab.panelAlive) {
    tab.panelCwd = tab.cwd ?? tab.initialCwd;
    tab.panelAlive = true;
  }
  tab.panelOpen = true;
  tab.focusedPane = "panel";
  return tab;
}

export function togglePanel() {
  const tab = activeTab.value;
  if (tab?.panelOpen && tab.focusedPane === "panel") {
    tab.panelOpen = false;
    tab.focusedPane = "main";
  } else {
    openPanel();
  }
}

/** Fecha de vez o terminal inferior (mata o shell dele). */
export function closePanel(tabId: number) {
  const tab = findTab(tabId);
  if (!tab) return;
  tab.panelAlive = false;
  tab.panelOpen = false;
  tab.focusedPane = "main";
}

// ---- grupos ----

export function createGroup(tabId: number) {
  const id = nextGroupId++;
  const group: Group = {
    id,
    name: `Grupo ${id}`,
    color: COLORS[(id - 1) % COLORS.length],
    tag: null,
    collapsed: false,
  };
  store.groups.push(group);
  setTabGroup(tabId, id);
  return group;
}

export function setTabGroup(tabId: number, groupId: number | null) {
  const tab = findTab(tabId);
  if (!tab || tab.groupId === groupId) return;

  if (groupId === null) {
    // Sai do grupo e vai pra logo depois dele, mantendo o grupo contíguo.
    const last = lastIndexOfGroup(tab.groupId!);
    tab.groupId = null;
    moveTo(tab, last + 1);
  } else {
    const last = lastIndexOfGroup(groupId);
    tab.groupId = groupId;
    if (last >= 0) moveTo(tab, last + 1);
    const group = findGroup(groupId);
    if (group) group.collapsed = false;
  }
  pruneGroups();
}

export function ungroup(groupId: number) {
  store.tabs.forEach((t) => t.groupId === groupId && (t.groupId = null));
  pruneGroups();
}

export function closeGroup(groupId: number) {
  store.tabs.filter((t) => t.groupId === groupId).forEach((t) => closeTab(t.id));
}

export function newTabInGroup(groupId: number) {
  const members = store.tabs.filter((t) => t.groupId === groupId);
  const last = members[members.length - 1];
  if (!last) return;
  store.activeId = last.id;
  newTab();
}

export function toggleCollapsed(groupId: number) {
  const group = findGroup(groupId);
  if (!group) return;
  if (!group.collapsed && activeTab.value?.groupId === groupId) {
    // A aba ativa vai sumir: pula pra aba visível mais próxima fora do grupo.
    const index = store.tabs.indexOf(activeTab.value);
    const outside = (t: Tab) => t.groupId !== groupId && isVisible(t);
    const next =
      store.tabs.slice(index).find(outside) ?? [...store.tabs.slice(0, index)].reverse().find(outside);
    if (!next) return; // não dá pra recolher o único grupo visível
    store.activeId = next.id;
  }
  group.collapsed = !group.collapsed;
}

export function renameGroup(groupId: number, name: string) {
  const group = findGroup(groupId);
  if (group) group.name = name.trim() || group.name;
}

export function setGroupTag(groupId: number, tag: TagId | null) {
  const group = store.groups.find((g) => g.id === groupId);
  if (group) group.tag = tag;
}

export function setGroupColor(groupId: number, color: string) {
  const group = findGroup(groupId);
  if (group) group.color = color;
}

// ---- títulos ----

const SHELLS = new Set(["zsh", "bash", "fish", "sh", "dash", "nu", "pwsh"]);

export function prettyPath(path: string, home: string | null) {
  if (home && (path === home || path.startsWith(home + "/"))) return "~" + path.slice(home.length);
  return path;
}

function basename(path: string) {
  return path.split("/").filter(Boolean).pop() ?? "/";
}

export function tabTitle(tab: Tab, home: string | null) {
  if (tab.name) return tab.name;
  const pretty = tab.cwd ? prettyPath(tab.cwd, home) : null;
  const dir = pretty === "~" ? "~" : pretty ? basename(pretty) : null;
  if (tab.process && !SHELLS.has(tab.process)) return dir ? `${tab.process} · ${dir}` : tab.process;
  return dir ?? "term7";
}
