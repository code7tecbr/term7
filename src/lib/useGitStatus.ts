import { onBeforeUnmount, ref, shallowRef, watch, type Ref } from "vue";
import { gitStatus, type GitStatus } from "./git";

const POLL_MS = 2500;

/** Status do git da pasta atual, atualizado ao trocar de pasta e por polling. */
export function useGitStatus(cwd: Ref<string | null>) {
  const status = shallowRef<GitStatus | null>(null);
  const loading = ref(false);
  let last = "";
  let rerun = false;

  async function refresh() {
    if (loading.value) {
      rerun = true;
      return;
    }
    const dir = cwd.value;
    if (!dir) {
      status.value = null;
      last = "";
      return;
    }
    loading.value = true;
    try {
      const next = await gitStatus(dir).catch(() => null);
      // Só troca o objeto se algo mudou, pra não redesenhar o painel à toa.
      const serialized = JSON.stringify(next);
      if (cwd.value === dir && serialized !== last) {
        last = serialized;
        status.value = next;
      }
    } finally {
      loading.value = false;
      if (rerun) {
        rerun = false;
        refresh();
      }
    }
  }

  watch(cwd, refresh, { immediate: true });
  const timer = setInterval(() => !document.hidden && refresh(), POLL_MS);
  onBeforeUnmount(() => clearInterval(timer));

  return { status, loading, refresh };
}
