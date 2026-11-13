//! Tela de extensões: lista as que vêm no `.deb` (`<prefixo>/share/term7/extensions`,
//! uma pasta com `extension.json` cada) e instala pelo tipo:
//! - `service`: serviço do usuário no systemd, com a configuração num arquivo 600;
//! - `mcp`: `claude mcp add-json -s user` em cada conta do Claude escolhida;
//! - `tts`: voz local pra leitura em voz alta. Instalar grava a configuração e roda
//!   `<entry> --setup` (venv e modelo em `$XDG_DATA_HOME/term7/<id>`); a fala chama
//!   `<entry> OUT.wav` com o texto no stdin (ver speech.rs).
//! A extensão roda direto da pasta do pacote: atualizar o term7 atualiza ela.

use std::{
    collections::BTreeMap,
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tauri::AppHandle;

#[derive(Deserialize, Serialize, Clone, Debug, PartialEq)]
pub struct ConfigField {
    key: String,
    label: String,
    /// text | url | secret | number
    #[serde(default = "text")]
    r#type: String,
    #[serde(default)]
    required: bool,
    #[serde(default)]
    default: Option<String>,
    #[serde(default)]
    placeholder: Option<String>,
    #[serde(default)]
    help: Option<String>,
}

fn text() -> String {
    "text".into()
}

#[derive(Deserialize, Clone, Debug)]
struct Manifest {
    id: String,
    name: String,
    description: String,
    #[serde(default)]
    link: Option<String>,
    /// service | mcp | tts
    kind: String,
    entry: String,
    #[serde(default)]
    config: Vec<ConfigField>,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct ClaudeAccount {
    /// Nome da pasta (`.claude`, `.claude-code7`...): é o id na tela.
    id: String,
    label: String,
    /// MCP do term7 registrado nesta conta.
    installed: bool,
}

#[derive(Serialize)]
pub struct ExtensionView {
    id: String,
    name: String,
    description: String,
    link: Option<String>,
    kind: String,
    config: Vec<ConfigField>,
    /// Valores salvos, menos os secretos (que só dizem se existem, em `secrets_set`).
    values: BTreeMap<String, String>,
    secrets_set: Vec<String>,
    installed: bool,
    /// Serviço rodando agora (só `service`).
    running: bool,
    /// Contas do Claude (só `mcp`).
    accounts: Vec<ClaudeAccount>,
}

const MCP_NAME: &str = "term7";

fn home() -> Result<PathBuf, String> {
    std::env::var_os("HOME").map(PathBuf::from).ok_or_else(|| "HOME não definido".into())
}

fn config_home() -> Result<PathBuf, String> {
    Ok(std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from).unwrap_or(home()?.join(".config")))
}

/// Pasta das extensões do pacote: `/usr/bin/term7` -> `/usr/share/term7/extensions`
/// (vale também pro `~/.local` do install-local.sh).
fn extensions_dir() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let dir = exe.parent()?.parent()?.join("share/term7/extensions");
    dir.is_dir().then_some(dir)
}

fn load_manifests(dir: &Path) -> Vec<(Manifest, PathBuf)> {
    let mut found: Vec<_> = fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let path = e.path();
            let manifest: Manifest = serde_json::from_str(&fs::read_to_string(path.join("extension.json")).ok()?).ok()?;
            Some((manifest, path))
        })
        .collect();
    found.sort_by(|a, b| a.0.name.cmp(&b.0.name));
    found
}

fn find(id: &str) -> Result<(Manifest, PathBuf), String> {
    let dir = extensions_dir().ok_or("as extensões não vieram com este term7")?;
    load_manifests(&dir).into_iter().find(|(m, _)| m.id == id).ok_or_else(|| format!("extensão {id} não encontrada"))
}

// ---- configuração (arquivo de ambiente do systemd) ----

fn env_path(id: &str) -> Result<PathBuf, String> {
    Ok(config_home()?.join("term7").join(format!("{id}.env")))
}

/// Lê `CHAVE="valor"` (o formato que `render_env` grava).
fn parse_env(text: &str) -> BTreeMap<String, String> {
    text.lines()
        .filter_map(|l| {
            let (k, v) = l.trim().split_once('=')?;
            let v = v.strip_prefix('"').and_then(|v| v.strip_suffix('"')).map_or(v.to_string(), |v| {
                v.replace("\\\"", "\"").replace("\\\\", "\\")
            });
            (!k.starts_with('#')).then(|| (k.to_string(), v))
        })
        .collect()
}

