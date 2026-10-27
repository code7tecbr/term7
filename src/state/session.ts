import { invoke } from "@tauri-apps/api/core";
import { restoreLayout, store, type Group, type SavedTab } from "./tabs";

interface SavedSession {
  version: 1;
  tabs: SavedTab[];
  groups: Group[];
  activeIndex: number;
}

export function snapshot(): string {
  const data: SavedSession = {
    version: 1,
    tabs: store.tabs.map((t) => ({
      cwd: t.cwd ?? t.initialCwd,
      groupId: t.groupId,
      name: t.name,
      color: t.color,
      tag: t.tag,
      files: t.files.map((f) => ({ ...f })),
      view: t.view,
      panelOpen: t.panelOpen,
      panelHeight: t.panelHeight,
    })),
    groups: store.groups.map((g) => ({ ...g })),
    activeIndex: store.tabs.findIndex((t) => t.id === store.activeId),
  };
  return JSON.stringify(data);
}

export const saveSession = (data = snapshot()) => invoke("session_save", { data });

/** Restaura a sessão anterior. Retorna false se não havia nada (ou estava inválido). */
export async function restoreSession(): Promise<boolean> {
  try {
    const raw = await invoke<string | null>("session_load");
    if (!raw) return false;
    const data = JSON.parse(raw) as SavedSession;
    if (data.version !== 1 || !Array.isArray(data.tabs) || !data.tabs.length) return false;
    restoreLayout(data.tabs, data.groups ?? [], data.activeIndex ?? 0);
    return store.tabs.length > 0;
  } catch (err) {
    console.warn("[term7] não deu pra restaurar a sessão", err);
    return false;
  }
}
