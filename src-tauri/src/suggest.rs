//! Comandos sugeridos pelo Claude CLI, pra virar botão de "rodar" no terminal.
//!
//! A tela do Claude já vem formatada (sem as crases do Markdown), então os
//! comandos saem do histórico da sessão: `$CLAUDE_CONFIG_DIR/sessions/<pid>.json`
//! diz o id da sessão do processo, e `projects/<pasta>/<id>.jsonl` tem as
//! respostas em Markdown. O frontend procura esses textos na tela.

use std::{
    collections::HashMap,
    io::{Read, Seek, SeekFrom},
    path::{Path, PathBuf},
    sync::Mutex,
    time::SystemTime,
};

use serde_json::Value;
use tauri::State;

use crate::pty::PtyState;

/// Quantas respostas do Claude, das mais recentes, entram na busca.
const RECENT_REPLIES: usize = 6;
/// Só o fim do histórico interessa (o arquivo cresce a sessão toda).
const TAIL_BYTES: u64 = 768 * 1024;
const MAX_COMMANDS: usize = 120;

/// Builtins do shell que não estão no PATH mas são comando de verdade.
const BUILTINS: &[&str] = &["cd", "export", "source", "unset", "alias"];

struct Proc {
    cwd: PathBuf,
    env: HashMap<String, String>,
}

fn read_proc(pid: u32) -> Option<Proc> {
    let comm = std::fs::read_to_string(format!("/proc/{pid}/comm")).ok()?;
    if comm.trim() != "claude" {
        return None;
    }
    let cwd = std::fs::read_link(format!("/proc/{pid}/cwd")).ok()?;
    let environ = std::fs::read(format!("/proc/{pid}/environ")).ok()?;
    let env = environ
        .split(|&b| b == 0)
        .filter_map(|var| {
            let (k, v) = std::str::from_utf8(var).ok()?.split_once('=')?;
            Some((k.to_string(), v.to_string()))
        })
        .collect();
    Some(Proc { cwd, env })
}

fn config_dir(env: &HashMap<String, String>) -> Option<PathBuf> {
    env.get("CLAUDE_CONFIG_DIR")
        .map(PathBuf::from)
        .or_else(|| env.get("HOME").map(|h| Path::new(h).join(".claude")))
}

/// Pasta do projeto no formato do Claude: "/home/x/a.b" -> "-home-x-a-b".
fn project_slug(cwd: &str) -> String {
    cwd.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '-' }).collect()
}

/// Histórico da sessão desse processo do Claude.
fn transcript(pid: u32, proc: &Proc) -> Option<PathBuf> {
    let config = config_dir(&proc.env)?;
    let info: Value = serde_json::from_str(&std::fs::read_to_string(config.join(format!("sessions/{pid}.json"))).ok()?).ok()?;
    let id = info.get("sessionId")?.as_str()?;
    // O id vira nome de arquivo: só aceita o formato de um UUID.
    if !id.chars().all(|c| c.is_ascii_hexdigit() || c == '-') {
        return None;
    }
    let cwd = info.get("cwd").and_then(Value::as_str).map(str::to_string).unwrap_or_else(|| proc.cwd.to_string_lossy().into_owned());
    let path = config.join("projects").join(project_slug(&cwd)).join(format!("{id}.jsonl"));
    path.exists().then_some(path)
}

fn read_tail(path: &Path) -> Option<String> {
    let mut file = std::fs::File::open(path).ok()?;
    let len = file.metadata().ok()?.len();
    file.seek(SeekFrom::Start(len.saturating_sub(TAIL_BYTES))).ok()?;
    let mut buf = Vec::new();
    file.read_to_end(&mut buf).ok()?;
    Some(String::from_utf8_lossy(&buf).into_owned())
}

/// Textos das últimas respostas do Claude (a primeira linha pode vir cortada pelo tail).
fn recent_replies(jsonl: &str) -> Vec<String> {
    let mut replies: Vec<String> = jsonl
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .filter(|d| d.get("type").and_then(Value::as_str) == Some("assistant"))
        .filter_map(|d| {
            let content = d.pointer("/message/content")?.as_array()?;
            let text: Vec<&str> = content
                .iter()
                .filter(|c| c.get("type").and_then(Value::as_str) == Some("text"))
                .filter_map(|c| c.get("text")?.as_str())
                .collect();
            (!text.is_empty()).then(|| text.join("\n"))
        })
        .collect();
    let skip = replies.len().saturating_sub(RECENT_REPLIES);
    replies.drain(..skip);
    replies
}

/// Diz se a palavra é um programa: builtin, caminho executável ou algo no PATH.
struct Exec<'a> {
    cwd: &'a Path,
    path: Vec<PathBuf>,
}

impl Exec<'_> {
    fn is_command(&self, word: &str) -> bool {
        if word.is_empty() || BUILTINS.contains(&word) {
            return !word.is_empty();
        }
        if word.contains('/') {
            let p = if let Some(rest) = word.strip_prefix("~/") {
                std::env::var("HOME").map(|h| Path::new(&h).join(rest)).unwrap_or_default()
            } else {
                self.cwd.join(word)
            };
            return is_executable(&p);
        }
        if !word.chars().all(|c| c.is_ascii_alphanumeric() || "-_.+".contains(c)) {
            return false;
        }
        self.path.iter().any(|dir| is_executable(&dir.join(word)))
    }
}

fn is_executable(p: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(p).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
}

/// Primeira palavra, pulando `!` (modo bash do Claude) e `$ ` (prompt).
fn strip_prompt(line: &str) -> &str {
    let line = line.trim();
    line.strip_prefix("$ ").or_else(|| line.strip_prefix("! ")).or_else(|| line.strip_prefix('!')).unwrap_or(line).trim()
}