fn render_env(values: &BTreeMap<String, String>) -> String {
    values
        .iter()
        .map(|(k, v)| format!("{k}=\"{}\"\n", v.replace('\\', "\\\\").replace('"', "\\\"")))
        .collect()
}

/// Junta o que veio da tela com o salvo. Secreto ausente mantém o salvo; vazio apaga.
fn merge_config(
    fields: &[ConfigField],
    saved: &BTreeMap<String, String>,
    given: &BTreeMap<String, Option<String>>,
) -> Result<BTreeMap<String, String>, String> {
    let mut out = BTreeMap::new();
    for f in fields {
        let value = match given.get(&f.key) {
            Some(Some(v)) => v.trim().to_string(),
            _ if f.r#type == "secret" => saved.get(&f.key).cloned().unwrap_or_default(),
            _ => f.default.clone().unwrap_or_default(),
        };
        if value.contains(['\n', '\r', '\0']) {
            return Err(format!("{}: não pode ter quebra de linha", f.label));
        }
        if f.required && value.is_empty() {
            return Err(format!("{}: obrigatório", f.label));
        }
        if f.r#type == "url" && !value.is_empty() && !(value.starts_with("http://") || value.starts_with("https://")) {
            return Err(format!("{}: precisa começar com http:// ou https://", f.label));
        }
        if f.r#type == "number" && !value.is_empty() && value.parse::<u32>().is_err() {
            return Err(format!("{}: precisa ser um número", f.label));
        }
        if !value.is_empty() {
            out.insert(f.key.clone(), value);
        }
    }
    Ok(out)
}

fn write_private(path: &Path, content: &str) -> Result<(), String> {
    fs::create_dir_all(path.parent().unwrap()).map_err(|e| e.to_string())?;
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, content).map_err(|e| e.to_string())?;
    // Antes do rename: o token nunca fica legível pra outros usuários.
    fs::set_permissions(&tmp, fs::Permissions::from_mode(0o600)).map_err(|e| e.to_string())?;
    fs::rename(tmp, path).map_err(|e| e.to_string())
}

// ---- tipo service: systemd do usuário ----

fn unit_name(id: &str) -> String {
    format!("term7-{id}.service")
}

fn unit_path(id: &str) -> Result<PathBuf, String> {
    Ok(config_home()?.join("systemd/user").join(unit_name(id)))
}

/// As extensões são Python; chamar o interpretador evita depender do bit de execução no pacote.
fn python3() -> Option<PathBuf> {
    std::env::var_os("PATH")
        .into_iter()
        .flat_map(|p| std::env::split_paths(&p).collect::<Vec<_>>())
        .chain([PathBuf::from("/usr/bin")])
        .map(|d| d.join("python3"))
        .find(|p| p.is_file())
}

fn render_unit(m: &Manifest, python: &Path, script: &Path, env: &Path) -> String {
    format!(
        "[Unit]\nDescription=term7: {}\n\n[Service]\nEnvironmentFile={}\nExecStart={} {}\nRestart=on-failure\nRestartSec=30\n\n[Install]\nWantedBy=default.target\n",
        m.name,
        env.display(),
        python.display(),
        script.display()
    )
}

fn systemctl(args: &[&str]) -> Result<String, String> {
    let out = Command::new("systemctl").arg("--user").args(args).stdin(Stdio::null()).output().map_err(|e| e.to_string())?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    } else {
        Err(format!("systemctl {}: {}", args.join(" "), String::from_utf8_lossy(&out.stderr).trim()))
    }
}

fn service_running(id: &str) -> bool {
    systemctl(&["is-active", "--quiet", &unit_name(id)]).is_ok()
}

// ---- tipo tts: voz local ----

fn data_home() -> Result<PathBuf, String> {
    Ok(std::env::var_os("XDG_DATA_HOME").map(PathBuf::from).unwrap_or(home()?.join(".local/share")))
}

/// Venv e modelo da extensão; `ready` dentro dela marca que o `--setup` terminou.
fn tts_dir(id: &str) -> Result<PathBuf, String> {
    Ok(data_home()?.join("term7").join(id))
}

fn tts_ready(id: &str) -> Result<bool, String> {
    Ok(tts_dir(id)?.join("ready").is_file())
}

/// Roda o `--setup` (cria o venv e baixa o modelo; pode levar minutos).
fn tts_setup(python: &Path, script: &Path) -> Result<(), String> {
    let out = Command::new(python).arg(script).arg("--setup").stdin(Stdio::null()).output().map_err(|e| e.to_string())?;
    if out.status.success() {
        return Ok(());
    }
    let err = String::from_utf8_lossy(&out.stderr);
    let last = err.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("").trim();
    Err(format!("a instalação falhou: {last}"))
}

