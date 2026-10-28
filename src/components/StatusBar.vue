<script setup lang="ts">
import { computed, onBeforeUnmount, ref } from "vue";
import { getVersion } from "@tauri-apps/api/app";
import { repoName, type GitStatus } from "../lib/git";
import { openUrl, overall, type Check, type CheckState, type GithubStatus, type RepoInfo } from "../lib/github";
import { checkUpdate, installUpdate, restartApp, type UpdateInfo } from "../lib/update";
import { prettyPath } from "../state/tabs";

const props = defineProps<{
  cwd: string | null;
  home: string | null;
  git: GitStatus | null;
  repo: RepoInfo | null;
  github: GithubStatus | null;
  diffOpen: boolean;
  panelOpen: boolean;
  pickerOpen: boolean;
  claudeHooksMissing: boolean;
  claudeHooksPath: string | null;
  /** Mensagem temporária (ex.: resultado de ativar os avisos). */
  note: string | null;
  /** MCPs do Claude da aba ativa (só quando o Claude está rodando nela). */
  mcp: { servers: { status: string }[] | null; loading: boolean } | null;
  mcpAvailable: boolean;
  mcpOpen: boolean;
}>();
defineEmits<{ toggleDiff: []; togglePanel: []; pickBranch: []; githubLogin: []; installClaudeHooks: []; toggleMcp: [] }>();

const mcpCounts = computed(() => {
  const servers = props.mcp?.servers ?? [];
  return {
    total: servers.length,
    ok: servers.filter((s) => s.status === "connected").length,
    auth: servers.filter((s) => s.status === "needs_auth").length,
    failed: servers.filter((s) => s.status === "failed").length,
  };
});

const open = (url: string) => openUrl(url).catch(() => {});

// Versão embutida no binário: a release grava a dela; build local fica "0.0.0-dev".
const RELEASES = "https://github.com/code7tecbr/term7/releases";
const version = ref<string | null>(null);
getVersion()
  .then((v) => {
    version.value = v;
    refreshUpdate();
  })
  .catch(() => {});
const isDev = computed(() => !!version.value?.includes("dev"));

function openRelease() {
  if (version.value) open(isDev.value ? RELEASES : `${RELEASES}/tag/v${version.value}`);
}

// ---- atualização ----
// Confere ao abrir e a cada 6 h. Build local ("dev") não confere: não tem pra quê.
const update = ref<UpdateInfo | null>(null);
const updating = ref<"idle" | "installing" | "installed">("idle");
const updateError = ref<string | null>(null);

async function refreshUpdate() {
  if (!version.value || isDev.value || updating.value !== "idle") return;
  update.value = await checkUpdate().catch(() => null);
}
const updateTimer = setInterval(refreshUpdate, 6 * 60 * 60 * 1000);
onBeforeUnmount(() => clearInterval(updateTimer));

async function runUpdate() {
  const u = update.value;
  if (!u || updating.value === "installing") return;
  if (updating.value === "installed") return restartApp();
  updating.value = "installing";
  updateError.value = null;
  try {
    await installUpdate(u.latest);
    updating.value = "installed";
  } catch (e) {
    updating.value = "idle";
    updateError.value = String(e);
  }
}

const updateLabel = computed(() => {
  if (updating.value === "installing") return "Atualizando…";
  if (updating.value === "installed") return `Reiniciar na v${update.value?.latest}`;
  return `Atualizar pra v${update.value?.latest}`;
});
const updateTitle = computed(() => {
  if (updateError.value) return `Não deu certo: ${updateError.value}. Clique pra tentar de novo.`;
  if (updating.value === "installed") return "Instalada. Reinicia o term7 (as abas voltam, mas os programas abertos nelas são encerrados).";
  return "Baixa o .deb da release e instala (o sistema pede a senha).";
});

// Branch no link: cada parte codificada, mas as barras ficam (ex.: feat/x).
const branchPath = (b: string) => b.split("/").map(encodeURIComponent).join("/");

const branchUrl = computed(() => {
  const web = props.repo?.webUrl;
  const branch = props.git?.branch;
  return web && branch ? `${web}/tree/${branchPath(branch)}` : web ?? null;
});

/** Branch publicada, sem PR e que não é a padrão: dá pra abrir um PR. */
const createPrUrl = computed(() => {
  const web = props.repo?.webUrl;
  const branch = props.git?.branch;
  if (!web || !branch || !props.git?.upstream) return null;
  if (props.github?.kind === "pr" || props.github?.kind === "missing" || props.github?.kind === "unauthenticated") return null;
  if (branch === (props.repo?.defaultBranch ?? "main")) return null;
  return `${web}/compare/${branchPath(branch)}?expand=1`;
});

