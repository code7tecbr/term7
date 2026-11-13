import type { BgTask } from "./pty";

export const TASK_LABEL = { running: "Rodando", done: "Terminou", failed: "Falhou", stopped: "Parada", ended: "Encerrada" } as const;

/** Estado com o código de saída quando ajuda: "Falhou (1)". */
export function taskState(t: BgTask) {
  return t.status === "failed" && t.exit_code !== null ? `${TASK_LABEL.failed} (${t.exit_code})` : TASK_LABEL[t.status];
}

/** Tempo rodando: até agora, ou até a última escrita no log quando já acabou. */
export function taskDuration(t: BgTask, now: number) {
  if (!t.started) return "";
  const end = t.status === "running" || t.updated === null ? now : t.updated * 1000;
  const s = Math.max(0, Math.round((end - Date.parse(t.started)) / 1000));
  if (s < 60) return `${s}s`;
  const m = Math.floor(s / 60);
  return m < 60 ? `${m}min ${s % 60}s` : `${Math.floor(m / 60)}h ${m % 60}min`;
}

/** 1536 -> "1,5 KB". */
export function logSize(bytes: number) {
  if (bytes < 1024) return `${bytes} B`;
  const kb = bytes / 1024;
  if (kb < 1024) return `${kb.toFixed(kb < 10 ? 1 : 0).replace(".", ",")} KB`;
  return `${(kb / 1024).toFixed(1).replace(".", ",")} MB`;
}
