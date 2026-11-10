//! Configurações do term7 em <app_data_dir>/settings.json (permissão 600: guarda
//! a chave da API de transcrição). A chave nunca volta pro frontend: ele só
//! sabe se existe uma.

use std::{fs, os::unix::fs::PermissionsExt, path::PathBuf};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Default)]
#[serde(rename_all = "snake_case")]
pub enum VoiceEngine {
    #[default]
    Api,
    Local,
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct VoiceSettings {
    pub engine: VoiceEngine,
    /// Base de uma API compatível com a da OpenAI (`<url>/audio/transcriptions`).
    pub api_url: String,
    pub api_model: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub api_key: String,
    /// Binário do whisper.cpp e o modelo .bin.
    pub local_bin: String,
    pub local_model: String,
    /// Código ISO ("pt"); vazio = o serviço detecta.
    pub language: String,
}

impl Default for VoiceSettings {
    fn default() -> Self {
        Self {
            engine: VoiceEngine::Api,
            api_url: "https://api.groq.com/openai/v1".into(),
            api_model: "whisper-large-v3-turbo".into(),
            api_key: String::new(),
            local_bin: "whisper-cli".into(),
            local_model: String::new(),
            language: "pt".into(),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Default)]
#[serde(default)]
pub struct Settings {
    pub voice: VoiceSettings,
    /// API local pras extensões (socket em $XDG_RUNTIME_DIR/term7/api.sock).
    pub api_enabled: bool,
}

/// O que o frontend vê: tudo, menos a chave.
#[derive(Serialize)]
pub struct SettingsView {
    voice: VoiceSettings,
    has_api_key: bool,
    api_enabled: bool,
    /// Erro ao ligar a API (ex.: outra janela já está com ela).
    api_error: Option<String>,
}

fn path(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir.join("settings.json"))
}

pub fn load(app: &AppHandle) -> Settings {
    path(app).ok().and_then(|p| fs::read_to_string(p).ok()).and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
}

fn save(app: &AppHandle, settings: &Settings) -> Result<(), String> {
    let path = path(app)?;
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, serde_json::to_string_pretty(settings).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    // Antes do rename: o arquivo com a chave nunca fica legível pra outros usuários.
    fs::set_permissions(&tmp, fs::Permissions::from_mode(0o600)).map_err(|e| e.to_string())?;
    fs::rename(tmp, path).map_err(|e| e.to_string())
}

fn view(s: Settings) -> SettingsView {
    let has_api_key = !s.voice.api_key.is_empty();
    let mut voice = s.voice;
    voice.api_key.clear();
    SettingsView { voice, has_api_key, api_enabled: s.api_enabled, api_error: None }
}

#[tauri::command]
pub fn settings_load(app: AppHandle) -> SettingsView {
    let v = view(load(&app));
    let api_error = (v.api_enabled && !crate::api::is_running(&app))
        .then(|| "A API não ligou: outra janela do term7 está com ela?".to_string());
    SettingsView { api_error, ..v }
}

/// `api_key`: None mantém a salva, "" apaga, outro valor troca.
/// Liga ou desliga a API das extensões e guarda a escolha.
#[tauri::command]
pub fn settings_set_api(app: AppHandle, enabled: bool) -> Result<SettingsView, String> {
    let settings = Settings { api_enabled: enabled, ..load(&app) };
    save(&app, &settings)?;
    let error = if enabled { crate::api::start(&app).err() } else { crate::api::stop(&app); None };
    Ok(SettingsView { api_error: error, ..view(settings) })
}

#[tauri::command]
pub fn settings_save(app: AppHandle, mut voice: VoiceSettings, api_key: Option<String>) -> Result<SettingsView, String> {
    voice.api_key = match api_key {
        Some(k) => k.trim().to_string(),
        None => load(&app).voice.api_key,
    };
    let settings = Settings { voice, ..load(&app) };
    save(&app, &settings)?;
    Ok(view(settings))
}
