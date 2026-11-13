//! Ditado: grava o microfone com `pw-record` (ou `arecord`) num .wav e
//! transcreve com uma API compatível com a da OpenAI (Groq, OpenAI...) ou com o
//! whisper.cpp local. O texto volta pro frontend, que cola no terminal.

use std::{
    io::{Read, Seek, SeekFrom, Write},
    path::PathBuf,
    process::{Child, Command, Stdio},
    sync::{
        atomic::{AtomicU64, Ordering},
        Mutex,
    },
    time::Duration,
};

use tauri::{AppHandle, Emitter, Manager, State};

use crate::settings::{self, VoiceEngine, VoiceSettings};

#[derive(Default)]
pub struct VoiceState {
    recording: Mutex<Option<(Child, PathBuf)>>,
    /// Cada gravação nova avança: o vigia de silêncio da anterior vê e para.
    generation: AtomicU64,
}

/// 100 ms de áudio a 16 kHz.
const WINDOW_SAMPLES: usize = 1600;
/// Silêncio depois da fala que encerra a gravação.
const QUIET_AFTER_SPEECH_MS: u32 = 1500;
/// Sem fala nenhuma nesse tempo, desiste (o Whisper inventa texto em cima de silêncio).
const NO_SPEECH_MS: u32 = 8000;

/// Decide, janela a janela, quando a fala acabou. O piso de ruído acompanha o
/// ambiente (ventilador, teclado) pra o limiar não ser fixo.
pub(crate) struct SilenceDetector {
    noise: Option<f64>,
    spoke: bool,
    quiet_ms: u32,
    elapsed_ms: u32,
}

impl SilenceDetector {
    pub(crate) fn new() -> Self {
        Self { noise: None, spoke: false, quiet_ms: 0, elapsed_ms: 0 }
    }

    /// RMS de uma janela de 100 ms. `Some(falou)` quando é hora de parar.
    pub(crate) fn push(&mut self, rms: f64) -> Option<bool> {
        self.elapsed_ms += 100;
        let noise = match self.noise {
            // Desce na hora, sobe devagar: a fala não vira "ruído".
            Some(n) if rms < n => rms,
            Some(n) => n * 0.99 + rms * 0.01,
            None => rms,
        };
        self.noise = Some(noise);
        if rms > (noise * 4.0).max(400.0) {
            self.spoke = true;
            self.quiet_ms = 0;
        } else if rms < (noise * 2.5).max(250.0) {
            self.quiet_ms += 100;
        } else {
            self.quiet_ms = 0;
        }
        if self.spoke && self.quiet_ms >= QUIET_AFTER_SPEECH_MS {
            return Some(true);
        }
        (!self.spoke && self.elapsed_ms >= NO_SPEECH_MS).then_some(false)
    }
}

fn rms(samples: &[u8]) -> f64 {
    let n = samples.len() / 2;
    if n == 0 {
        return 0.0;
    }
    let sum: f64 = samples.chunks_exact(2).map(|b| f64::from(i16::from_le_bytes([b[0], b[1]])).powi(2)).sum();
    (sum / n as f64).sqrt()
}

/// Lê o .wav enquanto ele cresce e avisa o frontend ("voice-auto-stop", com
/// `true` se houve fala) quando é hora de parar. O frontend decide o que fazer.
fn watch_silence(app: AppHandle, path: PathBuf, generation: u64) {
    std::thread::spawn(move || {
        let state = app.state::<VoiceState>();
        let mut detector = SilenceDetector::new();
        // Pula o cabeçalho do .wav (44 bytes no pw-record/arecord; par, então as amostras ficam alinhadas).
        let mut offset = 44u64;
        let mut pending: Vec<u8> = Vec::new();
        loop {
            std::thread::sleep(Duration::from_millis(100));
            if state.generation.load(Ordering::SeqCst) != generation {
                return;
            }
            let Ok(mut file) = std::fs::File::open(&path) else { continue };
            if file.seek(SeekFrom::Start(offset)).is_err() {
                continue;
            }
            let mut buf = Vec::new();
            if file.read_to_end(&mut buf).is_err() {
                continue;
            }
            offset += buf.len() as u64;
            pending.extend_from_slice(&buf);
            let window = WINDOW_SAMPLES * 2;
            while pending.len() >= window {
                let decision = detector.push(rms(&pending[..window]));
                pending.drain(..window);
                if let Some(spoke) = decision {
                    if state.generation.load(Ordering::SeqCst) == generation {
                        let _ = app.emit("voice-auto-stop", spoke);
                    }
                    return;
                }
            }
        }
    });
}

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
pub fn voice_start(app: AppHandle, state: State<'_, VoiceState>) -> Result<(), String> {
    let generation = state.generation.fetch_add(1, Ordering::SeqCst) + 1;
    let mut rec = state.recording.lock().unwrap();
    if let Some((child, path)) = rec.take() {
        stop_recorder(child);
        let _ = std::fs::remove_file(path);
    }
    let path = wav_path();
    let _ = std::fs::remove_file(&path);
    *rec = Some((spawn_recorder(&path)?, path.clone()));
    if settings::load(&app).voice.auto_stop {
        watch_silence(app.clone(), path, generation);
    }
    Ok(())
}

