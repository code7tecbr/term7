//! Uso da assinatura do Claude (limites de 5 h, 7 dias e os por modelo, como o
//! do Fable nas contas Max), pra barra de cima.
//!
//! O Claude só entrega esses números pra linha de status (`rate_limits` no JSON
//! que ele manda pro comando do `statusLine`). O term7 se coloca na frente do
//! comando do usuário: guarda uma cópia do JSON em
//! `$XDG_RUNTIME_DIR/term7/usage-<pasta de configuração>.json` e repassa tudo
//! pro comando original, que continua desenhando a linha igual. O `statusLine`
//! original fica em `<pasta do Claude>/term7-statusline.json`, pra desfazer.
//! Cada sessão também ganha o seu `session-<id>.json`, de onde sai o contexto.
//!
//! Os limites por modelo (o do Fable) não vêm na linha de status: saem do
//! `get_usage` do modo headless do Claude, o mesmo que o `/usage` mostra. O
//! term7 roda um Claude igual ao da aba (binário e ambiente) a cada 5 minutos,
//! sem hooks e sem mandar mensagem nenhuma: não gasta uso nem cria sessão.

use std::{
    collections::HashMap,
    io::{BufRead, BufReader, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{mpsc, Mutex, OnceLock},
    time::{Duration, Instant, UNIX_EPOCH},
};

use serde::Serialize;
use serde_json::{json, Value};
use tauri::State;

use crate::{
    claude::{claude_proc, read_settings, settings_path, state_dir, write_settings, ClaudeProc},
    pty::PtyState,
    suggest,
};

/// Marca o comando do term7 no settings.json.
const MARK: &str = "TERM7_USAGE=1";
const BACKUP: &str = "term7-statusline.json";

/// Pasta de configuração no nome do arquivo: "/home/u/.claude-code7" -> "-home-u--claude-code7".
fn slug(dir: &Path) -> String {
    let dir = dir.to_string_lossy();
    dir.trim_end_matches('/').chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '-' }).collect()
}

fn usage_file(config: &Path) -> PathBuf {
    state_dir().join(format!("usage-{}.json", slug(config)))
}

/// Aspas simples pro sh, sempre.
fn sh_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