fn first_word(cmd: &str) -> &str {
    // VAR=x comando: a palavra que importa é a depois das atribuições.
    cmd.split_whitespace().find(|w| !(w.contains('=') && !w.starts_with('='))).unwrap_or("")
}

/// Comandos de blocos ```sh/bash (e sem linguagem, se parecerem comando) e de
/// trechos `entre crases` com pelo menos duas palavras.
fn extract(text: &str, exec: &Exec) -> Vec<String> {
    let mut out = Vec::new();
    let mut lines = text.lines();
    let mut prose = String::new();

    while let Some(line) = lines.next() {
        let Some(fence) = line.trim_start().strip_prefix("```").or_else(|| line.trim_start().strip_prefix("~~~")) else {
            prose.push_str(line);
            prose.push('\n');
            continue;
        };
        let lang = fence.trim().to_lowercase();
        let shell = matches!(lang.as_str(), "sh" | "bash" | "shell" | "zsh" | "console" | "shellsession" | "");
        let console = matches!(lang.as_str(), "console" | "shellsession");
        let mut current = String::new();
        for line in lines.by_ref() {
            let t = line.trim_start();
            if t.starts_with("```") || t.starts_with("~~~") {
                break;
            }
            if !shell {
                continue;
            }
            // Continuação com "\": junta com a linha de baixo.
            if !current.is_empty() {
                current.push('\n');
                current.push_str(line.trim_end());
                if !line.trim_end().ends_with('\\') {
                    out.push(std::mem::take(&mut current));
                }
                continue;
            }
            if t.is_empty() || t.starts_with('#') || (console && !t.starts_with("$ ")) {
                continue;
            }
            let cmd = strip_prompt(t);
            // Bloco sem linguagem pode ser qualquer coisa: só vale se começar com um programa.
            if (lang.is_empty() || console) && !exec.is_command(first_word(cmd)) {
                continue;
            }
            if cmd.ends_with('\\') {
                current = cmd.to_string();
            } else {
                out.push(cmd.to_string());
            }
        }
        if !current.is_empty() {
            out.push(current);
        }
    }

    for (i, part) in prose.split('`').enumerate() {
        if i % 2 == 0 || part.contains('\n') {
            continue;
        }
        let cmd = strip_prompt(part);
        if cmd.split_whitespace().count() >= 2 && exec.is_command(first_word(cmd)) {
            out.push(cmd.to_string());
        }
    }
    out
}

/// Histórico já lido, por arquivo (o polling repete muito sem nada mudar).
static CACHE: Mutex<Option<(PathBuf, SystemTime, u64, Vec<String>)>> = Mutex::new(None);

fn suggestions(pid: u32) -> Vec<String> {
    let Some(proc) = read_proc(pid) else { return Vec::new() };
    let Some(path) = transcript(pid, &proc) else { return Vec::new() };
    let Ok(meta) = std::fs::metadata(&path) else { return Vec::new() };
    let (mtime, len) = (meta.modified().unwrap_or(SystemTime::UNIX_EPOCH), meta.len());
    if let Some((p, t, l, cmds)) = CACHE.lock().unwrap().as_ref() {
        if *p == path && *t == mtime && *l == len {
            return cmds.clone();
        }
    }

    let exec = Exec {
        cwd: &proc.cwd,
        path: proc.env.get("PATH").map(|p| std::env::split_paths(p).collect()).unwrap_or_default(),
    };
    let mut cmds: Vec<String> = Vec::new();
    for reply in read_tail(&path).map(|t| recent_replies(&t)).unwrap_or_default() {
        for cmd in extract(&reply, &exec) {
            if cmd.len() <= 2000 && !cmds.contains(&cmd) {
                cmds.push(cmd);
            }
        }
    }
    // Os mais recentes primeiro, e no máximo MAX_COMMANDS.
    cmds.reverse();
    cmds.truncate(MAX_COMMANDS);
    *CACHE.lock().unwrap() = Some((path, mtime, len, cmds.clone()));
    cmds
}

/// Comandos sugeridos nas últimas respostas do Claude da aba (vazio sem Claude).
#[tauri::command]
pub async fn claude_suggestions(state: State<'_, PtyState>, id: u32) -> Result<Vec<String>, String> {
    let Some(fg) = crate::pty::foreground_pid(&state, id) else { return Ok(Vec::new()) };
    tauri::async_runtime::spawn_blocking(move || suggestions(fg)).await.map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn exec() -> Exec<'static> {
        Exec { cwd: Path::new("/"), path: vec![PathBuf::from("/bin"), PathBuf::from("/usr/bin")] }
    }

    #[test]
    fn blocos_de_shell() {
        let text = "Rode:\n\n```sh\n# comentário\nls -la\nsudo apt install \\\n  git\n```\n\n```ts\nconst x = 1;\n```\n\
            ```\nnão é comando\nls /tmp\n```\n```console\n$ echo oi\noi\n```\n";
        assert_eq!(extract(text, &exec()), ["ls -la", "sudo apt install \\\n  git", "ls /tmp", "echo oi"]);
    }

    #[test]
    fn comandos_no_texto() {
        let text = "Use `ls -la` ou `! cat x`. O arquivo `src/x.ts`, a função `foo()` e o `ls` sozinho não.";
        assert_eq!(extract(text, &exec()), ["ls -la", "cat x"]);
    }

    #[test]
    fn variavel_antes_do_comando() {
        assert_eq!(first_word("FOO=1 BAR=2 ls -la"), "ls");
    }

    #[test]
    fn pasta_do_projeto() {
        assert_eq!(project_slug("/home/x/code7/term.7"), "-home-x-code7-term-7");
    }
}
