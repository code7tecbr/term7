//! Leitura do estado do git usando o binário `git` do sistema
//! (respeita a config do usuário e evita depender de libgit2).

use std::{collections::HashMap, path::Path, process::Command};

use serde::Serialize;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileChange {
    path: String,
    orig_path: Option<String>,
    /// added | modified | deleted | renamed | untracked | conflict
    kind: &'static str,
    staged: bool,
    unstaged: bool,
    additions: u32,
    deletions: u32,
    binary: bool,
}

#[derive(Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct GitStatus {
    root: String,
    branch: Option<String>,
    upstream: Option<String>,
    ahead: u32,
    behind: u32,
    files: Vec<FileChange>,
}

fn git(dir: &str, args: &[&str]) -> Option<std::process::Output> {
    Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        // Não segura o index.lock: o polling não pode atrapalhar os comandos do usuário.
        .env("GIT_OPTIONAL_LOCKS", "0")
        .output()
        .ok()
}

fn git_ok(dir: &str, args: &[&str]) -> Option<String> {
    let out = git(dir, args)?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

fn parse_branch(line: &str, status: &mut GitStatus) {
    // Formatos: "main...origin/main [ahead 1, behind 2]", "main", "No commits yet on main", "HEAD (no branch)"
    let (head, tracking) = match line.find(" [") {
        Some(i) => (&line[..i], line[i + 2..].trim_end_matches(']')),
        None => (line, ""),
    };
    let head = head.strip_prefix("No commits yet on ").unwrap_or(head);
    let (branch, upstream) = match head.split_once("...") {
        Some((b, u)) => (b, Some(u.to_string())),
        None => (head, None),
    };
    status.branch = Some(branch.to_string());
    status.upstream = upstream;
    for part in tracking.split(", ") {
        if let Some(n) = part.strip_prefix("ahead ") {
            status.ahead = n.parse().unwrap_or(0);
        } else if let Some(n) = part.strip_prefix("behind ") {
            status.behind = n.parse().unwrap_or(0);
        }
    }
}

/// Linhas adicionadas/removidas por arquivo (`git diff --numstat`).
fn numstat(root: &str, staged: bool) -> HashMap<String, (u32, u32, bool)> {
    let mut args = vec!["diff", "--numstat", "-z", "-M", "--no-ext-diff"];
    if staged {
        args.push("--cached");
    }
    let mut map = HashMap::new();
    let Some(out) = git_ok(root, &args) else { return map };

    let mut it = out.split('\0');
    while let Some(record) = it.next() {
        if record.is_empty() {
            continue;
        }
        let mut parts = record.splitn(3, '\t');
        let (add, del, path) = (parts.next(), parts.next(), parts.next().unwrap_or(""));
        // Em renomeações, o caminho vem vazio e os dois nomes chegam nos próximos campos.
        let path = if path.is_empty() {
            it.next();
            it.next().unwrap_or("").to_string()
        } else {
            path.to_string()
        };
        let binary = add == Some("-");
        let add = add.and_then(|s| s.parse().ok()).unwrap_or(0);
        let del = del.and_then(|s| s.parse().ok()).unwrap_or(0);
        map.insert(path, (add, del, binary));
    }
    map
}

/// Arquivo novo maior que isso não tem as linhas contadas (conta como binário).
const MAX_COUNT_BYTES: usize = 1024 * 1024;

/// Conta as linhas de um arquivo novo (não rastreado). Binário ou muito grande: (0, true).
fn count_lines(path: &Path) -> (u32, bool) {
    match std::fs::read(path) {
        Ok(bytes) if bytes.len() <= MAX_COUNT_BYTES => {
            if bytes.iter().take(8000).any(|&b| b == 0) {
                (0, true)
            } else {
                let lines = bytecount_newlines(&bytes);
                let trailing = !bytes.is_empty() && !bytes.ends_with(b"\n");
                (lines + trailing as u32, false)
            }
        }
        _ => (0, true),
    }
}

fn bytecount_newlines(bytes: &[u8]) -> u32 {
    bytes.iter().filter(|&&b| b == b'\n').count() as u32
}

fn kind(x: char, y: char) -> &'static str {
    match (x, y) {
        ('?', _) => "untracked",
        ('U', _) | (_, 'U') | ('A', 'A') | ('D', 'D') => "conflict",
        ('R', _) | ('C', _) => "renamed",
        ('A', _) => "added",
        ('D', _) | (_, 'D') => "deleted",
        _ => "modified",
    }
}

