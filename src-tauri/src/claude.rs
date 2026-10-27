//! Integração com o Claude Code: hooks avisam o estado do Claude em cada aba.
//!
//! O term7 exporta `TERM7_PANE` no shell de cada aba. O Claude herda a variável
//! e os hooks (instalados no settings.json dele) gravam o estado num arquivo
//! `$XDG_RUNTIME_DIR/term7/claude-<pane>`, que o polling da aba lê.
//! Fora do term7 a variável não existe e os hooks não fazem nada.

use std::{
    path::{Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use serde::Serialize;
use serde_json::{json, Map, Value};
use tauri::State;

use crate::pty::PtyState;

/// Marca que identifica os hooks do term7 no settings.json (pra atualizar/remover).
const MARKER: &str = "TERM7_PANE";

/// Evento do Claude Code -> estado da aba. None = sessão acabou (apaga o arquivo).
const EVENTS: &[(&str, Option<&str>)] = &[
    ("UserPromptSubmit", Some("working")),
    // Depois de uma permissão aprovada a ferramenta roda: volta a "trabalhando".
    ("PostToolUse", Some("working")),
    ("Notification", Some("waiting")),
    ("Stop", Some("done")),
    ("SessionEnd", None),
];

/// Id da aba visto pelos hooks. Leva o PID do app pra duas janelas (ou um
/// reinício) não lerem o arquivo uma da outra.
pub fn pane_key(id: u32) -> String {
    format!("{}-{id}", std::process::id())
}

fn state_dir() -> PathBuf {
    let base = std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from).unwrap_or_else(std::env::temp_dir);
    base.join("term7")
}

fn hook_command(state: Option<&str>) -> String {
    // Shell puro: não depende do binário do term7 nem de jq. Sempre sai com 0
    // pra nunca atrapalhar o Claude.
    match state {
        Some(s) => format!(
            r#"[ -n "${MARKER}" ] && d="${{XDG_RUNTIME_DIR:-/tmp}}/term7" && mkdir -p "$d" && echo {s} > "$d/claude-${MARKER}"; exit 0"#
        ),
        None => format!(r#"[ -n "${MARKER}" ] && rm -f "${{XDG_RUNTIME_DIR:-/tmp}}/term7/claude-${MARKER}"; exit 0"#),
    }
}

// ---- estado da aba ----

/// Estado gravado pelos hooks, se for da sessão do Claude que está rodando agora.
pub fn read_state(id: u32, foreground: u32) -> Option<String> {
    let path = state_dir().join(format!("claude-{}", pane_key(id)));
    let written = std::fs::metadata(&path).ok()?.modified().ok()?;
    // Arquivo de um Claude anterior (fechado sem SessionEnd): ignora.
    // A folga cobre o arredondamento do horário de boot.
    if let Some(start) = process_start(foreground) {
        if written + Duration::from_secs(2) < start {
            return None;
        }
    }
    let state = std::fs::read_to_string(&path).ok()?;
    let state = state.trim();
    matches!(state, "working" | "waiting" | "done").then(|| state.to_string())
}

/// Quando o processo começou: boot (btime, /proc/stat) + starttime (/proc/<pid>/stat).
fn process_start(pid: u32) -> Option<SystemTime> {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    // O nome do processo pode ter espaços: os campos começam depois do último ")".
    let fields: Vec<&str> = stat.rsplit_once(')')?.1.split_whitespace().collect();
    // starttime é o campo 22; depois do ")" o primeiro é o 3.
    let ticks: u64 = fields.get(19)?.parse().ok()?;
    let btime: u64 = std::fs::read_to_string("/proc/stat")
        .ok()?
        .lines()
        .find_map(|l| l.strip_prefix("btime ")?.trim().parse().ok())?;
    // /proc usa USER_HZ, que no Linux é sempre 100.
    Some(UNIX_EPOCH + Duration::from_secs(btime) + Duration::from_millis(ticks * 10))
}

// ---- instalação dos hooks ----

/// settings.json do Claude que roda na aba: respeita o CLAUDE_CONFIG_DIR do
/// processo (pode estar só no ~/.zshrc, que o app aberto pelo menu não vê).
fn settings_path(foreground: Option<u32>) -> Option<PathBuf> {
    let from_process = foreground.and_then(|pid| {
        let env = std::fs::read(format!("/proc/{pid}/environ")).ok()?;
        env.split(|&b| b == 0).find_map(|var| {
            let var = std::str::from_utf8(var).ok()?;
            var.strip_prefix("CLAUDE_CONFIG_DIR=").map(PathBuf::from)
        })
    });
    let dir = from_process
        .or_else(|| std::env::var_os("CLAUDE_CONFIG_DIR").map(PathBuf::from))
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".claude")))?;
    Some(dir.join("settings.json"))
}

