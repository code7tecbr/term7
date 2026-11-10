import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { watch } from "vue";
import type { Tab } from "../state/tabs";
import { gitStatus } from "./git";
import type { PtySession } from "./pty";
import { readScreen } from "./screens";

// API local das extensões: o backend (api.rs) recebe pelo socket e repassa os
// pedidos pra cá, onde estão as abas e as telas dos terminais.

export interface ApiDeps {
  tabs: () => Tab[];
  activeId: () => number | null;
  session: (tab: Tab) => PtySession | undefined;
  title: (tab: Tab) => string;
  /** Avisa na tela o que uma extensão fez (o usuário precisa saber). */
  notify: (msg: string) => void;
}

/** Estado do Claude visto de fora: `idle` = aberto e parado; null = sem Claude. */
export type ApiClaudeState = "working" | "waiting" | "done" | "idle" | null;

export function claudeState(tab: Pick<Tab, "process" | "claude">): ApiClaudeState {
  if (tab.process !== "claude") return null;
  return tab.claude ?? "idle";
}

/** Só entrega com o Claude parado: esperando permissão, o Enter responderia o pedido. */
export const canDeliver = (state: ApiClaudeState) => state === "done" || state === "idle";

/** Texto da extensão sem sequências de controle (só quebra de linha e tab passam). */
export const sanitize = (text: string) => text.replace(/[\x00-\x08\x0b-\x1f\x7f]/g, "").trim();

const MAX_TEXT = 20_000;
const MAX_LINES = 500;

class ApiError extends Error {}

/** Mensagens esperando o Claude da aba terminar, por id da aba. */
const queues = new Map<number, string[]>();

async function deliver(deps: ApiDeps, tab: Tab, text: string) {
  const session = deps.session(tab);
  if (!session) throw new ApiError("a aba não tem terminal");
  // Colagem "entre colchetes" + Enter separado: o Claude trata como texto colado e envia.
  await session.write(`\x1b[200~${text}\x1b[201~`);
  await new Promise((r) => setTimeout(r, 80));
  await session.write("\r");
  deps.notify(`Extensão mandou uma mensagem pro Claude da aba "${deps.title(tab)}".`);
}

/** Entrega a próxima mensagem da fila se o Claude da aba estiver parado. */
async function flush(deps: ApiDeps, tab: Tab) {
  const queue = queues.get(tab.id);
  if (!queue?.length || !canDeliver(claudeState(tab))) return;
  const text = queue.shift()!;
  if (!queue.length) queues.delete(tab.id);
  await deliver(deps, tab, text).catch(() => {});
}

function findTab(deps: ApiDeps, params: Record<string, unknown>) {
  const tab = deps.tabs().find((t) => t.id === params.tab);
  if (!tab) throw new ApiError(`aba ${String(params.tab)} não existe (veja tabs.list)`);
  return tab;
}

const callerTab = (deps: ApiDeps, params: Record<string, unknown>) =>
  params.callerPty == null ? undefined : deps.tabs().find((t) => deps.session(t)?.id === params.callerPty);

export async function handle(deps: ApiDeps, method: string, params: Record<string, unknown>): Promise<unknown> {
  switch (method) {
    case "tabs.list": {
      const self = callerTab(deps, params);
      return Promise.all(
        deps.tabs().map(async (tab) => ({
          id: tab.id,
          title: deps.title(tab),
          cwd: tab.cwd,
          branch: tab.cwd ? ((await gitStatus(tab.cwd).catch(() => null))?.branch ?? null) : null,
          process: tab.process,
          claude: claudeState(tab),
          /** O Claude pediu algo ou terminou e você ainda não olhou. */
          attention: tab.attention,
          tag: tab.tag,
          paused: tab.paused,
          active: tab.id === deps.activeId(),
          self: tab === self,
          queued: queues.get(tab.id)?.length ?? 0,
        })),
      );
    }
    case "tabs.read": {
      const tab = findTab(deps, params);
      const session = deps.session(tab);
      const n = Math.min(Math.max(Number(params.lines) || 50, 1), MAX_LINES);
      const lines = session ? readScreen(session.id, n) : null;
      if (!lines) throw new ApiError("a tela da aba não está disponível");
      return { id: tab.id, lines };
    }
    case "tabs.send": {
      const tab = findTab(deps, params);
      const text = sanitize(String(params.text ?? ""));
      if (!text) throw new ApiError("mensagem vazia");
      if (text.length > MAX_TEXT) throw new ApiError(`mensagem grande demais (máx. ${MAX_TEXT} caracteres)`);
      if (tab === callerTab(deps, params)) throw new ApiError("não dá pra mandar mensagem pra própria aba");
      // Nunca vai pro shell: lá o Enter executaria o texto como comando.
      if (claudeState(tab) === null) throw new ApiError(`a aba "${deps.title(tab)}" não está com o Claude aberto`);
      const queue = queues.get(tab.id) ?? [];
      queues.set(tab.id, [...queue, text]);
      if (!queue.length && canDeliver(claudeState(tab))) {
        await flush(deps, tab);
        return { status: "sent" };
      }
      return { status: "queued", position: queue.length + 1 };
    }
    default:
      throw new ApiError(`método desconhecido: ${method}`);
  }
}

/** Atende os pedidos do backend e entrega as mensagens da fila quando o Claude para. */
export async function startApi(deps: ApiDeps) {
  watch(
    () => deps.tabs().map((t) => [t.id, claudeState(t)] as const),
    () => {
      for (const tab of deps.tabs()) flush(deps, tab);
      // Aba fechada: a fila dela não tem mais pra onde ir.
      for (const id of queues.keys()) if (!deps.tabs().some((t) => t.id === id)) queues.delete(id);
    },
  );
  await listen<{ id: number; method: string; params: Record<string, unknown> }>("api-request", async ({ payload }) => {
    const { id, method, params } = payload;
    try {
      const result = await handle(deps, method, params ?? {});
      await invoke("api_respond", { id, result, error: null });
    } catch (err) {
      const error = err instanceof ApiError ? err.message : `erro interno: ${err}`;
      await invoke("api_respond", { id, result: null, error }).catch(() => {});
    }
  });
}

/** Só pros testes: esvazia as filas. */
export const resetQueues = () => queues.clear();
