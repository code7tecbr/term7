import { describe, expect, it } from "vitest";
import { serialQueue } from "./queue";

const tick = () => new Promise((r) => setTimeout(r, 0));

describe("serialQueue", () => {
  it("a segunda só começa quando a primeira termina", async () => {
    const q = serialQueue();
    const log: string[] = [];
    let finishFirst!: () => void;
    q.add(() => (log.push("a começa"), new Promise<void>((r) => (finishFirst = () => (log.push("a termina"), r())))));
    const done = q.add(async () => void log.push("b começa"));
    await tick();
    expect(log).toEqual(["a começa"]);
    finishFirst();
    await done;
    expect(log).toEqual(["a começa", "a termina", "b começa"]);
  });

  it("clear descarta as que esperavam, mas aceita as novas", async () => {
    const q = serialQueue();
    const log: string[] = [];
    let finish!: () => void;
    q.add(() => new Promise<void>((r) => (finish = r)));
    q.add(async () => void log.push("descartada"));
    await tick();
    q.clear();
    finish();
    await q.add(async () => void log.push("nova"));
    expect(log).toEqual(["nova"]);
  });

  it("uma tarefa que falha não trava a fila", async () => {
    const q = serialQueue();
    const log: string[] = [];
    q.add(() => Promise.reject(new Error("x")));
    await q.add(async () => void log.push("seguiu"));
    expect(log).toEqual(["seguiu"]);
  });
});
