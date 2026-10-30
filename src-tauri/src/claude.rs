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

/// Evento do Claude Code (com matcher opcional) -> estado da aba.
/// None = apaga o arquivo (sessão nova ou encerrada).
const EVENTS: &[(&str, Option<&str>, Option<&str>)] = &[
    // Sessão nova (inclusive depois de /clear) começa sem estado.
    ("SessionStart", None, None),
    ("UserPromptSubmit", None, Some("working")),
    // Depois de uma permissão aprovada a ferramenta roda: volta a "trabalhando".
    ("PostToolUse", None, Some("working")),
    // Só o que de fato espera você. Sem filtro entrava o idle_prompt, que chega
    // ~60 s depois de terminar e deixava a aba "esperando" pra sempre.
    (
        "Notification",
        Some("permission_prompt|elicitation_dialog|elicitation_url_dialog|agent_needs_input"),
        Some("waiting"),
    ),
    ("Stop", None, Some("done")),
    ("SessionEnd", None, None),
];

fn hook_entry(matcher: Option<&str>, state: Option<&str>) -> Value {
    let mut entry = json!({ "hooks": [{ "type": "command", "command": hook_command(state) }] });
    if let Some(m) = matcher {
        entry["matcher"] = json!(m);
    }
    entry
}

/// Id da aba visto pelos hooks. Leva o PID do app pra duas janelas (ou um
/// reinício) não lerem o arquivo uma da outra.
pub fn pane_key(id: u32) -> String {
    format!("{}-{id}", std::process::id())
}

