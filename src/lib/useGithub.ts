import { onBeforeUnmount, shallowRef, watch, type Ref } from "vue";
import { fetchPullRequest, fetchRuns, gitRepoInfo, overall, type GithubStatus, type RepoInfo } from "./github";

// Consulta a API do GitHub: bem mais espaçado que o polling do git local.
const IDLE_MS = 60_000;
const PENDING_MS = 15_000;

export function hasPending(status: GithubStatus | null) {
  if (status?.kind === "pr") return overall(status.pr.checks.map((c) => c.state)) === "pending";
  if (status?.kind === "runs") return overall(status.runs.map((r) => r.state)) === "pending";
  return false;
}

/** Info do repositório + PR e actions da branch atual. */
export function useGithub(root: Ref<string | null>, branch: Ref<string | null>, ahead: Ref<number>) {
  const repo = shallowRef<RepoInfo | null>(null);
  const github = shallowRef<GithubStatus | null>(null);
  let timer: ReturnType<typeof setTimeout> | undefined;
  // Descarta respostas de uma pasta/branch que já não é a atual.
  let generation = 0;
  let last = "";

  async function refresh() {
    clearTimeout(timer);
    const gen = ++generation;
    const dir = root.value;
    const name = branch.value;
    if (!dir || !name) {
      github.value = null;
      return;
    }
    if (!document.hidden) {
      try {
        let next = await fetchPullRequest(dir);
        if (!next && gen === generation) next = await fetchRuns(dir, name);
        if (gen !== generation) return;
        const serialized = JSON.stringify(next);
        if (serialized !== last) {
          last = serialized;
          github.value = next;
        }
      } catch {
        // Erro de rede ou da API: mantém o que já estava na tela.
        if (gen !== generation) return;
      }
    }
    timer = setTimeout(refresh, hasPending(github.value) ? PENDING_MS : IDLE_MS);
  }

  watch(
    root,
    async (dir) => {
      repo.value = null;
      if (!dir) return;
      const info = await gitRepoInfo(dir).catch(() => null);
      if (root.value === dir) repo.value = info;
    },
    { immediate: true },
  );

  watch(
    [root, branch],
    () => {
      github.value = null;
      last = "";
      refresh();
    },
    { immediate: true },
  );

  // Commits enviados (ahead diminuiu): o push dispara actions, então consulta já.
  watch(ahead, (now, before) => now < before && refresh());

  onBeforeUnmount(() => {
    clearTimeout(timer);
    generation++;
  });

  return { repo, github, refresh };
}