const pr = computed(() => (props.github?.kind === "pr" ? props.github.pr : null));
const runs = computed(() => (props.github?.kind === "runs" ? props.github.runs : []));

/** Checks do PR ou runs da branch, no mesmo formato. */
const checks = computed<Check[]>(() => pr.value?.checks ?? runs.value.map((r) => ({ name: r.name, state: r.state, url: r.url })));
const checksState = computed(() => overall(checks.value.map((c) => c.state)));

const STATE_LABEL: Record<CheckState, string> = {
  pending: "rodando",
  success: "passou",
  failure: "falhou",
  neutral: "ignorado",
};
const STATE_ICON: Record<CheckState, string> = { pending: "●", success: "✓", failure: "✗", neutral: "–" };

const checksTitle = computed(() =>
  checks.value.map((c) => `${STATE_ICON[c.state]} ${c.name} — ${STATE_LABEL[c.state]}`).join("\n"),
);

/** Com falha, vai direto pro log do que falhou; senão, pra lista de checks. */
function openChecks() {
  const failed = checks.value.find((c) => c.state === "failure" && c.url);
  if (failed?.url) return open(failed.url);
  if (pr.value) return open(`${pr.value.url}/checks`);
  const first = checks.value.find((c) => c.url);
  if (first?.url) open(first.url);
}

const prState = computed(() => {
  const p = pr.value;
  if (!p) return null;
  if (p.state === "MERGED") return { cls: "merged", label: "mergeado" };
  if (p.state === "CLOSED") return { cls: "closed", label: "fechado" };
  if (p.isDraft) return { cls: "draft", label: "rascunho" };
  // Ordem de prioridade: o que pede ação primeiro.
  if (p.reviewDecision === "CHANGES_REQUESTED") return { cls: "changes", label: "alterações pedidas", mark: "±" };
  if (p.reviewDecision === "APPROVED") return { cls: "approved", label: "aprovado", mark: "✓" };
  if (p.reviewRequested) return { cls: "review", label: "aguardando review", mark: "●" };
  return { cls: "opened", label: "aberto" };
});

const totals = computed(() => {
  const files = props.git?.files ?? [];
  return {
    count: files.length,
    additions: files.reduce((n, f) => n + f.additions, 0),
    deletions: files.reduce((n, f) => n + f.deletions, 0),
  };
});
</script>