pub(crate) fn state_dir() -> PathBuf {
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
pub(crate) fn settings_path(foreground: Option<u32>) -> Option<PathBuf> {
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

pub(crate) fn read_settings(path: &Path) -> Result<Value, String> {
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
    EVENTS.iter().all(|(event, matcher, state)| {
        let expected = hook_entry(*matcher, *state);
        settings["hooks"][event]
            .as_array()
            .is_some_and(|entries| entries.iter().any(|e| is_ours(e) && *e == expected))
    })
}

/// Tem algum hook do term7, mesmo que de uma versão antiga.
fn has_ours(settings: &Value) -> bool {
    settings["hooks"]
        .as_object()
        .is_some_and(|hooks| hooks.values().filter_map(Value::as_array).flatten().any(is_ours))
}

/// Coloca (ou atualiza) os hooks do term7, sem mexer nos outros hooks nem no resto do arquivo.
fn install(path: &Path) -> Result<(), String> {
    let mut settings = read_settings(path)?;
    let root = settings.as_object_mut().ok_or("settings.json não é um objeto")?;
    let hooks = root.entry("hooks").or_insert_with(|| Value::Object(Map::new()));
    let hooks = hooks.as_object_mut().ok_or("\"hooks\" no settings.json não é um objeto")?;
    // Tira os do term7 de todos os eventos (inclusive de eventos que deixaram de ser usados).
    for entries in hooks.values_mut().filter_map(Value::as_array_mut) {
        entries.retain(|e| !is_ours(e));
    }
    hooks.retain(|_, v| v.as_array().is_none_or(|a| !a.is_empty()));
    for (event, matcher, state) in EVENTS {
        let entries = hooks.entry(*event).or_insert_with(|| Value::Array(vec![]));
        let entries = entries.as_array_mut().ok_or(format!("\"hooks.{event}\" não é uma lista"))?;
        entries.push(hook_entry(*matcher, *state));
    }

    write_settings(path, &settings)
}

/// Grava o settings.json do Claude: cópia do original na primeira vez e troca atômica.
pub(crate) fn write_settings(path: &Path, settings: &Value) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    // Cópia do original na primeira vez, por segurança.
    let backup = path.with_extension("json.term7.bak");
    if path.exists() && !backup.exists() {
        std::fs::copy(path, &backup).map_err(|e| e.to_string())?;
    }
    let text = serde_json::to_string_pretty(settings).map_err(|e| e.to_string())? + "\n";
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
    let settings = read_settings(&path)?;
    // Já ativou antes, mas com hooks de uma versão antiga: atualiza sozinho.
    if has_ours(&settings) && !installed(&settings) {
        install(&path)?;
        return status(path);
    }
    let installed = installed(&settings);
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
        assert!(has_ours(&read_settings(&path).unwrap()));

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
    fn notification_ignora_o_idle_prompt() {
        let entry = hook_entry(EVENTS[3].1, EVENTS[3].2);
        assert_eq!(EVENTS[3].0, "Notification");
        let matcher = entry["matcher"].as_str().unwrap();
        assert!(matcher.contains("permission_prompt"));
        assert!(!matcher.contains("idle_prompt"));
    }

    #[test]
    fn atualiza_hooks_antigos() {
        let dir = std::env::temp_dir().join(format!("term7-test-old-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("settings.json");
        // Instalação da versão anterior: Notification sem matcher e sem SessionStart.
        let old = json!({ "hooks": { "Notification": [{ "hooks": [{ "type": "command", "command": hook_command(Some("waiting")) }] }] } });
        std::fs::write(&path, old.to_string()).unwrap();
        let st = status(path.clone()).unwrap();
        assert!(st.installed);
        let s = read_settings(&path).unwrap();
        assert_eq!(s["hooks"]["Notification"].as_array().unwrap().len(), 1);
        assert!(s["hooks"]["SessionStart"].is_array());
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
    /// Servidor de escopo de usuário (pedido pelo botão "Ver os globais").
    global: bool,
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

/// Status do Claude ("✔ Connected", "! Needs authentication"...) -> (status, motivo).
fn classify(status: &str) -> (&'static str, Option<String>) {
    let status = status.trim();
    let lower = status.to_lowercase();
    if lower.contains("needs auth") || lower.contains("authenticat") {
        ("needs_auth", None)
    } else if lower.contains("connected") && !lower.contains("fail") {
        ("connected", None)
    } else if lower.contains("fail") || status.starts_with(['✘', '✗', '×']) {
        ("failed", status.split_once('—').map(|(_, d)| d.trim().to_string()))
    } else {
        ("unknown", Some(status.to_string()))
    }
}

/// Lê a saída do `claude mcp get <nome>`. Só olha Status, URL e Command: os
/// headers e variáveis de ambiente (onde ficam os tokens) nunca são lidos.
fn parse_mcp_get(name: &str, output: &str) -> McpServer {
    let mut status = None;
    let mut target = String::new();
    for line in strip_ansi(output).lines() {
        let line = line.trim();
        if let Some(v) = line.strip_prefix("Status:") {
            status = Some(v.trim().to_string());
        } else if let Some(v) = line.strip_prefix("URL:").or_else(|| line.strip_prefix("Command:")) {
            target = safe_target(v);
        }
    }
    let (status, detail) = match status {
        Some(s) => classify(&s),
        None => ("unknown", Some("o Claude não informou o status".into())),
    };
    McpServer { name: name.to_string(), target, status, detail, global: false }
}

/// Nomes dos MCPs do projeto: os do `.mcp.json` e os de escopo local (no
/// `.claude.json`), na pasta do Claude ou acima dela. Os de usuário e os
/// conectores do claude.ai ficam de fora: valem pra tudo e demoram pra checar.
fn project_servers(cwd: &Path, config: &Value, home: Option<&Path>) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    let mut add = |servers: Option<&Value>| {
        for name in servers.and_then(Value::as_object).into_iter().flat_map(|m| m.keys()) {
            if !names.contains(name) {
                names.push(name.clone());
            }
        }
    };
    for dir in cwd.ancestors() {
        // A home não é projeto: um .mcp.json nela valeria pra tudo.
        if Some(dir) == home || dir.parent().is_none() {
            break;
        }
        let key = dir.to_string_lossy();
        add(config.pointer(&format!("/projects/{}/mcpServers", key.replace('~', "~0").replace('/', "~1"))));
        if let Ok(text) = std::fs::read_to_string(dir.join(".mcp.json")) {
            if let Ok(json) = serde_json::from_str::<Value>(&text) {
                add(json.get("mcpServers"));
            }
        }
    }
    names
}

/// Globais: escopo de usuário (`mcpServers` na raiz do `.claude.json`). Os
/// conectores do claude.ai ficam de fora: não estão no arquivo e checar todos é lento.
fn user_servers(config: &Value) -> Vec<String> {
    config.get("mcpServers").and_then(Value::as_object).map(|m| m.keys().cloned().collect()).unwrap_or_default()
}

/// Roda o binário do Claude da aba com o ambiente e a pasta dele, com prazo.
fn run_claude(exe: &Path, cwd: &Path, env: &[(String, String)], args: &[&str], timeout: Duration) -> Result<String, String> {
    let mut child = std::process::Command::new(exe)
        .args(args)
        .current_dir(cwd)
        .env_clear()
        .envs(env.iter().cloned())
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|e| e.to_string())?;
    // A checagem conecta no servidor; um travado não pode segurar pra sempre.
    let deadline = std::time::Instant::now() + timeout;
    loop {
        match child.try_wait().map_err(|e| e.to_string())? {
            Some(_) => break,
            None if std::time::Instant::now() > deadline => {
                let _ = child.kill();
                let _ = child.wait();
                return Err("demorou demais pra responder".into());
            }
            None => std::thread::sleep(Duration::from_millis(100)),
        }
    }
    let mut out = String::new();
    use std::io::Read;
    child.stdout.take().ok_or("sem saída")?.read_to_string(&mut out).map_err(|e| e.to_string())?;
    Ok(out)
}

/// Checa os MCPs do projeto como o Claude da aba faria: mesmo binário,
/// ambiente (CLAUDE_CONFIG_DIR) e pasta. Um `claude mcp get` por servidor, em paralelo.
fn mcp_list(foreground: u32, global: bool) -> Result<Vec<McpServer>, String> {
    let comm = std::fs::read_to_string(format!("/proc/{foreground}/comm")).unwrap_or_default();
    if comm.trim() != "claude" {
        return Err("o Claude não está rodando nesta aba".into());
    }
    let exe = std::fs::read_link(format!("/proc/{foreground}/exe")).map_err(|e| e.to_string())?;
    let cwd = std::fs::read_link(format!("/proc/{foreground}/cwd")).map_err(|e| e.to_string())?;
    let environ = std::fs::read(format!("/proc/{foreground}/environ")).map_err(|e| e.to_string())?;
    let env: Vec<(String, String)> = environ
        .split(|&b| b == 0)
        .filter_map(|var| {
            let (k, v) = std::str::from_utf8(var).ok()?.split_once('=')?;
            // Sem o TERM7_PANE: um SessionEnd desse comando apagaria o estado da aba.
            (k != MARKER).then(|| (k.to_string(), v.to_string()))
        })
        .collect();
    let var = |name: &str| env.iter().find(|(k, _)| k == name).map(|(_, v)| PathBuf::from(v));
    let home = var("HOME");
    let config_file = match var("CLAUDE_CONFIG_DIR") {
        Some(dir) => dir.join(".claude.json"),
        None => home.clone().unwrap_or_default().join(".claude.json"),
    };
    let config = read_settings(&config_file).unwrap_or(Value::Null);

    let names = if global { user_servers(&config) } else { project_servers(&cwd, &config, home.as_deref()) };
    let handles: Vec<_> = names
        .into_iter()
        .map(|name| {
            let (exe, cwd, env) = (exe.clone(), cwd.clone(), env.clone());
            std::thread::spawn(move || {
                let server = match run_claude(&exe, &cwd, &env, &["mcp", "get", &name], Duration::from_secs(30)) {
                    Ok(out) => parse_mcp_get(&name, &out),
                    Err(e) => McpServer { name, target: String::new(), status: "failed", detail: Some(e), global: false },
                };
                McpServer { global, ..server }
            })
        })
        .collect();
    Ok(handles.into_iter().filter_map(|h| h.join().ok()).collect())
}

#[tauri::command]
pub async fn claude_mcp_list(state: State<'_, PtyState>, id: u32, global: bool) -> Result<Vec<McpServer>, String> {
    let fg = crate::pty::foreground_pid(&state, id).ok_or("aba não encontrada")?;
    tauri::async_runtime::spawn_blocking(move || mcp_list(fg, global))
        .await
        .map_err(|e| e.to_string())?
}

#[cfg(test)]
mod mcp_tests {
    use super::*;

    #[test]
    fn le_a_saida_do_mcp_get() {
        let out = "context7:\n  Scope: Local config\n  Status: ✔ Connected\n  Type: http\n  \
            URL: https://mcp.context7.com/mcp?key=SEGREDO\n  Headers:\n    Authorization: Bearer SEGREDO\n";
        let s = parse_mcp_get("context7", out);
        assert_eq!(s.status, "connected");
        assert_eq!(s.target, "https://mcp.context7.com/mcp");

        let out = "x:\n  Status: ✘ Failed to connect — ECONNREFUSED\n  Command: npx -y pacote --api-key SEGREDO\n  \
            Environment:\n    TOKEN=SEGREDO\n";
        let s = parse_mcp_get("x", out);
        assert_eq!(s.status, "failed");
        assert_eq!(s.detail.as_deref(), Some("ECONNREFUSED"));
        assert_eq!(s.target, "npx -y");

        assert_eq!(parse_mcp_get("y", "y:\n  Status: ! Needs authentication\n").status, "needs_auth");
    }

    #[test]
    fn so_os_mcps_do_projeto() {
        let dir = std::env::temp_dir().join(format!("term7-mcp-{}", std::process::id()));
        let sub = dir.join("app/src");
        std::fs::create_dir_all(&sub).unwrap();
        std::fs::write(dir.join("app/.mcp.json"), r#"{"mcpServers":{"playwright":{},"local":{}}}"#).unwrap();
        let app = dir.join("app").to_string_lossy().to_string();
        let config = json!({
            "mcpServers": { "global": {} },
            "projects": { app: { "mcpServers": { "local": {}, "blender": {} } } }
        });
        let names = project_servers(&sub, &config, Some(&dir));
        assert_eq!(names, ["local", "blender", "playwright"]);
        assert!(project_servers(&dir, &config, Some(&dir)).is_empty());
        assert_eq!(user_servers(&config), ["global"]);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