/// Comando de uma extensão de voz instalada: (python3, script). Pra speech.rs.
pub fn tts_command(id: &str) -> Result<(PathBuf, PathBuf), String> {
    let (m, dir) = find(id)?;
    if m.kind != "tts" {
        return Err(format!("{} não é uma extensão de voz", m.name));
    }
    if !tts_ready(id)? {
        return Err(format!("instale a extensão “{}” (Configurações → Extensões)", m.name));
    }
    Ok((python3().ok_or("precisa do python3 instalado")?, dir.join(m.entry)))
}

/// Extensões de voz instaladas, pra escolher na leitura em voz alta: (id, nome).
pub fn installed_tts() -> Vec<(String, String)> {
    let Some(dir) = extensions_dir() else { return Vec::new() };
    load_manifests(&dir)
        .into_iter()
        .filter(|(m, _)| m.kind == "tts" && tts_ready(&m.id).unwrap_or(false))
        .map(|(m, _)| (m.id, m.name))
        .collect()
}

// ---- tipo mcp: contas do Claude ----

/// Contas do Claude: a padrão (`~/.claude.json`) e cada `~/.claude-*` com um
/// `.claude.json` dentro (usadas com CLAUDE_CONFIG_DIR).
fn claude_accounts(home: &Path) -> Vec<(ClaudeAccount, Option<PathBuf>)> {
    let installed = |file: &Path| {
        fs::read_to_string(file)
            .ok()
            .and_then(|t| serde_json::from_str::<Value>(&t).ok())
            .is_some_and(|c| c.pointer(&format!("/mcpServers/{MCP_NAME}")).is_some())
    };
    let mut out = Vec::new();
    if home.join(".claude.json").is_file() {
        let account = ClaudeAccount { id: ".claude".into(), label: "~/.claude (padrão)".into(), installed: installed(&home.join(".claude.json")) };
        out.push((account, None));
    }
    let mut dirs: Vec<_> = fs::read_dir(home)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
            name.starts_with(".claude-") && p.is_dir() && p.join(".claude.json").is_file()
        })
        .collect();
    dirs.sort();
    for dir in dirs {
        let name = dir.file_name().unwrap().to_string_lossy().into_owned();
        let account = ClaudeAccount { id: name.clone(), label: format!("~/{name}"), installed: installed(&dir.join(".claude.json")) };
        out.push((account, Some(dir)));
    }
    out
}

/// Binário do Claude: o app aberto pelo menu nem sempre tem o ~/.local/bin no PATH.
fn claude_bin(home: &Path) -> Option<PathBuf> {
    let from_path = std::env::var_os("PATH").into_iter().flat_map(|p| std::env::split_paths(&p).collect::<Vec<_>>());
    from_path
        .map(|d| d.join("claude"))
        .chain([home.join(".local/bin/claude"), home.join(".claude/local/claude"), PathBuf::from("/usr/local/bin/claude")])
        .find(|p| p.is_file())
}

fn claude_mcp(bin: &Path, config_dir: Option<&Path>, args: &[&str]) -> Result<(), String> {
    let mut cmd = Command::new(bin);
    cmd.args(["mcp"]).args(args).stdin(Stdio::null());
    match config_dir {
        Some(dir) => cmd.env("CLAUDE_CONFIG_DIR", dir),
        None => cmd.env_remove("CLAUDE_CONFIG_DIR"),
    };
    let out = cmd.output().map_err(|e| e.to_string())?;
    if out.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
}

// ---- comandos ----