fn read_status(cwd: &str) -> Option<GitStatus> {
    let root = git_ok(cwd, &["rev-parse", "--show-toplevel"])?.trim().to_string();
    let out = git_ok(
        &root,
        &["status", "--porcelain=v1", "-z", "--branch", "--untracked-files=all"],
    )?;

    let mut status = GitStatus { root: root.clone(), ..Default::default() };
    let unstaged_stats = numstat(&root, false);
    let staged_stats = numstat(&root, true);

    let mut it = out.split('\0');
    while let Some(entry) = it.next() {
        if let Some(branch) = entry.strip_prefix("## ") {
            parse_branch(branch, &mut status);
            continue;
        }
        if entry.len() < 4 {
            continue;
        }
        let mut chars = entry.chars();
        let (x, y) = (chars.next().unwrap_or(' '), chars.next().unwrap_or(' '));
        let path = entry[3..].to_string();
        let orig_path = matches!(x, 'R' | 'C').then(|| it.next().unwrap_or("").to_string());

        let (mut additions, mut deletions, mut binary) = (0, 0, false);
        if x == '?' {
            (additions, binary) = count_lines(&Path::new(&root).join(&path));
        } else {
            for stats in [&unstaged_stats, &staged_stats] {
                if let Some(&(a, d, b)) = stats.get(&path) {
                    additions += a;
                    deletions += d;
                    binary |= b;
                }
            }
        }

        status.files.push(FileChange {
            kind: kind(x, y),
            staged: !matches!(x, ' ' | '?'),
            unstaged: y != ' ',
            path,
            orig_path,
            additions,
            deletions,
            binary,
        });
    }
    Some(status)
}

// Comandos async + spawn_blocking: o git pode demorar e não pode travar a thread da janela.

#[tauri::command]
pub async fn git_status(cwd: String) -> Result<Option<GitStatus>, String> {
    tauri::async_runtime::spawn_blocking(move || read_status(&cwd))
        .await
        .map_err(|e| e.to_string())
}

/// Limite da lista do explorador: repositório gigante não pode travar a interface.
const MAX_FILES: usize = 50_000;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileList {
    paths: Vec<String>,
    truncated: bool,
}

/// Arquivos do repositório (rastreados + novos), sem os ignorados pelo .gitignore.
fn ls_files(root: &str) -> FileList {
    let out = git_ok(root, &["ls-files", "-z", "--cached", "--others", "--exclude-standard"])
        .unwrap_or_default();
    let mut paths: Vec<String> = out.split('\0').filter(|p| !p.is_empty()).map(String::from).collect();
    // Arquivo com conflito aparece uma vez por estágio.
    paths.sort_unstable();
    paths.dedup();
    let truncated = paths.len() > MAX_FILES;
    paths.truncate(MAX_FILES);
    FileList { paths, truncated }
}

#[tauri::command]
pub async fn git_ls_files(root: String) -> Result<FileList, String> {
    tauri::async_runtime::spawn_blocking(move || ls_files(&root))
        .await
        .map_err(|e| e.to_string())
}

// ---- conteúdo no último commit (pro visualizador comparar) ----

fn show_head(root: &str, path: &str) -> Option<crate::files::FileContent> {
    let out = git(root, &["show", &format!("HEAD:{path}")])?;
    out.status.success().then(|| crate::files::to_content(out.stdout))
}

/// Conteúdo do arquivo no HEAD. None se o arquivo é novo ou não há commits.
#[tauri::command]
pub async fn git_show_head(root: String, path: String) -> Result<Option<crate::files::FileContent>, String> {
    tauri::async_runtime::spawn_blocking(move || show_head(&root, &path))
        .await
        .map_err(|e| e.to_string())
}

// ---- branches ----

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Branch {
    name: String,
    remote: bool,
    current: bool,
    /// Data do último commit (unix, segundos).
    updated_at: i64,
}

fn list_branches(root: &str) -> Vec<Branch> {
    let Some(out) = git_ok(
        root,
        &[
            "for-each-ref",
            "--sort=-committerdate",
            "--format=%(refname)%00%(refname:short)%00%(HEAD)%00%(committerdate:unix)",
            "refs/heads",
            "refs/remotes",
        ],
    ) else {
        return vec![];
    };

    let mut branches: Vec<Branch> = out
        .lines()
        .filter_map(|line| {
            let mut f = line.split('\0');
            let (full, short, head, date) = (f.next()?, f.next()?, f.next()?, f.next()?);
            // "origin/HEAD" é só um apelido do branch padrão do remoto.
            if full.ends_with("/HEAD") {
                return None;
            }
            Some(Branch {
                name: short.to_string(),
                remote: full.starts_with("refs/remotes/"),
                current: head == "*",
                updated_at: date.parse().unwrap_or(0),
            })
        })
        .collect();

    // Remoto que já tem branch local com o mesmo nome só polui a lista.
    let local: std::collections::HashSet<String> =
        branches.iter().filter(|b| !b.remote).map(|b| b.name.clone()).collect();
    branches.retain(|b| {
        !b.remote || !b.name.split_once('/').is_some_and(|(_, name)| local.contains(name))
    });
    branches
}

