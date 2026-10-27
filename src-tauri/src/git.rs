//! Leitura do estado do git usando o binário `git` do sistema
//! (respeita a config do usuário e evita depender de libgit2).

use std::{collections::HashMap, path::Path, process::Command};

use serde::Serialize;

/// Diffs maiores que isso são cortados, pra não travar a interface.
const MAX_DIFF_BYTES: usize = 1024 * 1024;

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

/// Conta as linhas de um arquivo novo (não rastreado). Binário ou muito grande: (0, true).
fn count_lines(path: &Path) -> (u32, bool) {
    match std::fs::read(path) {
        Ok(bytes) if bytes.len() <= MAX_DIFF_BYTES => {
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

fn read_diff(root: &str, path: &str, orig_path: Option<&str>, untracked: bool) -> String {
    let stdout = |args: &[&str]| {
        git(root, args)
            .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
            .unwrap_or_default()
    };
    let base = ["diff", "--no-color", "--no-ext-diff", "-M"];

    let mut diff = if untracked {
        // Sai com código 1 quando há diferença, então usa o stdout mesmo assim.
        stdout(&[&base[..], &["--no-index", "--", "/dev/null", path]].concat())
    } else {
        let mut paths = vec!["--"];
        paths.extend(orig_path);
        paths.push(path);
        let has_head = git(root, &["rev-parse", "--verify", "-q", "HEAD"])
            .is_some_and(|o| o.status.success());
        if has_head {
            stdout(&[&base[..], &["HEAD"], &paths].concat())
        } else {
            // Repositório sem nenhum commit: mostra o que está no stage + o que não está.
            stdout(&[&base[..], &["--cached"], &paths].concat()) + &stdout(&[&base[..], &paths].concat())
        }
    };

    if diff.len() > MAX_DIFF_BYTES {
        let mut cut = MAX_DIFF_BYTES;
        while !diff.is_char_boundary(cut) {
            cut -= 1;
        }
        diff.truncate(cut);
        diff.push_str("\n\\ term7: diff cortado (arquivo muito grande)\n");
    }
    diff
}

// Comandos async + spawn_blocking: o git pode demorar e não pode travar a thread da janela.

#[tauri::command]
pub async fn git_status(cwd: String) -> Result<Option<GitStatus>, String> {
    tauri::async_runtime::spawn_blocking(move || read_status(&cwd))
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn git_diff(
    root: String,
    path: String,
    orig_path: Option<String>,
    untracked: bool,
) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || read_diff(&root, &path, orig_path.as_deref(), untracked))
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
