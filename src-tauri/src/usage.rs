//! Uso da assinatura do Claude (limites de 5 h, 7 dias e os por modelo, como o
//! do Fable nas contas Max), pra barra de cima.
//!
//! O Claude só entrega esses números pra linha de status (`rate_limits` no JSON
//! que ele manda pro comando do `statusLine`). O term7 se coloca na frente do
//! comando do usuário: guarda uma cópia do JSON em
//! `$XDG_RUNTIME_DIR/term7/usage-<pasta de configuração>.json` e repassa tudo
//! pro comando original, que continua desenhando a linha igual. O `statusLine`
//! original fica em `<pasta do Claude>/term7-statusline.json`, pra desfazer.

use std::{
    path::{Path, PathBuf},
    time::UNIX_EPOCH,
};

use serde::Serialize;
use serde_json::{json, Value};
use tauri::State;

use crate::{
    claude::{read_settings, settings_path, state_dir, write_settings},
    pty::PtyState,
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
    format!(r#"{MARK}; {save}; {forward}; s=$?; mv -f "$f.$$" "$f" 2>/dev/null; exit $s"#)
}

fn is_ours(settings: &Value) -> bool {
    settings["statusLine"]["command"].as_str().is_some_and(|c| c.starts_with(MARK))
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

#[derive(Serialize)]
pub struct Window {
    /// Como vem do Claude: "five_hour", "seven_day", "seven_day_fable"...
    key: String,
    used: f64,
    /// Unix, em segundos.
    resets_at: Option<i64>,
}

#[derive(Serialize)]
pub struct Usage {
    /// O statusLine do term7 está no settings.json desse Claude.
    installed: bool,
    windows: Vec<Window>,
    /// Quando o Claude mandou os números pela última vez (Unix, segundos).
    updated: Option<u64>,
    settings_path: String,
}

fn windows(json: &Value) -> Vec<Window> {
    let Some(limits) = json["rate_limits"].as_object() else { return Vec::new() };
    let mut list: Vec<Window> = limits
        .iter()
        .filter_map(|(key, w)| {
            Some(Window {
                key: key.clone(),
                used: w.get("used_percentage")?.as_f64()?,
                resets_at: w.get("resets_at").and_then(Value::as_i64),
            })
        })
        .collect();
    // 5 h primeiro, depois 7 dias, depois os por modelo.
    let rank = |k: &str| match k {
        "five_hour" => 0,
        "seven_day" => 1,
        _ => 2,
    };
    list.sort_by(|a, b| rank(&a.key).cmp(&rank(&b.key)).then_with(|| a.key.cmp(&b.key)));
    list
}

fn read(path: &Path) -> Result<Usage, String> {
    let settings = read_settings(path)?;
    let config = path.parent().ok_or("pasta do Claude não encontrada")?;
    let file = usage_file(config);
    let updated = std::fs::metadata(&file).ok().and_then(|m| m.modified().ok()).and_then(|t| t.duration_since(UNIX_EPOCH).ok()).map(|d| d.as_secs());
    let windows = std::fs::read_to_string(&file).ok().and_then(|t| serde_json::from_str::<Value>(&t).ok()).map(|j| windows(&j)).unwrap_or_default();
    Ok(Usage { installed: is_ours(&settings), windows, updated, settings_path: path.to_string_lossy().into_owned() })
}

fn path_for(state: &State<'_, PtyState>, id: u32) -> Result<PathBuf, String> {
    settings_path(crate::pty::foreground_pid(state, id)).ok_or_else(|| "pasta do Claude não encontrada".into())
}

#[tauri::command]
pub async fn claude_usage(state: State<'_, PtyState>, id: u32) -> Result<Usage, String> {
    let path = path_for(&state, id)?;
    tauri::async_runtime::spawn_blocking(move || read(&path)).await.map_err(|e| e.to_string())?
}

/// Liga (`enabled`) ou desfaz a captura do uso no settings.json do Claude da aba.
#[tauri::command]
pub async fn claude_usage_setup(state: State<'_, PtyState>, id: u32, enabled: bool) -> Result<Usage, String> {
    let path = path_for(&state, id)?;
    tauri::async_runtime::spawn_blocking(move || {
        if enabled {
            install(&path)?;
        } else {
            remove(&path)?;
        }
        read(&path)
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
        let w = windows(&j);
        assert_eq!(w.iter().map(|w| w.key.as_str()).collect::<Vec<_>>(), ["five_hour", "seven_day", "seven_day_fable"]);
        assert_eq!(w[0].used, 23.0);
        assert_eq!(slug(Path::new("/home/u/.claude-code7/")), "-home-u--claude-code7");
    }

    #[test]
    fn o_shell_grava_o_arquivo_e_repassa() {
        let dir = std::env::temp_dir().join(format!("term7-usage-sh-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let out = std::process::Command::new("sh")
            .arg("-c")
            .arg(wrapper(Some("cat; echo ' ok'")))
            .env("XDG_RUNTIME_DIR", &dir)
            .env("CLAUDE_CONFIG_DIR", "/x/.claude-code7/")
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .spawn()
            .and_then(|mut c| {
                use std::io::Write;
                c.stdin.take().unwrap().write_all(br#"{"a":1}"#)?;
                c.wait_with_output()
            })
            .unwrap();
        assert_eq!(String::from_utf8_lossy(&out.stdout), "{\"a\":1} ok\n");
        let saved = std::fs::read_to_string(dir.join("term7").join("usage--x--claude-code7.json")).unwrap();
        assert_eq!(saved, r#"{"a":1}"#);
        // Mesmo nome que o Rust calcula.
        assert_eq!(usage_file(Path::new("/x/.claude-code7")).file_name(), Some(std::ffi::OsStr::new("usage--x--claude-code7.json")));
    }
}