#[tauri::command]
pub async fn git_branches(root: String) -> Result<Vec<Branch>, String> {
    tauri::async_runtime::spawn_blocking(move || list_branches(&root))
        .await
        .map_err(|e| e.to_string())
}

fn checkout(root: &str, branch: &str, remote: bool, create: bool) -> Result<(), String> {
    let args: Vec<&str> = if create {
        vec!["switch", "-c", branch]
    } else if remote {
        // Cria a branch local rastreando a remota (origin/foo -> foo).
        vec!["switch", "--track", branch]
    } else {
        vec!["switch", branch]
    };
    let out = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(&args)
        .output()
        .map_err(|e| e.to_string())?;
    if out.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
}

#[tauri::command]
pub async fn git_checkout(root: String, branch: String, remote: bool, create: bool) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || checkout(&root, &branch, remote, create))
        .await
        .map_err(|e| e.to_string())?
}

// ---- fetch (pra achar branches remotas novas) ----

fn fetch(root: &str) -> Result<(), String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["fetch", "--all", "--prune"])
        // Sem terminal pra digitar senha: falha em vez de travar esperando.
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()
        .map_err(|e| e.to_string())?;
    if out.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
}

#[tauri::command]
pub async fn git_fetch(root: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || fetch(&root))
        .await
        .map_err(|e| e.to_string())?
}

// ---- info do repositório (worktree e link web) ----

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RepoInfo {
    /// A pasta é uma worktree adicional (`git worktree add`), não o clone principal.
    is_worktree: bool,
    /// Pasta do clone principal, quando é worktree.
    main_root: Option<String>,
    /// Página do repositório no navegador (ex.: https://github.com/dono/repo).
    web_url: Option<String>,
    /// Branch padrão do remote (origin/HEAD), se conhecida.
    default_branch: Option<String>,
}

/// Converte o endereço do remote em link web. Aceita SSH (`git@host:dono/repo.git`,
/// `ssh://git@host/dono/repo`) e HTTPS.
fn web_url(remote: &str) -> Option<String> {
    let remote = remote.trim();
    let rest = if let Some(r) = remote.strip_prefix("https://") {
        r.to_string()
    } else if let Some(r) = remote.strip_prefix("ssh://") {
        r.to_string()
    } else if let Some((user_host, path)) = remote.split_once(':') {
        // git@host:dono/repo — sem "://", então o ":" separa host e caminho.
        if user_host.contains('/') || path.starts_with("//") {
            return None;
        }
        format!("{user_host}/{path}")
    } else {
        return None;
    };
    // Tira usuário/token ("git@", "x-access-token:abc@") e porta.
    let rest = rest.rsplit_once('@').map_or(rest.as_str(), |(_, r)| r);
    let (host, path) = rest.split_once('/')?;
    let host = host.split(':').next()?;
    let path = path.trim_end_matches('/').trim_end_matches(".git");
    (!host.is_empty() && path.contains('/')).then(|| format!("https://{host}/{path}"))
}

fn repo_info(root: &str) -> RepoInfo {
    let dirs = git_ok(
        root,
        &["rev-parse", "--path-format=absolute", "--git-dir", "--git-common-dir"],
    );
    let (git_dir, common_dir) = dirs
        .as_deref()
        .and_then(|s| s.split_once('\n'))
        .map(|(a, b)| (a.trim().to_string(), b.trim().to_string()))
        .unwrap_or_default();
    let is_worktree = !git_dir.is_empty() && git_dir != common_dir;
    let main_root = is_worktree
        .then(|| Path::new(&common_dir).parent().map(|p| p.to_string_lossy().into_owned()))
        .flatten();

    let default_branch = git_ok(root, &["symbolic-ref", "--short", "refs/remotes/origin/HEAD"])
        .map(|s| s.trim().trim_start_matches("origin/").to_string())
        .filter(|s| !s.is_empty());

    RepoInfo {
        is_worktree,
        main_root,
        web_url: git_ok(root, &["remote", "get-url", "origin"]).and_then(|r| web_url(&r)),
        default_branch,
    }
}

#[tauri::command]
pub async fn git_repo_info(root: String) -> Result<RepoInfo, String> {
    tauri::async_runtime::spawn_blocking(move || repo_info(&root))
        .await
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::web_url;

    #[test]
    fn converte_remotes() {
        let gh = Some("https://github.com/code7tecbr/term7".to_string());
        assert_eq!(web_url("git@github.com:code7tecbr/term7.git"), gh);
        assert_eq!(web_url("https://github.com/code7tecbr/term7.git"), gh);
        assert_eq!(web_url("https://x-access-token:abc@github.com/code7tecbr/term7"), gh);
        assert_eq!(web_url("ssh://git@github.com:22/code7tecbr/term7.git\n"), gh);
        assert_eq!(web_url("/caminho/local/repo"), None);
        assert_eq!(web_url("file:///caminho/repo"), None);
    }
}