fn view(m: Manifest) -> Result<ExtensionView, String> {
    let saved = fs::read_to_string(env_path(&m.id)?).map(|t| parse_env(&t)).unwrap_or_default();
    let secret = |k: &String| m.config.iter().any(|f| &f.key == k && f.r#type == "secret");
    let values = saved.iter().filter(|(k, _)| !secret(k)).map(|(k, v)| (k.clone(), v.clone())).collect();
    let secrets_set = saved.keys().filter(|k| secret(k)).cloned().collect();
    let (installed, running, accounts) = match m.kind.as_str() {
        "service" => (unit_path(&m.id)?.is_file(), service_running(&m.id), Vec::new()),
        "tts" => (tts_ready(&m.id)?, false, Vec::new()),
        _ => {
            let accounts: Vec<_> = claude_accounts(&home()?).into_iter().map(|(a, _)| a).collect();
            (accounts.iter().any(|a| a.installed), false, accounts)
        }
    };
    Ok(ExtensionView {
        id: m.id,
        name: m.name,
        description: m.description,
        link: m.link,
        kind: m.kind,
        config: m.config,
        values,
        secrets_set,
        installed,
        running,
        accounts,
    })
}

fn list() -> Result<Vec<ExtensionView>, String> {
    let Some(dir) = extensions_dir() else { return Ok(Vec::new()) };
    load_manifests(&dir).into_iter().map(|(m, _)| view(m)).collect()
}

fn install(id: &str, config: BTreeMap<String, Option<String>>, accounts: Vec<String>) -> Result<ExtensionView, String> {
    let (m, dir) = find(id)?;
    let script = dir.join(&m.entry);
    let python = python3().ok_or("precisa do python3 instalado")?;
    match m.kind.as_str() {
        "service" => {
            let env = env_path(id)?;
            let saved = fs::read_to_string(&env).map(|t| parse_env(&t)).unwrap_or_default();
            write_private(&env, &render_env(&merge_config(&m.config, &saved, &config)?))?;
            let unit = unit_path(id)?;
            fs::create_dir_all(unit.parent().unwrap()).map_err(|e| e.to_string())?;
            fs::write(&unit, render_unit(&m, &python, &script, &env)).map_err(|e| e.to_string())?;
            systemctl(&["daemon-reload"])?;
            systemctl(&["enable", &unit_name(id)])?;
            // restart também sobe se estava parado, e pega a configuração nova.
            systemctl(&["restart", &unit_name(id)])?;
        }
        "tts" => {
            let env = env_path(id)?;
            let saved = fs::read_to_string(&env).map(|t| parse_env(&t)).unwrap_or_default();
            write_private(&env, &render_env(&merge_config(&m.config, &saved, &config)?))?;
            // Já instalada: salvar só troca a configuração, sem baixar de novo.
            if !tts_ready(id)? {
                tts_setup(&python, &script)?;
            }
        }
        "mcp" => {
            let home = home()?;
            let bin = claude_bin(&home).ok_or("não achei o comando claude (Claude Code)")?;
            let spec = json!({ "type": "stdio", "command": python, "args": [script] }).to_string();
            for (account, config_dir) in claude_accounts(&home) {
                let wanted = accounts.contains(&account.id);
                if account.installed {
                    // Reinstalar troca o registro (o caminho pode ter mudado).
                    claude_mcp(&bin, config_dir.as_deref(), &["remove", "-s", "user", MCP_NAME])
                        .map_err(|e| format!("{}: {e}", account.label))?;
                }
                if wanted {
                    claude_mcp(&bin, config_dir.as_deref(), &["add-json", "-s", "user", MCP_NAME, &spec])
                        .map_err(|e| format!("{}: {e}", account.label))?;
                }
            }
        }
        other => return Err(format!("tipo de extensão desconhecido: {other}")),
    }
    view(m)
}

fn uninstall(id: &str) -> Result<ExtensionView, String> {
    let (m, _) = find(id)?;
    match m.kind.as_str() {
        "service" => {
            let _ = systemctl(&["disable", "--now", &unit_name(id)]);
            let _ = fs::remove_file(unit_path(id)?);
            let _ = fs::remove_file(env_path(id)?);
            let _ = systemctl(&["daemon-reload"]);
        }
        "tts" => {
            let _ = fs::remove_dir_all(tts_dir(id)?);
            let _ = fs::remove_file(env_path(id)?);
        }
        _ => return install(id, BTreeMap::new(), Vec::new()),
    }
    view(m)
}

async fn blocking<T: Send + 'static>(f: impl FnOnce() -> Result<T, String> + Send + 'static) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(f).await.map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn extensions_list() -> Result<Vec<ExtensionView>, String> {
    blocking(list).await
}

/// Instala ou atualiza (configuração/contas). `config`: secreto `null` mantém o salvo.
/// Liga a API: sem ela as extensões que falam com o term7 não funcionam (a de voz
/// é chamada pelo próprio term7 e não precisa).
#[tauri::command]
pub async fn extension_install(
    app: AppHandle,
    id: String,
    config: BTreeMap<String, Option<String>>,
    accounts: Vec<String>,
) -> Result<ExtensionView, String> {
    let lookup = id.clone();
    if blocking(move || find(&lookup)).await?.0.kind != "tts" {
        crate::settings::enable_api(&app)?;
    }
    blocking(move || install(&id, config, accounts)).await
}

/// Extensões de voz instaladas (id e nome), pras configurações da leitura em voz alta.
#[tauri::command]
pub async fn extensions_tts() -> Result<Vec<(String, String)>, String> {
    blocking(|| Ok(installed_tts())).await
}

