import { invoke } from "@tauri-apps/api/core";

export type ChangeKind = "added" | "modified" | "deleted" | "renamed" | "untracked" | "conflict";

export interface FileChange {
  path: string;
  origPath: string | null;
  kind: ChangeKind;
  staged: boolean;
  unstaged: boolean;
  additions: number;
  deletions: number;
  binary: boolean;
}

export interface GitStatus {
  root: string;
  branch: string | null;
  upstream: string | null;
  ahead: number;
  behind: number;
  files: FileChange[];
}

/** null quando a pasta não está dentro de um repositório. */
export const gitStatus = (cwd: string) => invoke<GitStatus | null>("git_status", { cwd });

export const gitDiff = (root: string, file: FileChange) =>
  invoke<string>("git_diff", {
    root,
    path: file.path,
    origPath: file.origPath,
    untracked: file.kind === "untracked",
  });

export interface DiffLine {
  kind: "hunk" | "add" | "del" | "ctx" | "meta";
  text: string;
  oldNo?: number;
  newNo?: number;
}

/** Converte a saída do `git diff` em linhas prontas pra renderizar. */
export function parseDiff(text: string): DiffLine[] {
  const lines: DiffLine[] = [];
  let inHunk = false;
  let oldNo = 0;
  let newNo = 0;

  for (const line of text.replace(/\n$/, "").split("\n")) {
    if (line.startsWith("diff --git")) {
      inHunk = false;
      continue;
    }
    const hunk = /^@@ -(\d+)(?:,\d+)? \+(\d+)(?:,\d+)? @@(.*)$/.exec(line);
    if (hunk) {
      oldNo = Number(hunk[1]);
      newNo = Number(hunk[2]);
      inHunk = true;
      lines.push({ kind: "hunk", text: line });
      continue;
    }
    if (!inHunk) {
      if (line.startsWith("Binary files")) lines.push({ kind: "meta", text: "Arquivo binário" });
      continue;
    }
    const sign = line[0];
    const body = line.slice(1);
    if (sign === "+") lines.push({ kind: "add", text: body, newNo: newNo++ });
    else if (sign === "-") lines.push({ kind: "del", text: body, oldNo: oldNo++ });
    else if (sign === "\\") lines.push({ kind: "meta", text: line.slice(2) });
    else lines.push({ kind: "ctx", text: body, oldNo: oldNo++, newNo: newNo++ });
  }
  return lines;
}

export interface FileContent {
  text: string;
  binary: boolean;
}

export const readTextFile = (path: string) => invoke<FileContent>("read_text_file", { path });

/** Conteúdo no último commit. null se o arquivo é novo. */
export const gitShowHead = (root: string, path: string) =>
  invoke<FileContent | null>("git_show_head", { root, path });

export interface Branch {
  name: string;
  remote: boolean;
  current: boolean;
  updatedAt: number;
}

export const gitBranches = (root: string) => invoke<Branch[]>("git_branches", { root });

export const gitCheckout = (root: string, branch: string, opts: { remote?: boolean; create?: boolean } = {}) =>
  invoke<void>("git_checkout", { root, branch, remote: !!opts.remote, create: !!opts.create });

export const gitFetch = (root: string) => invoke<void>("git_fetch", { root });

export const repoName = (root: string) => root.split("/").filter(Boolean).pop() ?? root;
