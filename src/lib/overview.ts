import { invoke } from "@tauri-apps/api/core";
import type { Group, Tab } from "../state/tabs";
import { claudeState, type ApiClaudeState } from "./api";
import type { Agent } from "./pty";

/** O que o Claude da aba fez por último, lido do histórico (overview.rs). */
export type Activity = { kind: "text"; text: string } | { kind: "tool"; name: string; detail: string | null };
export interface Peek {
  config_dir: string | null;
  activity: Activity | null;
}
export const claudePeek = (id: number) => invoke<Peek | null>("claude_peek", { id });

export type CardState = Exclude<ApiClaudeState, null>;
export const STATE_LABEL: Record<CardState, string> = {
  working: "trabalhando",
  waiting: "esperando você",
  done: "pronto",
  idle: "ocioso",
};

export interface Section {
  group: Group | null;
  tabs: Tab[];
}

/** Abas com Claude, por grupo na ordem da barra lateral; as soltas vão no fim. */
export function sections(tabs: Tab[], groups: Group[]): { sections: Section[]; others: number } {
  const byGroup = new Map<number | null, Tab[]>();
  let others = 0;
  for (const tab of tabs) {
    if (claudeState(tab) === null) {
      others++;
      continue;
    }
    const key = groups.some((g) => g.id === tab.groupId) ? tab.groupId : null;
    if (!byGroup.has(key)) byGroup.set(key, []);
    byGroup.get(key)!.push(tab);
  }
  const list: Section[] = [];
  for (const [key, list_] of byGroup) if (key !== null) list.push({ group: groups.find((g) => g.id === key)!, tabs: list_ });
  const loose = byGroup.get(null);
  if (loose) list.push({ group: null, tabs: loose });
  return { sections: list, others };
}

/** "~/.claude-code7" -> "code7"; a pasta padrão (~/.claude) vira "padrão". */
export function accountLabel(configDir: string | null): string | null {
  if (!configDir) return null;
  const base = configDir.replace(/\/+$/, "").split("/").pop() ?? "";
  if (base === ".claude") return "padrão";
  return base.replace(/^\./, "").replace(/^claude[-_.]?/, "") || base;
}

/** Subagentes pendurados embaixo do card: só enquanto rodam (terminou, o card encolhe de volta). */
export function childAgents(agents: Agent[], max = 4): Agent[] {
  const started = (a: Agent) => Date.parse(a.started ?? "") || 0;
  return agents
    .filter((a) => a.status === "running")
    .sort((a, b) => started(a) - started(b))
    .slice(0, max);
}

/** Lista do popup: rodando primeiro, depois os mais recentes. */
export function sortAgents(agents: Agent[]): Agent[] {
  const time = (a: Agent) => Date.parse(a.updated ?? a.started ?? "") || 0;
  return [...agents].sort((a, b) => Number(b.status === "running") - Number(a.status === "running") || time(b) - time(a));
}

/** "45s", "3min", "1h 2min". */
export function elapsed(ms: number): string {
  const s = Math.max(0, Math.floor(ms / 1000));
  if (s < 60) return `${s}s`;
  const m = Math.floor(s / 60);
  return m < 60 ? `${m}min` : `${Math.floor(m / 60)}h ${m % 60}min`;
}

/** Desde quando cada aba está no estado atual (o "trabalhando há 3min" do card). */
const since = new Map<number, { state: CardState; at: number }>();

export function noteState(tabId: number, state: ApiClaudeState, now = Date.now()) {
  if (state === null) return void since.delete(tabId);
  if (since.get(tabId)?.state !== state) since.set(tabId, { state, at: now });
}

export const stateSince = (tabId: number) => since.get(tabId)?.at ?? null;
