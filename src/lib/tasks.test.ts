import { describe, expect, it } from "vitest";
import type { BgTask } from "./pty";
import { logSize, taskDuration, taskState } from "./tasks";

const task = (over: Partial<BgTask>): BgTask => ({
  id: "b1",
  command: "npm run dev",
  description: "Sobe o servidor",
  status: "running",
  exit_code: null,
  started: "2026-11-13T10:00:00Z",
  updated: null,
  size: 0,
  ...over,
});

describe("tarefas em segundo plano", () => {
  it("mostra o código de saída só quando falhou", () => {
    expect(taskState(task({ status: "failed", exit_code: 1 }))).toBe("Falhou (1)");
    expect(taskState(task({ status: "done", exit_code: 0 }))).toBe("Terminou");
    expect(taskState(task({ status: "ended" }))).toBe("Encerrada");
  });

  it("conta o tempo até agora ou até a última saída", () => {
    const start = Date.parse("2026-11-13T10:00:00Z");
    expect(taskDuration(task({}), start + 75_000)).toBe("1min 15s");
    expect(taskDuration(task({ status: "done", updated: start / 1000 + 30 }), start + 999_000)).toBe("30s");
    expect(taskDuration(task({ started: null }), start)).toBe("");
  });

  it("formata o tamanho do log", () => {
    expect(logSize(500)).toBe("500 B");
    expect(logSize(1536)).toBe("1,5 KB");
    expect(logSize(3 * 1024 * 1024)).toBe("3,0 MB");
  });
});
