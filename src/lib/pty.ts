import { Channel, invoke } from "@tauri-apps/api/core";

export interface SpawnOptions {
  cols: number;
  rows: number;
  cwd?: string;
  onData: (data: Uint8Array) => void;
  onExit: (code: number) => void;
}

export interface PtyInfo {
  cwd: string | null;
  process: string | null;
}

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