#[tauri::command]
pub async fn extension_uninstall(id: String) -> Result<ExtensionView, String> {
    blocking(move || uninstall(&id)).await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("term7-ext-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn field(key: &str, ty: &str) -> ConfigField {
        ConfigField { key: key.into(), label: key.into(), r#type: ty.into(), required: false, default: None, placeholder: None, help: None }
    }

    #[test]
    fn le_os_manifestos_do_repositorio() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../extensions");
        let found: Vec<_> = load_manifests(&dir).into_iter().map(|(m, p)| (m.id, m.kind, p.join(m.entry).is_file())).collect();
        assert_eq!(
            found,
            [
                ("mcp".into(), "mcp".into(), true),
                ("kokoro".into(), "tts".into(), true),
                ("statusbot".into(), "service".into(), true)
            ]
        );
    }

    #[test]
    fn arquivo_de_ambiente_ida_e_volta() {
        let values = BTreeMap::from([("A".to_string(), "http://x:8080".to_string()), ("B".to_string(), r#"com "aspas" e \"#.to_string())]);
        assert_eq!(parse_env(&render_env(&values)), values);
    }

    #[test]
    fn configuracao_mantem_o_secreto_e_valida() {
        let mut url = field("URL", "url");
        url.required = true;
        let mut every = field("EVERY", "number");
        every.default = Some("60".into());
        let fields = [url, field("TOKEN", "secret"), every];
        let saved = BTreeMap::from([("TOKEN".to_string(), "segredo".to_string())]);

        let given = BTreeMap::from([("URL".to_string(), Some(" http://localhost:8080 ".to_string())), ("TOKEN".to_string(), None)]);
        let out = merge_config(&fields, &saved, &given).unwrap();
        assert_eq!(out["URL"], "http://localhost:8080");
        assert_eq!(out["TOKEN"], "segredo");
        assert_eq!(out["EVERY"], "60");

        // Token vazio apaga.
        let given = BTreeMap::from([("URL".to_string(), Some("http://x".to_string())), ("TOKEN".to_string(), Some(String::new()))]);
        assert!(!merge_config(&fields, &saved, &given).unwrap().contains_key("TOKEN"));

        let bad = |k: &str, v: &str| {
            let mut g = BTreeMap::from([("URL".to_string(), Some("http://x".to_string()))]);
            g.insert(k.into(), Some(v.into()));
            merge_config(&fields, &saved, &g).unwrap_err()
        };
        assert!(bad("URL", "").contains("obrigatório"));
        assert!(bad("URL", "ftp://x").contains("http"));
        assert!(bad("EVERY", "rápido").contains("número"));
        assert!(bad("TOKEN", "a\nB=1").contains("quebra de linha"));
    }

    #[test]
    fn acha_as_contas_do_claude() {
        let home = tmp("contas");
        fs::write(home.join(".claude.json"), r#"{"mcpServers":{"term7":{"type":"stdio"}}}"#).unwrap();
        fs::create_dir_all(home.join(".claude-code7")).unwrap();
        fs::write(home.join(".claude-code7/.claude.json"), r#"{"mcpServers":{}}"#).unwrap();
        // Sem .claude.json não é conta; arquivo solto também não.
        fs::create_dir_all(home.join(".claude-old")).unwrap();
        fs::write(home.join(".claude.json.backup"), "{}").unwrap();

        let got: Vec<_> = claude_accounts(&home).into_iter().map(|(a, dir)| (a.id, a.installed, dir.is_some())).collect();
        assert_eq!(got, [(".claude".into(), true, false), (".claude-code7".into(), false, true)]);
        fs::remove_dir_all(home).unwrap();
    }

    #[test]
    fn unidade_do_systemd() {
        let m = Manifest {
            id: "statusbot".into(),
            name: "statusbot".into(),
            description: String::new(),
            link: None,
            kind: "service".into(),
            entry: "term7-statusbot".into(),
            config: Vec::new(),
        };
        let script = Path::new("/usr/share/term7/extensions/statusbot/term7-statusbot");
        let unit = render_unit(&m, Path::new("/usr/bin/python3"), script, Path::new("/h/.config/term7/statusbot.env"));
        assert!(unit.contains("ExecStart=/usr/bin/python3 /usr/share/term7/extensions/statusbot/term7-statusbot\n"));
        assert!(unit.contains("EnvironmentFile=/h/.config/term7/statusbot.env\n"));
        assert!(unit.contains("WantedBy=default.target"));
    }
}