/// Comando que guarda o JSON e repassa pro original. O mesmo cálculo do nome do
/// arquivo é feito no shell, com a pasta de configuração do Claude que roda.
fn wrapper(original: Option<&str>) -> String {
    let save = r#"d="${XDG_RUNTIME_DIR:-/tmp}/term7"; mkdir -p "$d"; c="${CLAUDE_CONFIG_DIR:-$HOME/.claude}"; f="$d/usage-$(printf %s "${c%/}" | tr -c 'A-Za-z0-9' '-').json""#;
    let forward = match original {
        Some(cmd) => format!(r#"tee "$f.$$" | sh -c {}"#, sh_quote(cmd)),
        None => r#"cat > "$f.$$""#.to_string(),
    };
    // O id da sessão é o primeiro "session_id" do JSON (só hexadecimal e hífen).
    let per_session = r#"i=$(grep -o '"session_id" *: *"[0-9a-f-]*"' "$f.$$" | head -n 1 | sed 's/.*"\([0-9a-f-]*\)"$/\1/'); [ -n "$i" ] && cp -f "$f.$$" "$d/session-$i.json.$$" && mv -f "$d/session-$i.json.$$" "$d/session-$i.json""#;
    format!(r#"{MARK}; {save}; {forward}; s=$?; {per_session}; mv -f "$f.$$" "$f" 2>/dev/null; exit $s"#)
}

fn is_ours(settings: &Value) -> bool {
    settings["statusLine"]["command"].as_str().is_some_and(|c| c.starts_with(MARK))
}

/// Instalado por uma versão antiga do term7 (comando diferente do atual).
fn outdated(path: &Path, settings: &Value) -> bool {
    let Ok(orig) = original(path, settings) else { return false };
    let cmd = orig.get("command").and_then(Value::as_str).filter(|_| orig["type"] == "command");
    is_ours(settings) && settings["statusLine"]["command"].as_str() != Some(wrapper(cmd).as_str())
}

fn backup_path(settings: &Path) -> PathBuf {
    settings.with_file_name(BACKUP)
}

/// O statusLine que estava antes do term7 (null = não havia).
fn original(settings_file: &Path, settings: &Value) -> Result<Value, String> {
    if !is_ours(settings) {
        return Ok(settings.get("statusLine").cloned().unwrap_or(Value::Null));
    }
    match std::fs::read_to_string(backup_path(settings_file)) {
        Ok(text) => serde_json::from_str(&text).map_err(|e| format!("{BACKUP} inválido: {e}")),
        Err(_) => Ok(Value::Null),
    }
}

fn install(path: &Path) -> Result<(), String> {
    let mut settings = read_settings(path)?;
    let orig = original(path, &settings)?;
    if !is_ours(&settings) {
        let text = serde_json::to_string_pretty(&orig).map_err(|e| e.to_string())?;
        std::fs::write(backup_path(path), text + "\n").map_err(|e| e.to_string())?;
    }
    let cmd = orig.get("command").and_then(Value::as_str).filter(|_| orig["type"] == "command");
    // Mantém o resto (padding etc.) do statusLine original; só troca o comando.
    let mut line = if orig.is_object() { orig.clone() } else { json!({ "type": "command" }) };
    line["type"] = json!("command");
    line["command"] = json!(wrapper(cmd));
    settings.as_object_mut().ok_or("settings.json não é um objeto")?.insert("statusLine".into(), line);
    write_settings(path, &settings)
}

fn remove(path: &Path) -> Result<(), String> {
    let mut settings = read_settings(path)?;
    if !is_ours(&settings) {
        return Ok(());
    }
    let orig = original(path, &settings)?;
    let root = settings.as_object_mut().ok_or("settings.json não é um objeto")?;
    if orig.is_null() {
        root.remove("statusLine");
    } else {
        root.insert("statusLine".into(), orig);
    }
    write_settings(path, &settings)?;
    let _ = std::fs::remove_file(backup_path(path));
    Ok(())
}

#[derive(Serialize, Clone)]
pub struct Window {
    /// Como vem do Claude: "five_hour", "seven_day", "seven_day_fable"...
    key: String,
    used: f64,
    /// Unix, em segundos.
    resets_at: Option<i64>,
}

#[derive(Serialize)]
pub struct Context {
    used: f64,
    /// Tamanho da janela, em tokens.
    size: Option<u64>,
}

#[derive(Serialize)]
pub struct Usage {
    /// O statusLine do term7 está no settings.json desse Claude.
    installed: bool,
    windows: Vec<Window>,
    /// Contexto usado pela sessão da aba (da última linha de status dela).
    context: Option<Context>,
    /// Quando o Claude mandou os números pela última vez (Unix, segundos).
    updated: Option<u64>,
    settings_path: String,
}

fn windows(json: &Value) -> Vec<Window> {
    let Some(limits) = json["rate_limits"].as_object() else { return Vec::new() };
    limits
        .iter()
        .filter_map(|(key, w)| {
            Some(Window {
                key: key.clone(),
                used: w.get("used_percentage")?.as_f64()?,
                resets_at: w.get("resets_at").and_then(Value::as_i64),
            })
        })
        .collect()
}

fn read_json(path: &Path) -> Option<Value> {
    serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()
}

fn context(json: &Value) -> Option<Context> {
    let w = &json["context_window"];
    Some(Context { used: w["used_percentage"].as_f64()?, size: w["context_window_size"].as_u64() })
}

/// "2026-11-06T13:00:00.498+00:00" -> Unix. Só o formato que a API do Claude usa.
fn parse_time(s: &str) -> Option<i64> {
    let num = |r: std::ops::Range<usize>| s.get(r)?.parse::<i64>().ok();
    let (y, mo, d, h, mi, se) = (num(0..4)?, num(5..7)?, num(8..10)?, num(11..13)?, num(14..16)?, num(17..19)?);
    // Dias desde 1970 (algoritmo de Howard Hinnant).
    let y2 = if mo <= 2 { y - 1 } else { y };
    let era = y2.div_euclid(400);
    let yoe = y2 - era * 400;
    let doy = (153 * (mo + if mo > 2 { -3 } else { 9 }) + 2) / 5 + d - 1;
    let days = era * 146_097 + yoe * 365 + yoe / 4 - yoe / 100 + doy - 719_468;
    let rest = &s[19..];
    let offset = match rest.find(['+', '-']) {
        Some(i) => {
            let sign = if rest.as_bytes()[i] == b'-' { -1 } else { 1 };
            let oh: i64 = rest.get(i + 1..i + 3)?.parse().ok()?;
            let om: i64 = rest.get(i + 4..i + 6)?.parse().ok()?;
            sign * (oh * 3600 + om * 60)
        }
        None => 0,
    };
    Some(days * 86_400 + h * 3600 + mi * 60 + se - offset)
}

/// Os `limits` do get_usage com as mesmas chaves da linha de status;
/// os por modelo viram "seven_day_<nome>".
fn limits_windows(limits: &Value) -> Vec<Window> {
    let Some(list) = limits.as_array() else { return Vec::new() };
    list.iter()
        .filter_map(|l| {
            let key = match l["kind"].as_str()? {
                "session" => "five_hour".to_string(),
                "weekly_all" => "seven_day".to_string(),
                "weekly_scoped" => {
                    let name = l["scope"]["model"]["display_name"].as_str()?;
                    let name: String = name.to_lowercase().chars().map(|c| if c.is_alphanumeric() { c } else { '_' }).collect();
                    format!("seven_day_{name}")
                }
                _ => return None,
            };
            Some(Window { key, used: l["percent"].as_f64()?, resets_at: l["resets_at"].as_str().and_then(parse_time) })
        })
        .collect()
}

const GET_USAGE: &str = r#"{"type":"control_request","request_id":"term7-usage","request":{"subtype":"get_usage"}}"#;
const LIMITS_EVERY: Duration = Duration::from_secs(5 * 60);

/// Pergunta os limites a um Claude headless igual ao da aba.
fn fetch_limits(p: &ClaudeProc) -> Result<Vec<Window>, String> {
    let mut child = Command::new(&p.exe)
        .args(["-p", "--input-format", "stream-json", "--output-format", "stream-json", "--verbose"])
        // Os hooks do usuário não têm por que rodar a cada consulta.
        .args(["--settings", r#"{"disableAllHooks":true}"#])
        .current_dir(&p.cwd)
        .env_clear()
        .envs(p.env.iter().cloned())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| e.to_string())?;
    // Com o stdin fechado, ele responde e sai sozinho.
    child.stdin.take().ok_or("sem stdin")?.write_all(format!("{GET_USAGE}\n").as_bytes()).map_err(|e| e.to_string())?;
    let stdout = child.stdout.take().ok_or("sem stdout")?;
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            if line.contains(r#""control_response""#) {
                let _ = tx.send(line);
                return;
            }
        }
    });
    let line = rx.recv_timeout(Duration::from_secs(30));
    let deadline = Instant::now() + Duration::from_secs(5);
    while child.try_wait().ok().flatten().is_none() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(100));
    }
    let _ = child.kill();
    let _ = child.wait();
    let json: Value = serde_json::from_str(&line.map_err(|_| "o Claude não respondeu")?).map_err(|e| e.to_string())?;
    Ok(limits_windows(&json["response"]["response"]["rate_limits"]["limits"]))
}

