import { Channel, invoke } from "@tauri-apps/api/core";

export interface SpawnOptions {
  cols: number;
  rows: number;
  cwd?: string;
  onData: (data: Uint8Array) => void;
  onExit: (code: number) => void;
}

/** Estado do Claude CLI na aba, avisado pelos hooks (ver claude.rs). */
export type ClaudeState = "working" | "waiting" | "done";

export interface PtyInfo {
  cwd: string | null;
  process: string | null;
  claude: ClaudeState | null;
}

export interface ClaudeHooksStatus {
  installed: boolean;
  settingsPath: string;
}

export const claudeHooksStatus = (id: number) => invoke<ClaudeHooksStatus>("claude_hooks_status", { id });
export interface McpServer {
  name: string;
  /** Endereço/comando sem query nem argumentos (podem ter token). */
  target: string;
  status: "connected" | "needs_auth" | "failed" | "unknown";
  detail: string | null;
  /** Escopo de usuário (pedido pelo botão "Ver os globais"). */
  global: boolean;
}

/** MCPs do projeto do Claude da aba, ou os globais (`claude mcp get` com o binário, ambiente e pasta dele). */
export const claudeMcpList = (id: number, global = false) => invoke<McpServer[]>("claude_mcp_list", { id, global });

export const claudeHooksInstall = (id: number) => invoke<ClaudeHooksStatus>("claude_hooks_install", { id });

export interface PtySession {
  id: number;
  write: (data: string) => Promise<void>;
  resize: (cols: number, rows: number) => Promise<void>;
  kill: () => Promise<void>;
  info: () => Promise<PtyInfo>;
}

// Abre um shell no backend Rust. A saída chega como bytes brutos pelo Channel
// (sem JSON no meio), e o xterm decodifica o UTF-8 sozinho.
export async function spawnPty(opts: SpawnOptions): Promise<PtySession> {
  const data = new Channel<ArrayBuffer>();
  data.onmessage = (chunk) => opts.onData(new Uint8Array(chunk));

  const exit = new Channel<number>();
  exit.onmessage = opts.onExit;

  const id = await invoke<number>("pty_spawn", {
    cols: opts.cols,
    rows: opts.rows,
    cwd: opts.cwd ?? null,
    onData: data,
    onExit: exit,
  });

  return {
    id,
    write: (input) => invoke("pty_write", { id, data: input }),
    resize: (cols, rows) => invoke("pty_resize", { id, cols, rows }),
    kill: () => invoke("pty_kill", { id }),
    info: () => invoke<PtyInfo>("pty_info", { id }),
  };
}
