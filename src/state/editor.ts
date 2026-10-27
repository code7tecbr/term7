import { activeTab, type OpenFile, type Tab } from "./tabs";

const same = (a: Pick<OpenFile, "root" | "path">, b: Pick<OpenFile, "root" | "path">) =>
  a.root === b.root && a.path === b.path;

/** Abre o arquivo numa aba da área principal (ou vai pra ela, se já estiver aberta). */
export function openFile(file: Omit<OpenFile, "mode"> & { mode?: OpenFile["mode"] }, tab = activeTab.value) {
  if (!tab) return;
  const i = tab.files.findIndex((f) => same(f, file));
  if (i >= 0) {
    if (file.mode) tab.files[i].mode = file.mode;
    tab.view = i;
    return;
  }
  // Como no VS Code: a nova aba entra logo depois da que está aberta.
  const at = tab.view >= 0 ? tab.view + 1 : tab.files.length;
  tab.files.splice(at, 0, { mode: "file", ...file });
  tab.view = at;
}

export function closeFile(tab: Tab, index: number) {
  tab.files.splice(index, 1);
  if (tab.view === index) tab.view = Math.min(index, tab.files.length - 1);
  else if (tab.view > index) tab.view--;
}

export function showTerminal(tab: Tab) {
  tab.view = -1;
}

/** O arquivo mostrado agora na área principal, se for um arquivo. */
export const shownFile = (tab: Tab) => (tab.view >= 0 ? tab.files[tab.view] : undefined);