struct Limits {
    at: Instant,
    windows: Vec<Window>,
    busy: bool,
}

fn limits_cache() -> &'static Mutex<HashMap<PathBuf, Limits>> {
    static CACHE: OnceLock<Mutex<HashMap<PathBuf, Limits>>> = OnceLock::new();
    CACHE.get_or_init(Default::default)
}

/// Limites guardados dessa pasta do Claude; vencidos, atualiza em segundo plano.
fn cached_limits(config: &Path, pid: u32) -> Vec<Window> {
    let mut cache = limits_cache().lock().unwrap();
    let entry = cache.entry(config.to_path_buf()).or_insert(Limits { at: Instant::now() - LIMITS_EVERY, windows: Vec::new(), busy: false });
    if !entry.busy && entry.at.elapsed() >= LIMITS_EVERY {
        if let Ok(proc) = claude_proc(pid) {
            entry.busy = true;
            let config = config.to_path_buf();
            std::thread::spawn(move || {
                let got = fetch_limits(&proc);
                let mut cache = limits_cache().lock().unwrap();
                if let Some(e) = cache.get_mut(&config) {
                    // Se falhar, fica com os últimos e tenta de novo na próxima volta.
                    if let Ok(w) = got {
                        e.windows = w;
                    }
                    e.at = Instant::now();
                    e.busy = false;
                }
            });
        }
    }
    entry.windows.clone()
}

