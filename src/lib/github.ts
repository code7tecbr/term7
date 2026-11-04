import { invoke } from "@tauri-apps/api/core";

export interface RepoInfo {
  isWorktree: boolean;
  mainRoot: string | null;
  webUrl: string | null;
  defaultBranch: string | null;
}

export const gitRepoInfo = (root: string) => invoke<RepoInfo>("git_repo_info", { root });

type GhResult =
  | { kind: "missing" }
  | { kind: "unauthenticated" }
  | { kind: "none" }
  | { kind: "ok"; json: string }
  | { kind: "error"; message: string };

export const openUrl = (url: string) => invoke<void>("open_url", { url });

export type CheckState = "pending" | "success" | "failure" | "neutral";

export interface Check {
  name: string;
  state: CheckState;
  url: string | null;
}

export interface PullRequest {
  number: number;
  title: string;
  /** OPEN | MERGED | CLOSED */
  state: string;
  isDraft: boolean;
  url: string;
  checks: Check[];
  /** APPROVED | CHANGES_REQUESTED | REVIEW_REQUIRED | null (sem regra de review). */
  reviewDecision: string | null;
  /** Há revisor (pessoa ou time) com review pendente. */
  reviewRequested: boolean;
}

/** Uma execução de workflow, pra branch sem PR. */
export interface Run {
  name: string;
  state: CheckState;
  url: string;
  headSha: string;
}

export type GithubStatus =
  | { kind: "missing" }
  | { kind: "unauthenticated" }
  | { kind: "pr"; pr: PullRequest }
  | { kind: "runs"; runs: Run[] };

// O rollup mistura dois tipos da API: CheckRun (actions) e StatusContext (status antigos).
interface RawCheck {
  __typename?: string;
  name?: string;
  workflowName?: string;
  context?: string;
  status?: string;
  conclusion?: string;
  state?: string;
  detailsUrl?: string;
  targetUrl?: string;
}

const PENDING = ["QUEUED", "IN_PROGRESS", "WAITING", "PENDING", "REQUESTED", "EXPECTED"];
const OK = ["SUCCESS"];
const NEUTRAL = ["NEUTRAL", "SKIPPED", "CANCELLED", "STALE"];

function checkState(status: string | undefined, result: string | undefined): CheckState {
  const s = (status ?? "").toUpperCase();
  const r = (result ?? "").toUpperCase();
  if (PENDING.includes(s) || (!r && s !== "COMPLETED")) return "pending";
  if (OK.includes(r)) return "success";
  if (NEUTRAL.includes(r)) return "neutral";
  return "failure";
}

function parseCheck(raw: RawCheck): Check {
  if (raw.__typename === "StatusContext" || raw.context) {
    // StatusContext só tem "state"; PENDING/EXPECTED contam como rodando.
    const state = (raw.state ?? "").toUpperCase();
    return {
      name: raw.context ?? "status",
      state: PENDING.includes(state) ? "pending" : checkState("COMPLETED", state === "ERROR" ? "FAILURE" : state),
      url: raw.targetUrl || null,
    };
  }
  return {
    name: raw.workflowName && raw.name ? `${raw.workflowName} / ${raw.name}` : (raw.name ?? "check"),
    state: checkState(raw.status, raw.conclusion),
    url: raw.detailsUrl || null,
  };
}

/** Junta vários estados num só: falha > rodando > sucesso. */
export function overall(states: CheckState[]): CheckState | null {
  if (!states.length) return null;
  if (states.includes("failure")) return "failure";
  if (states.includes("pending")) return "pending";
  if (states.includes("success")) return "success";
  return "neutral";
}

function fromGh(res: Exclude<GhResult, { kind: "ok" }>): GithubStatus | null {
  if (res.kind === "error") throw new Error(res.message);
  if (res.kind === "none") return null;
  return { kind: res.kind };
}

/**
 * Resultado do review. O GitHub só preenche o reviewDecision quando a branch
 * exige aprovação; sem essa regra, vem do último review de cada pessoa.
 */
export function reviewDecision(raw: { reviewDecision?: string | null; latestReviews?: { state?: string }[] | null }) {
  if (raw.reviewDecision) return raw.reviewDecision;
  const states = (raw.latestReviews ?? []).map((r) => r.state);
  if (states.includes("CHANGES_REQUESTED")) return "CHANGES_REQUESTED";
  if (states.includes("APPROVED")) return "APPROVED";
  return null;
}

export async function fetchPullRequest(root: string): Promise<GithubStatus | null> {
  const res = await invoke<GhResult>("gh_pr", { root });
  if (res.kind !== "ok") return fromGh(res);
  const raw = JSON.parse(res.json);
  return {
    kind: "pr",
    pr: {
      number: raw.number,
      title: raw.title,
      state: raw.state,
      isDraft: !!raw.isDraft,
      url: raw.url,
      checks: ((raw.statusCheckRollup ?? []) as RawCheck[]).map(parseCheck),
      reviewDecision: reviewDecision(raw),
      reviewRequested: Array.isArray(raw.reviewRequests) && raw.reviewRequests.length > 0,
    },
  };
}

/** Runs do último commit da branch que tem actions. */
/**
 * Eventos que dizem respeito ao código da branch. Workflows de comentário e
 * review (bots como Claude e CodeRabbit) rodam "em cima" da branch principal,
 * no último commit dela, e encheriam a barra com coisa que não é deploy/build.
 */
const BRANCH_EVENTS = ["push", "workflow_dispatch", "schedule", "release", "workflow_run", "merge_group", "repository_dispatch"];

export async function fetchRuns(root: string, branch: string): Promise<GithubStatus | null> {
  const res = await invoke<GhResult>("gh_runs", { root });
  if (res.kind !== "ok") return fromGh(res);
  type RawRun = RawCheck & { url: string; headSha: string; headBranch: string; event?: string };
  const raw = (JSON.parse(res.json) as RawRun[]).filter(
    (r) => r.headBranch === branch && (!r.event || BRANCH_EVENTS.includes(r.event)),
  );
  if (!raw.length) return null;
  // A lista vem do mais novo pro mais antigo; fica só com os runs do commit mais recente.
  const sha = raw[0].headSha;
  const runs = raw
    .filter((r) => r.headSha === sha)
    .map((r) => ({
      name: r.name || "workflow",
      state: checkState(r.status, r.conclusion),
      url: r.url,
      headSha: r.headSha,
    }));
  return { kind: "runs", runs };
}
