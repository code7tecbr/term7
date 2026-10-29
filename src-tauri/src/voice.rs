//! Ditado: grava o microfone com `pw-record` (ou `arecord`) num .wav e
//! transcreve com uma API compatível com a da OpenAI (Groq, OpenAI...) ou com o
//! whisper.cpp local. O texto volta pro frontend, que cola no terminal.

use std::{
    io::Write,
    path::PathBuf,
    process::{Child, Command, Stdio},
    sync::Mutex,
};

use tauri::{AppHandle, State};

use crate::settings::{self, VoiceEngine, VoiceSettings};

#[derive(Default)]
pub struct VoiceState(Mutex<Option<(Child, PathBuf)>>);

fn wav_path() -> PathBuf {
    std::env::temp_dir().join(format!("term7-voz-{}.wav", std::process::id()))
}

/// 16 kHz mono: é o que o Whisper usa, e deixa o arquivo pequeno pro upload.
fn spawn_recorder(path: &PathBuf) -> Result<Child, String> {
    let pw = Command::new("pw-record")
        .args(["--rate", "16000", "--channels", "1", "--format", "s16"])
        .arg(path)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();
    if let Ok(child) = pw {
        return Ok(child);
    }
    Command::new("arecord")
        .args(["-q", "-f", "S16_LE", "-r", "16000", "-c", "1"])
        .arg(path)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| "não achei o pw-record nem o arecord pra gravar o microfone".to_string())
}

/// SIGINT faz o gravador fechar o .wav direito (com SIGKILL o cabeçalho fica errado).
fn stop_recorder(mut child: Child) {
    let _ = Command::new("kill").args(["-INT", &child.id().to_string()]).status();
    let _ = child.wait();
}

#[tauri::command]
pub fn voice_start(state: State<'_, VoiceState>) -> Result<(), String> {
    let mut rec = state.0.lock().unwrap();
    if let Some((child, path)) = rec.take() {
        stop_recorder(child);
        let _ = std::fs::remove_file(path);
    }
    let path = wav_path();
    let _ = std::fs::remove_file(&path);
    *rec = Some((spawn_recorder(&path)?, path));
    Ok(())
}

#[tauri::command]
pub fn voice_cancel(state: State<'_, VoiceState>) {
    if let Some((child, path)) = state.0.lock().unwrap().take() {
        stop_recorder(child);
        let _ = std::fs::remove_file(path);
    }
}

fn last_line(bytes: &[u8]) -> String {
    let text = String::from_utf8_lossy(bytes);
    text.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("").trim().chars().take(300).collect()
}

fn transcribe_api(v: &VoiceSettings, wav: &PathBuf) -> Result<String, String> {
    if v.api_key.is_empty() {
        return Err("falta a chave da API (Configurações → Voz)".into());
    }
    let url = format!("{}/audio/transcriptions", v.api_url.trim_end_matches('/'));
    if !url.starts_with("https://") && !url.starts_with("http://localhost") && !url.starts_with("http://127.0.0.1") {
        return Err("a URL da API precisa ser https".into());
    }
    let mut args = vec![
        "-sS".to_string(),
        "--max-time".into(),
        "90".into(),
        "-w".into(),
        "\n%{http_code}".into(),
        // A chave vai pelo stdin: na linha de comando ela apareceria no `ps`.
        "-H".into(),
        "@-".into(),
        "-F".into(),
        format!("file=@{}", wav.display()),
        "-F".into(),
        format!("model={}", v.api_model),
        "-F".into(),
        "response_format=text".into(),
    ];
    if !v.language.is_empty() {
        args.extend(["-F".into(), format!("language={}", v.language)]);
    }
    args.push(url);
    let mut child = Command::new("curl")
        .args(&args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|_| "não achei o curl".to_string())?;
    child
        .stdin
        .take()
        .unwrap()
        .write_all(format!("Authorization: Bearer {}\n", v.api_key).as_bytes())
        .map_err(|e| e.to_string())?;
    let out = child.wait_with_output().map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err(format!("falha na conexão: {}", last_line(&out.stderr)));
    }
    let body = String::from_utf8_lossy(&out.stdout);
    let (text, code) = body.rsplit_once('\n').unwrap_or(("", &body));
    if code.trim() != "200" {
        // Erro da API vem em JSON ({"error":{"message":...}}).
        let msg = serde_json::from_str::<serde_json::Value>(text)
            .ok()
            .and_then(|j| j.pointer("/error/message").and_then(|m| m.as_str()).map(str::to_string))
            .unwrap_or_else(|| last_line(text.as_bytes()));
        return Err(format!("a API respondeu {}: {msg}", code.trim()));
    }
    Ok(text.trim().to_string())
}

/// "~/x" -> "$HOME/x": o caminho vem digitado na configuração, sem shell pra expandir.
fn expand_home(p: &str) -> String {
    match (p.strip_prefix("~/"), std::env::var("HOME")) {
        (Some(rest), Ok(home)) => format!("{home}/{rest}"),
        _ => p.to_string(),
    }
}

fn transcribe_local(v: &VoiceSettings, wav: &PathBuf) -> Result<String, String> {
    if v.local_model.is_empty() {
        return Err("falta o modelo do whisper.cpp (Configurações → Voz)".into());
    }
    let bin = if v.local_bin.is_empty() { "whisper-cli".to_string() } else { expand_home(&v.local_bin) };
    let mut cmd = Command::new(bin);
    // -nt: sem marcação de tempo; -np: sem o log de progresso.
    cmd.args(["-nt", "-np", "-m", &expand_home(&v.local_model), "-f"]).arg(wav);
    if !v.language.is_empty() {
        cmd.args(["-l", &v.language]);
    }
    let out = cmd
        .stdin(Stdio::null())
        .output()
        .map_err(|_| format!("não achei o binário do whisper.cpp ({})", v.local_bin))?;
    if !out.status.success() {
        return Err(format!("o whisper.cpp falhou: {}", last_line(&out.stderr)));
    }
    let text = String::from_utf8_lossy(&out.stdout);
    Ok(text.split_whitespace().collect::<Vec<_>>().join(" "))
}

/// Para de gravar e devolve o texto.
#[tauri::command]
pub async fn voice_stop(app: AppHandle, state: State<'_, VoiceState>) -> Result<String, String> {
    let Some((child, wav)) = state.0.lock().unwrap().take() else { return Err("não estava gravando".into()) };
    let voice = settings::load(&app).voice;
    tauri::async_runtime::spawn_blocking(move || {
        stop_recorder(child);
        let result = match std::fs::metadata(&wav) {
            // Só o cabeçalho do .wav: o microfone não mandou nada.
            Ok(m) if m.len() > 1000 => match voice.engine {
                VoiceEngine::Api => transcribe_api(&voice, &wav),
                VoiceEngine::Local => transcribe_local(&voice, &wav),
            },
            _ => Err("nada gravado (o microfone está funcionando?)".into()),
        };
        let _ = std::fs::remove_file(&wav);
        result
    })
    .await
    .map_err(|e| e.to_string())?
}