<template>
  <footer class="statusbar">
    <span class="cwd" :title="cwd ?? ''">
      <svg viewBox="0 0 16 16" width="13" height="13"><path d="M2 4.5A1.5 1.5 0 0 1 3.5 3h3l1.5 1.5h4.5A1.5 1.5 0 0 1 14 6v5.5a1.5 1.5 0 0 1-1.5 1.5h-9A1.5 1.5 0 0 1 2 11.5z" fill="none" stroke="currentColor" stroke-width="1.2" /></svg>
      {{ cwd ? prettyPath(cwd, home) : "" }}
    </span>

    <div class="right" @mousedown.prevent>
      <span v-if="note" class="note">{{ note }}</span>
      <button
        v-if="mcpAvailable"
        class="btn mcp"
        data-mcp-toggle
        :class="{ open: mcpOpen }"
        title="Servidores MCP do Claude desta aba"
        @click="$emit('toggleMcp')"
      >
        <span class="mcp-label">MCP</span>
        <template v-if="mcp?.servers">
          <span class="mcp-ok">{{ mcpCounts.ok }}/{{ mcpCounts.total }}</span>
          <span v-if="mcpCounts.auth" class="mcp-auth" title="Precisam de autenticação">⚠ {{ mcpCounts.auth }}</span>
          <span v-if="mcpCounts.failed" class="mcp-failed" title="Falharam">✕ {{ mcpCounts.failed }}</span>
        </template>
        <span v-else-if="mcp?.loading" class="clean">…</span>
      </button>
      <button
        v-if="!note && claudeHooksMissing"
        class="btn claude"
        :title="`Instala hooks no ${claudeHooksPath ?? 'settings.json do Claude'} pra aba avisar quando o Claude pedir algo ou terminar. Os outros hooks e configurações ficam como estão.`"
        @click="$emit('installClaudeHooks')"
      >
        Ativar avisos do Claude
      </button>

      <button class="btn" :class="{ open: panelOpen }" title="Terminal inferior (Ctrl+')" @click="$emit('togglePanel')">
        <svg viewBox="0 0 16 16" width="13" height="13"><rect x="2" y="2.5" width="12" height="11" rx="1.5" fill="none" stroke="currentColor" stroke-width="1.2" /><path d="M2 9.5h12" stroke="currentColor" stroke-width="1.2" /></svg>
        Terminal
      </button>

      <template v-if="git">
        <button
          class="btn"
          :class="{ open: pickerOpen, dirty: totals.count > 0 }"
          :title="`${git.root}\nTrocar de branch`"
          @click="$emit('pickBranch')"
        >
          <svg viewBox="0 0 16 16" width="13" height="13"><circle cx="5" cy="3.5" r="1.5" fill="none" stroke="currentColor" stroke-width="1.2" /><circle cx="5" cy="12.5" r="1.5" fill="none" stroke="currentColor" stroke-width="1.2" /><circle cx="11" cy="6" r="1.5" fill="none" stroke="currentColor" stroke-width="1.2" /><path d="M5 5v6M11 7.5c0 2-2 2.5-5 3.5" fill="none" stroke="currentColor" stroke-width="1.2" /></svg>
          <span class="repo">{{ repoName(repo?.mainRoot ?? git.root) }}</span>
          <span v-if="repo?.isWorktree" class="tag" :title="`Worktree de ${repo.mainRoot ?? ''}`">worktree</span>
          <span class="slash">/</span>
          <span class="branch">{{ git.branch ?? "HEAD" }}</span>
          <span v-if="git.ahead" class="sync">↑{{ git.ahead }}</span>
          <span v-if="git.behind" class="sync">↓{{ git.behind }}</span>
          <svg viewBox="0 0 16 16" width="10" height="10"><path d="M4 6l4 4 4-4" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round" /></svg>
        </button>

        <template v-if="pr && prState">
          <button class="btn pr" :class="prState.cls" :title="`#${pr.number} ${pr.title}\n${prState.label} — abrir no navegador`" @click="open(pr.url)">
            <svg viewBox="0 0 16 16" width="13" height="13"><circle cx="4.5" cy="3.5" r="1.5" fill="none" stroke="currentColor" stroke-width="1.2" /><circle cx="4.5" cy="12.5" r="1.5" fill="none" stroke="currentColor" stroke-width="1.2" /><circle cx="11.5" cy="12.5" r="1.5" fill="none" stroke="currentColor" stroke-width="1.2" /><path d="M4.5 5v6M11.5 11V6.5A1.5 1.5 0 0 0 10 5H7.5m0 0L9 3.5M7.5 5 9 6.5" fill="none" stroke="currentColor" stroke-width="1.2" stroke-linecap="round" stroke-linejoin="round" /></svg>
            #{{ pr.number }}
            <span v-if="prState.mark" class="mark">{{ prState.mark }}</span>
          </button>
        </template>

        <button v-if="checksState" class="btn checks" :class="checksState" :title="`${checksTitle}\n\nAbrir no navegador`" @click="openChecks">
          <span class="dot" />
          <span>{{ pr ? "checks" : checks.length === 1 ? checks[0].name : "actions" }}</span>
        </button>

        <button v-if="createPrUrl" class="btn" title="Abrir um pull request desta branch no navegador" @click="open(createPrUrl)">Criar PR</button>

        <button v-if="github?.kind === 'unauthenticated'" class="btn login" title="O gh não está logado: abre o terminal inferior com gh auth login" @click="$emit('githubLogin')">
          Entrar no GitHub
        </button>

        <button v-if="branchUrl && !pr" class="btn icon" :title="`Abrir ${git.branch ?? 'repositório'} no navegador`" @click="open(branchUrl)">
          <svg viewBox="0 0 16 16" width="13" height="13"><path d="M9 3h4v4M13 3 7.5 8.5M12 9.5V12a1 1 0 0 1-1 1H4a1 1 0 0 1-1-1V5a1 1 0 0 1 1-1h2.5" fill="none" stroke="currentColor" stroke-width="1.2" stroke-linecap="round" stroke-linejoin="round" /></svg>
        </button>

        <button class="btn" :class="{ open: diffOpen }" title="Explorador de arquivos (Ctrl+Shift+G)" @click="$emit('toggleDiff')">
          <template v-if="totals.count">
            <span>{{ totals.count }} {{ totals.count === 1 ? "arquivo" : "arquivos" }}</span>
            <span class="add">+{{ totals.additions }}</span>
            <span class="del">−{{ totals.deletions }}</span>
          </template>
          <span v-else class="clean">sem alterações</span>
        </button>
      </template>

      <button
        v-if="update?.newer && update.installed"
        class="btn update"
        :class="{ error: updateError, ready: updating === 'installed' }"
        :title="updateTitle"
        :disabled="updating === 'installing'"
        @click="runUpdate"
      >
        <svg viewBox="0 0 16 16" width="12" height="12"><path d="M8 13V3M4 7l4-4 4 4" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round" /></svg>
        {{ updateLabel }}
      </button>
      <button
        v-if="version"
        class="btn version"
        :title="isDev ? 'term7 — build local (fora de uma release)' : update?.newer && !update.installed ? `term7 v${version} — a v${update.latest} já saiu (este build não é o do apt)` : `term7 v${version} — ver a release`"
        @click="openRelease"
      >
        {{ isDev ? "dev" : `v${version}` }}
      </button>
    </div>
  </footer>