#[tauri::command]
pub fn voice_cancel(state: State<'_, VoiceState>) {
    state.generation.fetch_add(1, Ordering::SeqCst);
    if let Some((child, path)) = state.recording.lock().unwrap().take() {
        stop_recorder(child);
        let _ = std::fs::remove_file(path);
    }
}

pub(crate) fn last_line(bytes: &[u8]) -> String {
    let text = String::from_utf8_lossy(bytes);
    text.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("").trim().chars().take(300).collect()
}

/// `<base>/<path>`, só https (ou um servidor na própria máquina): a chave vai junto.
pub(crate) fn api_url(base: &str, path: &str) -> Result<String, String> {
    let url = format!("{}/{path}", base.trim_end_matches('/'));
    if !url.starts_with("https://") && !url.starts_with("http://localhost") && !url.starts_with("http://127.0.0.1") {
        return Err("a URL da API precisa ser https".into());
    }
    Ok(url)
}

/// Mensagem do erro da API ({"error":{"message":...}}), ou a última linha da resposta.
pub(crate) fn api_error(body: &[u8]) -> String {
    serde_json::from_slice::<serde_json::Value>(body)
        .ok()
        .and_then(|j| j.pointer("/error/message").and_then(|m| m.as_str()).map(str::to_string))
        .unwrap_or_else(|| last_line(body))
}

fn transcribe_api(v: &VoiceSettings, wav: &PathBuf) -> Result<String, String> {
    if v.api_key.is_empty() {
        return Err("falta a chave da API (Configurações → Voz)".into());
    }
    let url = api_url(&v.api_url, "audio/transcriptions")?;
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
        return Err(format!("a API respondeu {}: {}", code.trim(), api_error(text.as_bytes())));
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
    state.generation.fetch_add(1, Ordering::SeqCst);
    let Some((child, wav)) = state.recording.lock().unwrap().take() else { return Err("não estava gravando".into()) };
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

#[cfg(test)]
mod tests {
    use super::*;

    fn feed(detector: &mut SilenceDetector, rms: f64, windows: usize) -> Option<bool> {
        (0..windows).find_map(|_| detector.push(rms))
    }

    #[test]
    fn para_depois_de_falar_e_calar() {
        let mut d = SilenceDetector::new();
        assert_eq!(feed(&mut d, 80.0, 5), None); // ambiente
        assert_eq!(feed(&mut d, 3000.0, 20), None); // fala
        assert_eq!(feed(&mut d, 90.0, 14), None); // pausa curta entre frases não para
        assert_eq!(feed(&mut d, 2500.0, 5), None);
        assert_eq!(feed(&mut d, 90.0, 15), Some(true));
    }

    #[test]
    fn desiste_sem_fala() {
        let mut d = SilenceDetector::new();
        assert_eq!(feed(&mut d, 120.0, 79), None);
        assert_eq!(d.push(120.0), Some(false));
    }

    #[test]
    fn ruido_alto_constante_nao_conta_como_fala() {
        // Ventilador forte: o piso sobe junto e o limiar acompanha.
        let mut d = SilenceDetector::new();
        assert_eq!(feed(&mut d, 600.0, 79), None);
        assert_eq!(d.push(600.0), Some(false));
    }

    #[test]
    fn rms_das_amostras() {
        let bytes: Vec<u8> = [1000i16, -1000, 1000, -1000].iter().flat_map(|s| s.to_le_bytes()).collect();
        assert_eq!(rms(&bytes), 1000.0);
        assert_eq!(rms(&[]), 0.0);
    }
}