/// A linha de status vale mais (muda a cada resposta); o get_usage completa o que falta.
fn merge(mut live: Vec<Window>, limits: Vec<Window>) -> Vec<Window> {
    for w in limits {
        if !live.iter().any(|l| l.key == w.key) {
            live.push(w);
        }
    }
    let rank = |k: &str| match k {
        "five_hour" => 0,
        "seven_day" => 1,
        _ => 2,
    };
    live.sort_by(|a, b| rank(&a.key).cmp(&rank(&b.key)).then_with(|| a.key.cmp(&b.key)));
    live
}

fn read(path: &Path, pid: Option<u32>) -> Result<Usage, String> {
    let mut settings = read_settings(path)?;
    // Versão antiga do comando: troca pela atual (sem perder o original).
    if outdated(path, &settings) {
        install(path)?;
        settings = read_settings(path)?;
    }
    let installed = is_ours(&settings);
    let config = path.parent().ok_or("pasta do Claude não encontrada")?;
    let file = usage_file(config);
    let updated = std::fs::metadata(&file).ok().and_then(|m| m.modified().ok()).and_then(|t| t.duration_since(UNIX_EPOCH).ok()).map(|d| d.as_secs());
    let live = read_json(&file).map(|j| windows(&j)).unwrap_or_default();
    let limits = match pid {
        Some(pid) if installed => cached_limits(config, pid),
        _ => Vec::new(),
    };
    let session = pid.and_then(|pid| suggest::session_id(pid, &suggest::read_proc(pid)?));
    let context = session.and_then(|id| read_json(&state_dir().join(format!("session-{id}.json")))).and_then(|j| context(&j));
    Ok(Usage { installed, windows: merge(live, limits), context, updated, settings_path: path.to_string_lossy().into_owned() })
}

#[tauri::command]
pub async fn claude_usage(state: State<'_, PtyState>, id: u32) -> Result<Usage, String> {
    let pid = crate::pty::foreground_pid(&state, id);
    let path = settings_path(pid).ok_or("pasta do Claude não encontrada")?;
    tauri::async_runtime::spawn_blocking(move || read(&path, pid)).await.map_err(|e| e.to_string())?
}