</template>

<style scoped>
.statusbar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  height: 26px;
  padding: 0 10px;
  background: var(--bg-elevated);
  border-top: 1px solid var(--border);
  color: var(--fg-muted);
  font-size: 12px;
  user-select: none;
}
.cwd {
  display: flex;
  align-items: center;
  gap: 6px;
  min-width: 0;
  overflow: hidden;
  white-space: nowrap;
  text-overflow: ellipsis;
}
.right {
  display: flex;
  align-items: center;
  gap: 4px;
  flex-shrink: 0;
}
.btn {
  display: flex;
  align-items: center;
  gap: 6px;
  height: 20px;
  padding: 0 8px;
  border: 0;
  border-radius: 5px;
  background: none;
  color: inherit;
  font: inherit;
  cursor: pointer;
}
.btn:hover,
.btn.open {
  background: var(--hover);
  color: var(--fg);
}
.repo {
  color: var(--fg);
  font-weight: 500;
}
.slash {
  opacity: 0.5;
}
.btn.dirty .branch {
  color: var(--yellow);
}
.add {
  color: var(--green);
}
.del {
  color: var(--red);
}
.claude {
  color: var(--claude);
}
.mcp-label {
  font-weight: 600;
  letter-spacing: 0.04em;
}
.mcp-ok {
  color: var(--green);
}
.mcp-auth {
  color: var(--yellow);
}
.mcp-failed {
  color: var(--red);
}
.note {
  max-width: 420px;
  overflow: hidden;
  color: var(--fg);
  white-space: nowrap;
  text-overflow: ellipsis;
}
.update {
  color: var(--accent);
  font-weight: 600;
}
.update.ready {
  color: var(--green);
}
.update.error {
  color: var(--red);
}
.update:disabled {
  opacity: 0.7;
  cursor: progress;
}
.version {
  font-family: var(--font-mono);
  font-size: 11px;
  opacity: 0.7;
}
.clean,
.sync {
  opacity: 0.8;
}
.tag {
  padding: 0 5px;
  border: 1px solid var(--border);
  border-radius: 4px;
  font-size: 10.5px;
  line-height: 14px;
  color: var(--accent);
}
.btn.icon {
  padding: 0 5px;
}
/* Cor = estado do PR; o fundo leve ajuda a ver de longe. */
.pr.opened {
  color: var(--accent);
}
.pr.review {
  background: color-mix(in srgb, var(--yellow) 14%, transparent);
  color: var(--yellow);
}
.pr.approved {
  background: color-mix(in srgb, var(--green) 14%, transparent);
  color: var(--green);
}
.pr.changes {
  background: color-mix(in srgb, #ff9e64 16%, transparent);
  color: #ff9e64;
}
.pr .mark {
  font-size: 10px;
}
.pr.merged {
  color: var(--purple);
}
.pr.closed {
  color: var(--red);
}
.pr.draft {
  color: var(--fg-muted);
}
.login {
  color: var(--accent);
}
.dot {
  width: 7px;
  height: 7px;
  border-radius: 50%;
  background: currentColor;
}
.checks.success .dot {
  color: var(--green);
}
.checks.failure .dot {
  color: var(--red);
}
.checks.neutral .dot {
  color: var(--fg-muted);
}
.checks.pending .dot {
  color: var(--yellow);
  animation: pulse 1.2s ease-in-out infinite;
}
.checks.failure {
  color: var(--red);
}
@keyframes pulse {
  50% {
    opacity: 0.3;
  }
}
</style>
