/** Fila de tarefas uma depois da outra; `clear` descarta as que ainda não começaram. */
export function serialQueue() {
  let tail: Promise<void> = Promise.resolve();
  let generation = 0;
  return {
    add(task: () => Promise<unknown>): Promise<void> {
      const gen = generation;
      tail = tail.then(() => (gen === generation ? task().then(() => {}, () => {}) : undefined));
      return tail;
    },
    clear() {
      generation++;
    },
  };
}