fn read_settings(path: &Path) -> Result<Value, String> {
    match std::fs::read_to_string(path) {
        Ok(text) if text.trim().is_empty() => Ok(json!({})),
        Ok(text) => serde_json::from_str(&text).map_err(|e| format!("{} inválido: {e}", path.display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(json!({})),
        Err(e) => Err(e.to_string()),
    }
}

fn is_ours(entry: &Value) -> bool {
    entry["hooks"]
        .as_array()
        .is_some_and(|hooks| hooks.iter().any(|h| h["command"].as_str().is_some_and(|c| c.contains(MARKER))))
}

fn installed(settings: &Value) -> bool {
    EVENTS.iter().all(|(event, state)| {
        settings["hooks"][event].as_array().is_some_and(|entries| {
            entries.iter().any(|e| is_ours(e) && e["hooks"][0]["command"] == hook_command(*state).as_str())
        })
    })
}

/// Coloca (ou atualiza) os hooks do term7, sem mexer nos outros hooks nem no resto do arquivo.
fn install(path: &Path) -> Result<(), String> {
    let mut settings = read_settings(path)?;
    let root = settings.as_object_mut().ok_or("settings.json não é um objeto")?;
    let hooks = root.entry("hooks").or_insert_with(|| Value::Object(Map::new()));
    let hooks = hooks.as_object_mut().ok_or("\"hooks\" no settings.json não é um objeto")?;
    for (event, state) in EVENTS {
        let entries = hooks.entry(*event).or_insert_with(|| Value::Array(vec![]));
        let entries = entries.as_array_mut().ok_or(format!("\"hooks.{event}\" não é uma lista"))?;
        entries.retain(|e| !is_ours(e));
        entries.push(json!({ "hooks": [{ "type": "command", "command": hook_command(*state) }] }));
    }

    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    // Cópia do original na primeira vez, por segurança.
    let backup = path.with_extension("json.term7.bak");
    if path.exists() && !backup.exists() {
        std::fs::copy(path, &backup).map_err(|e| e.to_string())?;
    }
    let text = serde_json::to_string_pretty(&settings).map_err(|e| e.to_string())? + "\n";
    let tmp = path.with_extension("json.term7.tmp");
    std::fs::write(&tmp, text).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, path).map_err(|e| e.to_string())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HooksStatus {
    installed: bool,
    settings_path: String,
}

fn status(path: PathBuf) -> Result<HooksStatus, String> {
    let installed = installed(&read_settings(&path)?);
    Ok(HooksStatus { installed, settings_path: path.to_string_lossy().into_owned() })
}

fn path_for(state: &State<'_, PtyState>, id: u32) -> Result<PathBuf, String> {
    settings_path(crate::pty::foreground_pid(state, id)).ok_or_else(|| "pasta do Claude não encontrada".into())
}

#[tauri::command]
pub async fn claude_hooks_status(state: State<'_, PtyState>, id: u32) -> Result<HooksStatus, String> {
    let path = path_for(&state, id)?;
    tauri::async_runtime::spawn_blocking(move || status(path))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn claude_hooks_install(state: State<'_, PtyState>, id: u32) -> Result<HooksStatus, String> {
    let path = path_for(&state, id)?;
    tauri::async_runtime::spawn_blocking(move || {
        install(&path)?;
        status(path)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn instala_sem_apagar_o_resto() {
        let dir = std::env::temp_dir().join(format!("term7-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("settings.json");
        std::fs::write(
            &path,
            r#"{"model":"opus","hooks":{"Stop":[{"hooks":[{"type":"command","command":"echo meu"}]}]}}"#,
        )
        .unwrap();

        install(&path).unwrap();
        install(&path).unwrap(); // instalar de novo não duplica

        let s = read_settings(&path).unwrap();
        assert!(installed(&s));
        assert_eq!(s["model"], "opus");
        let stop = s["hooks"]["Stop"].as_array().unwrap();
        assert_eq!(stop.len(), 2);
        assert_eq!(stop[0]["hooks"][0]["command"], "echo meu");
        // A ordem das chaves do arquivo original é mantida.
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.find("\"model\"").unwrap() < text.find("\"hooks\"").unwrap());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn comando_do_hook() {
        assert_eq!(
            hook_command(Some("done")),
            r#"[ -n "$TERM7_PANE" ] && d="${XDG_RUNTIME_DIR:-/tmp}/term7" && mkdir -p "$d" && echo done > "$d/claude-$TERM7_PANE"; exit 0"#
        );
    }
}

// ---- servidores MCP do Claude da aba ----

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct McpServer {
    name: String,
    /// Endereço ou comando, sem query string nem argumentos (podem ter token).
    target: String,
    /// connected | needs_auth | failed | unknown
    status: &'static str,
    /// Motivo da falha, quando houver.
    detail: Option<String>,
}

/// Endereço sem credenciais: URL fica só com host e caminho; comando, só com o
/// executável e o primeiro argumento (ex.: `npx -y`, sem `--api-key ...`).
fn safe_target(target: &str) -> String {
    let target = target.trim();
    let (main, transport) = match target.rsplit_once(" (") {
        Some((m, t)) if t.ends_with(')') => (m, format!(" ({t}")),
        _ => (target, String::new()),
    };
    if let Some((scheme, rest)) = main.split_once("://") {
        let rest = rest.split(['?', '#']).next().unwrap_or("");
        let rest = rest.rsplit_once('@').map_or(rest, |(_, r)| r);
        format!("{scheme}://{rest}{transport}")
    } else {
        main.split_whitespace().take(2).collect::<Vec<_>>().join(" ")
    }
}

fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            // CSI: ESC [ ... letra final
            for c in chars.by_ref() {
                if c.is_ascii_alphabetic() {
                    break;
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// Lê a saída do `claude mcp list`: "nome: endereço - ✔ Connected".
fn parse_mcp_list(output: &str) -> Vec<McpServer> {
    strip_ansi(output)
        .lines()
        .filter_map(|line| {
            let (name, rest) = line.split_once(": ")?;
            let (target, status) = rest.rsplit_once(" - ")?;
            let lower = status.to_lowercase();
            let (status, detail) = if lower.contains("needs auth") || lower.contains("authenticat") {
                ("needs_auth", None)
            } else if lower.contains("connected") && !lower.contains("fail") {
                ("connected", None)
            } else if lower.contains("fail") || status.starts_with(['✘', '✗', '×']) {
                let detail = status.split_once('—').map(|(_, d)| d.trim().to_string());
                ("failed", detail)
            } else {
                ("unknown", Some(status.trim().to_string()))
            };
            Some(McpServer { name: name.trim().to_string(), target: safe_target(target), status, detail })
        })
        .collect()
}

/// Roda `claude mcp list` como o Claude da aba roda: mesmo binário, ambiente e
/// pasta (pega o CLAUDE_CONFIG_DIR e os MCPs do projeto certos).
fn mcp_list(foreground: u32) -> Result<Vec<McpServer>, String> {
    let comm = std::fs::read_to_string(format!("/proc/{foreground}/comm")).unwrap_or_default();
    if comm.trim() != "claude" {
        return Err("o Claude não está rodando nesta aba".into());
    }
    let exe = std::fs::read_link(format!("/proc/{foreground}/exe")).map_err(|e| e.to_string())?;
    let cwd = std::fs::read_link(format!("/proc/{foreground}/cwd")).map_err(|e| e.to_string())?;
    let environ = std::fs::read(format!("/proc/{foreground}/environ")).map_err(|e| e.to_string())?;
    let env = environ.split(|&b| b == 0).filter_map(|var| {
        let var = std::str::from_utf8(var).ok()?;
        let (k, v) = var.split_once('=')?;
        // Sem o TERM7_PANE: um SessionEnd desse comando apagaria o estado da aba.
        (k != MARKER).then(|| (k.to_string(), v.to_string()))
    });

    let mut child = std::process::Command::new(exe)
        .args(["mcp", "list"])
        .current_dir(cwd)
        .env_clear()
        .envs(env)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|e| e.to_string())?;

    // A checagem conecta em cada servidor; um travado não pode segurar pra sempre.
    let deadline = std::time::Instant::now() + Duration::from_secs(45);
    loop {
        match child.try_wait().map_err(|e| e.to_string())? {
            Some(_) => break,
            None if std::time::Instant::now() > deadline => {
                let _ = child.kill();
                let _ = child.wait();
                return Err("o claude mcp list demorou demais".into());
            }
            None => std::thread::sleep(Duration::from_millis(100)),
        }
    }
    let mut out = String::new();
    use std::io::Read;
    child.stdout.take().ok_or("sem saída")?.read_to_string(&mut out).map_err(|e| e.to_string())?;
    Ok(parse_mcp_list(&out))
}

#[tauri::command]
pub async fn claude_mcp_list(state: State<'_, PtyState>, id: u32) -> Result<Vec<McpServer>, String> {
    let fg = crate::pty::foreground_pid(&state, id).ok_or("aba não encontrada")?;
    tauri::async_runtime::spawn_blocking(move || mcp_list(fg))
        .await
        .map_err(|e| e.to_string())?
}

#[cfg(test)]
mod mcp_tests {
    use super::*;

    #[test]
    fn le_a_saida_do_mcp_list() {
        let out = "Checking MCP server health…\n\n\
            context7: https://mcp.context7.com/mcp (HTTP) - ✔ Connected\n\
            paper: http://127.0.0.1:29979/mcp (HTTP) - ✘ Failed to connect — ECONNREFUSED\n\
            penpot: https://design.penpot.app/mcp/stream?userToken=SEGREDO (HTTP) - ✔ Connected\n\
            claude.ai Gmail: https://gmailmcp.googleapis.com/mcp/v1 - ! Needs authentication\n\
            ctx: npx -y @upstash/context7-mcp --api-key SEGREDO - ✔ Connected\n";
        let s = parse_mcp_list(out);
        assert_eq!(s.len(), 5);
        assert_eq!(s[0].status, "connected");
        assert_eq!(s[1].status, "failed");
        assert_eq!(s[1].detail.as_deref(), Some("ECONNREFUSED"));
        assert_eq!(s[2].target, "https://design.penpot.app/mcp/stream (HTTP)");
        assert_eq!(s[3].name, "claude.ai Gmail");
        assert_eq!(s[3].status, "needs_auth");
        assert_eq!(s[4].target, "npx -y");
        assert!(s.iter().all(|x| !x.target.contains("SEGREDO")));
    }
}