/// Liga (`enabled`) ou desfaz a captura do uso no settings.json do Claude da aba.
#[tauri::command]
pub async fn claude_usage_setup(state: State<'_, PtyState>, id: u32, enabled: bool) -> Result<Usage, String> {
    let pid = crate::pty::foreground_pid(&state, id);
    let path = settings_path(pid).ok_or("pasta do Claude não encontrada")?;
    tauri::async_runtime::spawn_blocking(move || {
        if enabled {
            install(&path)?;
        } else {
            remove(&path)?;
        }
        read(&path, pid)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_settings(name: &str, content: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("term7-usage-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("settings.json");
        std::fs::write(&path, content).unwrap();
        path
    }

    #[test]
    fn instala_e_desfaz_mantendo_o_original() {
        let path = tmp_settings("orig", r#"{"model":"opus","statusLine":{"type":"command","command":"bash ~/x.sh","padding":0}}"#);
        install(&path).unwrap();
        let s = read_settings(&path).unwrap();
        let cmd = s["statusLine"]["command"].as_str().unwrap();
        assert!(cmd.starts_with(MARK) && cmd.contains("sh -c 'bash ~/x.sh'"));
        assert_eq!(s["statusLine"]["padding"], 0);
        assert_eq!(s["model"], "opus");
        // Instalar de novo não perde o original.
        install(&path).unwrap();
        remove(&path).unwrap();
        let s = read_settings(&path).unwrap();
        assert_eq!(s["statusLine"], json!({"type":"command","command":"bash ~/x.sh","padding":0}));
        assert!(!backup_path(&path).exists());
    }

    #[test]
    fn sem_statusline_antes() {
        let path = tmp_settings("vazio", "{}");
        install(&path).unwrap();
        assert!(read_settings(&path).unwrap()["statusLine"]["command"].as_str().unwrap().contains(r#"cat > "$f.$$""#));
        remove(&path).unwrap();
        assert!(read_settings(&path).unwrap().get("statusLine").is_none());
    }

    #[test]
    fn le_as_janelas() {
        let j = json!({"rate_limits": {
            "seven_day_fable": {"used_percentage": 34.0, "resets_at": 20},
            "seven_day": {"used_percentage": 22.4, "resets_at": 10},
            "five_hour": {"used_percentage": 23, "resets_at": 5},
            "lixo": {"x": 1}
        }});
        let w = merge(windows(&j), Vec::new());
        assert_eq!(w.iter().map(|w| w.key.as_str()).collect::<Vec<_>>(), ["five_hour", "seven_day", "seven_day_fable"]);
        assert_eq!(w[0].used, 23.0);
        assert_eq!(slug(Path::new("/home/u/.claude-code7/")), "-home-u--claude-code7");
    }

    #[test]
    fn le_os_limites_do_get_usage() {
        let limits = json!([
            {"kind": "session", "percent": 3, "resets_at": "2026-11-02T22:10:00.498396+00:00", "scope": null},
            {"kind": "weekly_all", "percent": 15, "resets_at": "2026-11-06T13:00:00+00:00", "scope": null},
            {"kind": "weekly_scoped", "percent": 7, "resets_at": "2026-11-06T13:00:00Z", "scope": {"model": {"display_name": "Fable"}}},
            {"kind": "weekly_scoped", "percent": 1, "scope": {"surface": {"display_name": "Cowork"}}}
        ]);
        let w = limits_windows(&limits);
        assert_eq!(w.iter().map(|w| w.key.as_str()).collect::<Vec<_>>(), ["five_hour", "seven_day", "seven_day_fable"]);
        assert_eq!(w[2].resets_at, Some(1_793_970_000));
        assert_eq!(parse_time("2026-11-02T19:10:00-03:00"), parse_time("2026-11-02T22:10:00+00:00"));
        // A linha de status ganha nas chaves repetidas.
        let live = vec![Window { key: "five_hour".into(), used: 9.0, resets_at: None }];
        let m = merge(live, w);
        assert_eq!(m.iter().map(|w| (w.key.as_str(), w.used)).collect::<Vec<_>>(), [("five_hour", 9.0), ("seven_day", 15.0), ("seven_day_fable", 7.0)]);
    }

    #[test]
    fn o_shell_grava_o_arquivo_e_repassa() {
        let dir = std::env::temp_dir().join(format!("term7-usage-sh-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let out = std::process::Command::new("sh")
            .arg("-c")
            .arg(wrapper(Some("cat; echo ' ok'")))
            .env("PATH", std::env::var("PATH").unwrap_or_default())
            .env("XDG_RUNTIME_DIR", &dir)
            .env("CLAUDE_CONFIG_DIR", "/x/.claude-code7/")
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .spawn()
            .and_then(|mut c| {
                use std::io::Write;
                c.stdin.take().unwrap().write_all(br#"{"a":1,"session_id":"ab-12","x":{"session_id":"ff"}}"#)?;
                c.wait_with_output()
            })
            .unwrap();
        let input = r#"{"a":1,"session_id":"ab-12","x":{"session_id":"ff"}}"#;
        assert_eq!(String::from_utf8_lossy(&out.stdout), format!("{input} ok\n"));
        let saved = std::fs::read_to_string(dir.join("term7").join("usage--x--claude-code7.json")).unwrap();
        assert_eq!(saved, input);
        assert_eq!(std::fs::read_to_string(dir.join("term7").join("session-ab-12.json")).unwrap(), input);
        // Mesmo nome que o Rust calcula.
        assert_eq!(usage_file(Path::new("/x/.claude-code7")).file_name(), Some(std::ffi::OsStr::new("usage--x--claude-code7.json")));
    }
}
