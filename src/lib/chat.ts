import { Channel, invoke } from "@tauri-apps/api/core";
import type { AgentStep } from "./pty";

/** Modo chat (experimental): o Claude da aba conversando em JSON, sem a interface de terminal. */
export interface ChatStarted {
  id: number;
  session_id: string;
  cwd: string;
  history: AgentStep[];
}

/** Encerra o Claude do terminal da aba e abre o mesmo em modo chat, na mesma sessão. */
export function chatStart(ptyId: number, onLine: (line: string) => void, onExit: (code: number, stderr: string) => void) {
  const line = new Channel<string>();
  line.onmessage = onLine;
  const exit = new Channel<[number, string]>();
  exit.onmessage = ([code, stderr]) => onExit(code, stderr);
  return invoke<ChatStarted>("chat_start", { ptyId, onLine: line, onExit: exit });
}

export const chatSend = (id: number, message: unknown) => invoke<void>("chat_send", { id, message });
export const chatStop = (id: number) => invoke<void>("chat_stop", { id });

/** Onde o ditado e o anexo escrevem quando a aba está no chat (registrado pelo ChatView). */
export const chatInputs = new Map<number, (text: string) => void>();

const MAX_RESULT = 800;

function clip(s: string, max: number) {
  s = s.trim();
  return s.length <= max ? s : `${s.slice(0, max)} …`;
}

/** Caminhos dentro da pasta da sessão ficam relativos; fora dela, a pasta pessoal vira "~". */
export function relative(text: string, cwd: string, home: string | null) {
  let out = cwd.length > 1 ? text.replaceAll(`${cwd.replace(/\/$/, "")}/`, "") : text;
  if (home && home.length > 1) out = out.replaceAll(`${home.replace(/\/$/, "")}/`, "~/");
  return out;
}

/** Mesmo resumo do popup dos agentes (ver agents.rs): no Bash, a descrição; o comando vai no detalhe. */
export function toolSummary(name: string, input: Record<string, unknown>, cwd: string, home: string | null) {
  const field = (k: string) => (typeof input?.[k] === "string" ? (input[k] as string) : null);
  if (name === "Bash") {
    const command = field("command");
    const detail = command ? clip(relative(command, cwd, home), 2000) : null;
    const desc = field("description");
    return desc?.trim() ? { summary: clip(desc, 600), detail } : { summary: detail ?? "", detail: null };
  }
  const main =
    {
      Read: field("file_path"),
      Edit: field("file_path"),
      Write: field("file_path"),
      NotebookEdit: field("notebook_path"),
      Grep: field("pattern"),
      Glob: field("pattern"),
      WebFetch: field("url"),
      WebSearch: field("query"),
      Agent: field("description"),
      Task: field("description"),
      Skill: field("skill"),
    }[name] ?? JSON.stringify(input);
  return { summary: clip(relative(main, cwd, home), 600), detail: null };
}

export function resultText(content: unknown) {
  if (typeof content === "string") return clip(content, MAX_RESULT);
  if (!Array.isArray(content)) return "";
  const text = content
    .map((c: { type?: string; text?: string }) => (c.type === "text" ? (c.text ?? "") : `[${c.type}]`))
    .join("\n");
  return clip(text, MAX_RESULT);
}
