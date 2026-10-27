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

export interface FileList {
  paths: string[];
  /** Repositório enorme: a lista foi cortada. */
  truncated: boolean;
}

/** Arquivos do repositório (rastreados + novos), respeitando o .gitignore. */
export const gitLsFiles = (root: string) => invoke<FileList>("git_ls_files", { root });

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
