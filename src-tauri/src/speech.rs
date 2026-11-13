//! Saída de voz: lê em voz alta a última resposta do Claude da aba (ou um texto
//! de teste) com a API `/audio/speech` no formato da OpenAI (Groq, OpenAI...) ou
//! com uma extensão de voz local (tipo `tts`, como a do Kokoro).
//! O áudio vem em trechos curtos: o próximo é gerado enquanto o atual toca.

use std::{
    io::Write,
    os::unix::fs::OpenOptionsExt,
    path::PathBuf,
    process::{Command, Stdio},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
    thread::{self, JoinHandle},
    time::Duration,
};

use tauri::{AppHandle, Manager, State};

use crate::{
    pty::PtyState,
    settings::{self, SpeechEngine, SpeechSettings, VoiceSettings},
    suggest, voice,
};

/// Os modelos Orpheus do Groq recusam pedidos com texto muito maior que isso.
const CHUNK_CHARS: usize = 200;
/// Nada toca até o primeiro trecho chegar inteiro: curto, a fala começa logo e
/// os seguintes são gerados enquanto ele toca.
const FIRST_CHUNK_CHARS: usize = 80;
/// Resposta longa vira minutos de áudio, custo e cota (o Groq gratuito dá 100
/// pedidos por dia e 1.200 tokens por minuto): lê só o começo.
const MAX_CHARS: usize = 600;
/// Tentativas por trecho quando a API pede pra esperar (429) ou falha do lado dela (5xx).
const ATTEMPTS: usize = 3;
/// Espera máxima por tentativa: mais que isso, a leitura já perdeu o sentido.
const MAX_WAIT: Duration = Duration::from_secs(15);

#[derive(Default)]
pub struct SpeechState {
    /// Cada fala nova (ou parar) avança a geração; a que estava tocando vê e desiste.
    generation: AtomicU64,
    /// pid do pw-play/aplay tocando agora.
    player: Mutex<Option<u32>>,
}

/// "- item", "* item", "1. item", "2) item" -> "item".
fn strip_list_marker(line: &str) -> &str {
    for marker in ["- ", "* ", "+ "] {
        if let Some(rest) = line.strip_prefix(marker) {
            return rest;
        }
    }
    let digits = line.chars().take_while(|c| c.is_ascii_digit()).count();
    if digits > 0 {
        let rest = &line[digits..];
        if let Some(rest) = rest.strip_prefix(". ").or_else(|| rest.strip_prefix(") ")) {
            return rest;
        }
    }
    line
}

/// "[texto](url)" -> "texto".
fn strip_links(line: &str) -> String {
    let mut out = String::new();
    let mut rest = line;
    while let Some(open) = rest.find('[') {
        let after = &rest[open + 1..];
        if let Some((close, end)) = after.find("](").and_then(|c| after[c + 2..].find(')').map(|e| (c, c + 2 + e))) {
            out.push_str(&rest[..open]);
            out.push_str(&after[..close]);
            rest = &after[end + 1..];
        } else {
            out.push_str(&rest[..=open]);
            rest = after;
        }
    }
    out.push_str(rest);
    out
}

/// Caminho de arquivo vira só o nome sem extensão: a voz trava tentando
/// pronunciar "src/lib/voice.test.ts" ("isss..."). "src/a.rs:" -> "a:".
fn speakable_word(word: &str) -> String {
    let core = word.trim_end_matches(['.', ',', ';', ':', '!', '?', ')']);
    let tail = &word[core.len()..];
    let mut name = core.rsplit('/').find(|p| !p.is_empty()).unwrap_or(core);
    // Extensão: depois do último ponto, curta e com letra ("3.5" fica).
    while let Some((stem, ext)) = name.rsplit_once('.') {
        let is_ext = (1..=4).contains(&ext.len())
            && ext.chars().all(|c| c.is_ascii_alphanumeric())
            && ext.chars().any(|c| c.is_ascii_alphabetic());
        if !is_ext {
            break;
        }
        if stem.is_empty() {
            name = ext; // ".deb" -> "deb"
            break;
        }
        name = stem;
    }
    format!("{name}{tail}")
}

/// Markdown da resposta -> texto pra falar: sem blocos de código, tabelas, links
/// e marcação. Cada linha vira uma frase, pra voz pausar entre os itens.
pub(crate) fn speakable(markdown: &str) -> String {
    let mut sentences: Vec<String> = Vec::new();
    let mut in_code = false;
    for raw in markdown.lines() {
        let line = raw.trim();
        if line.starts_with("```") {
            in_code = !in_code;
            continue;
        }
        // Vazia, tabela ou linha separadora (---, ***).
        if in_code || line.starts_with('|') || line.chars().all(|c| matches!(c, '-' | '*' | '_' | '=')) {
            continue;
        }
        let line = strip_list_marker(line.trim_start_matches(['#', '>']).trim_start());
        let text = strip_links(line).replace(['`', '*'], "").replace("__", "");
        let mut sentence = text.split_whitespace().filter(|w| !w.contains("://")).map(speakable_word).collect::<Vec<_>>().join(" ");
        if sentence.is_empty() {
            continue;
        }
        if !sentence.ends_with(['.', '!', '?', ':', ';']) {
            sentence.push('.');
        }
        sentences.push(sentence);
    }
    sentences.join(" ")
}

/// Quebra uma frase em pedaços de até `max` caracteres, entre palavras.
fn fit(sentence: &str, max: usize) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    for word in sentence.split_whitespace() {
        let word: String = word.chars().take(max).collect();
        if !cur.is_empty() && cur.chars().count() + 1 + word.chars().count() > max {
            out.push(std::mem::take(&mut cur));
        }
        if !cur.is_empty() {
            cur.push(' ');
        }
        cur.push_str(&word);
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

/// Trechos de até `max` caracteres (o primeiro, até `first`), juntando frases
/// inteiras; para no fim da frase que passaria de `limit` no total.
pub(crate) fn chunks(text: &str, first: usize, max: usize, limit: usize) -> Vec<String> {
    let mut sentences: Vec<String> = Vec::new();
    let mut cur = String::new();
    for word in text.split_whitespace() {
        if !cur.is_empty() {
            cur.push(' ');
        }
        cur.push_str(word);
        if word.ends_with(['.', '!', '?']) {
            sentences.push(std::mem::take(&mut cur));
        }
    }
    if !cur.is_empty() {
        sentences.push(cur);
    }

    let mut out: Vec<String> = Vec::new();
    let mut total = 0;
    for sentence in sentences {
        let len = sentence.chars().count();
        if total + len > limit && !out.is_empty() {
            break;
        }
        total += len;
        for piece in fit(&sentence, max) {
            let n = piece.chars().count();
            let cap = if out.len() == 1 { first } else { max };
            match out.last_mut() {
                Some(last) if last.chars().count() + 1 + n <= cap => {
                    last.push(' ');
                    last.push_str(&piece);
                }
                _ => out.push(piece),
            }
        }
    }
    // Uma frase só já passou do primeiro trecho: corta entre palavras.
    if out.first().is_some_and(|head| head.chars().count() > first) {
        let pieces = fit(&out[0], first);
        out[0] = pieces[0].clone();
        out.insert(1, pieces[1..].join(" "));
    }
    out
}

struct Config {
    url: String,
    key: String,
    model: String,
    voice: String,
}

fn config(voice_cfg: &VoiceSettings, speech: &SpeechSettings) -> Result<Config, String> {
    let (base, key) = speech.endpoint(voice_cfg);
    if key.is_empty() {
        let which = if speech.same_api { "do ditado" } else { "de voz" };
        return Err(format!("falta a chave da API {which} (Configurações → Voz)"));
    }
    Ok(Config {
        url: voice::api_url(base, "audio/speech")?,
        key: key.to_string(),
        model: speech.api_model.clone(),
        voice: speech.voice.clone(),
    })
}

/// Quem gera o áudio de cada trecho.
enum Engine {
    Api(Config),
    /// Extensão de voz: `python3 <script> OUT.wav`, com o texto no stdin.
    Local { python: PathBuf, script: PathBuf },
}

fn engine(s: &settings::Settings) -> Result<Engine, String> {
    match s.speech.engine {
        SpeechEngine::Api => config(&s.voice, &s.speech).map(Engine::Api),
        SpeechEngine::Extension => {
            let (python, script) = crate::extensions::tts_command(&s.speech.extension)?;
            Ok(Engine::Local { python, script })
        }
    }
}

fn synthesize_local(python: &PathBuf, script: &PathBuf, text: &str, out: &PathBuf) -> Result<(), String> {
    let mut child = Command::new(python)
        .arg(script)
        .arg(out)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("não deu pra rodar a voz local: {e}"))?;
    child.stdin.take().unwrap().write_all(text.as_bytes()).map_err(|e| e.to_string())?;
    let res = child.wait_with_output().map_err(|e| e.to_string())?;
    if !res.status.success() {
        let _ = std::fs::remove_file(out);
        return Err(format!("a voz local falhou: {}", voice::last_line(&res.stderr)));
    }
    Ok(())
}

fn tmp_path(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("term7-fala-{}-{name}", std::process::id()))
}

/// Erro de um pedido; `retry` diz quanto esperar quando vale tentar de novo.
struct SynthError {
    msg: String,
    retry: Option<Duration>,
}

/// `Retry-After` dos cabeçalhos da resposta (segundos, às vezes fracionados).
fn retry_after(headers: &str) -> Option<Duration> {
    headers.lines().find_map(|line| {
        let (name, value) = line.split_once(':')?;
        if !name.trim().eq_ignore_ascii_case("retry-after") {
            return None;
        }
        value.trim().parse::<f64>().ok().filter(|s| s.is_finite() && *s >= 0.0).map(Duration::from_secs_f64)
    })
}

/// 429: espera o que a API pediu (2 s se não disse), a não ser que seja longo demais
/// (cota do dia acabou); 5xx: 1 s. O resto não adianta repetir.
fn retry_delay(code: &str, headers: &str) -> Option<Duration> {
    match code {
        "429" => Some(retry_after(headers).unwrap_or(Duration::from_secs(2))).filter(|wait| *wait <= MAX_WAIT),
        c if c.starts_with('5') => Some(Duration::from_secs(1)),
        _ => None,
    }
}

/// Gera o .wav de um trecho. O texto vai num arquivo (600) e a chave pelo
/// stdin: na linha de comando os dois apareceriam no `ps`.
fn synthesize(cfg: &Config, text: &str, out: &PathBuf) -> Result<(), SynthError> {
    let fatal = |msg: String| SynthError { msg, retry: None };
    let body = serde_json::json!({ "model": cfg.model, "voice": cfg.voice, "input": text, "response_format": "wav" });
    let body_path = out.with_extension("json");
    std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(&body_path)
        .and_then(|mut f| f.write_all(body.to_string().as_bytes()))
        .map_err(|e| fatal(e.to_string()))?;
    let headers_path = out.with_extension("headers");

    let result = (|| {
        let mut child = Command::new("curl")
            .args(["-sS", "--max-time", "60", "-w", "%{http_code}", "-H", "@-", "-H", "Content-Type: application/json"])
            .arg("--data-binary")
            .arg(format!("@{}", body_path.display()))
            .arg("-D")
            .arg(&headers_path)
            .arg("-o")
            .arg(out)
            .arg(&cfg.url)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|_| fatal("não achei o curl".into()))?;
        child
            .stdin
            .take()
            .unwrap()
            .write_all(format!("Authorization: Bearer {}\n", cfg.key).as_bytes())
            .map_err(|e| fatal(e.to_string()))?;
        let res = child.wait_with_output().map_err(|e| fatal(e.to_string()))?;
        if !res.status.success() {
            return Err(fatal(format!("falha na conexão: {}", voice::last_line(&res.stderr))));
        }
        let code = String::from_utf8_lossy(&res.stdout).trim().to_string();
        if code != "200" {
            let msg = voice::api_error(&std::fs::read(out).unwrap_or_default());
            let _ = std::fs::remove_file(out);
            let headers = std::fs::read_to_string(&headers_path).unwrap_or_default();
            let retry = retry_delay(&code, &headers);
            let msg = if code == "429" && retry.is_none() {
                "a cota da API de voz acabou por agora (o Groq gratuito dá 100 pedidos por dia)".to_string()
            } else {
                format!("a API de voz respondeu {code}: {msg}")
            };
            return Err(SynthError { msg, retry });
        }
        Ok(())
    })();
    let _ = std::fs::remove_file(&body_path);
    let _ = std::fs::remove_file(&headers_path);
    result
}

/// `synthesize` com novas tentativas; desiste da espera se a fala for interrompida.
fn synthesize_retrying(app: &AppHandle, generation: u64, cfg: &Config, text: &str, out: &PathBuf) -> Result<(), String> {
    let current = || app.state::<SpeechState>().generation.load(Ordering::SeqCst) == generation;
    let mut attempt = 1;
    loop {
        match synthesize(cfg, text, out) {
            Ok(()) => return Ok(()),
            Err(SynthError { retry: Some(wait), .. }) if attempt < ATTEMPTS => {
                // Em fatias, pra parar na hora se a fala for interrompida.
                let mut left = wait;
                while !left.is_zero() {
                    if !current() {
                        return Err("interrompida".into());
                    }
                    let step = left.min(Duration::from_millis(250));
                    thread::sleep(step);
                    left -= step;
                }
                attempt += 1;
            }
            Err(e) => return Err(e.msg),
        }
    }
}

fn play(state: &SpeechState, wav: &PathBuf) -> Result<(), String> {
    let spawn = |bin: &str, args: &[&str]| {
        Command::new(bin).args(args).arg(wav).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn()
    };
    let mut child = spawn("pw-play", &[])
        .or_else(|_| spawn("aplay", &["-q"]))
        .map_err(|_| "não achei o pw-play nem o aplay pra tocar o áudio".to_string())?;
    *state.player.lock().unwrap() = Some(child.id());
    let _ = child.wait();
    *state.player.lock().unwrap() = None;
    Ok(())
}

fn stop_playing(state: &SpeechState) {
    state.generation.fetch_add(1, Ordering::SeqCst);
    if let Some(pid) = *state.player.lock().unwrap() {
        let _ = Command::new("kill").arg(pid.to_string()).status();
    }
}

type Job = JoinHandle<Result<PathBuf, String>>;

/// Trecho já pedido que não vai mais tocar: espera e apaga o arquivo.
fn discard(job: Option<Job>) {
    if let Some(Ok(Ok(path))) = job.map(JoinHandle::join) {
        let _ = std::fs::remove_file(path);
    }
}

/// Fala o texto (já limpo), interrompendo o que estiver tocando. Volta quando
/// termina ou quando outra fala/parar assume.
fn speak(app: &AppHandle, text: &str) -> Result<(), String> {
    let state = app.state::<SpeechState>();
    stop_playing(&state);
    let generation = state.generation.load(Ordering::SeqCst);
    let current = || state.generation.load(Ordering::SeqCst) == generation;

    let engine = Arc::new(engine(&settings::load(app))?);
    let parts = chunks(text, FIRST_CHUNK_CHARS, CHUNK_CHARS, MAX_CHARS);
    let request = |i: usize| -> Job {
        let (app, engine, text, path) = (app.clone(), engine.clone(), parts[i].clone(), tmp_path(&format!("{generation}-{i}.wav")));
        thread::spawn(move || {
            match &*engine {
                Engine::Api(cfg) => synthesize_retrying(&app, generation, cfg, &text, &path),
                Engine::Local { python, script } => synthesize_local(python, script, &text, &path),
            }
            .map(|_| path)
        })
    };

    let mut next = (!parts.is_empty()).then(|| request(0));
    let mut i = 0;
    while let Some(job) = next.take() {
        let wav = match job.join() {
            Ok(Ok(wav)) => wav,
            // Interrompida no meio: não é erro pra mostrar.
            _ if !current() => break,
            Ok(Err(e)) => return Err(e),
            Err(_) => return Err("falhou ao gerar o áudio".into()),
        };
        i += 1;
        if !current() {
            let _ = std::fs::remove_file(&wav);
            break;
        }
        if i < parts.len() {
            next = Some(request(i));
        }
        let played = play(&state, &wav);
        let _ = std::fs::remove_file(&wav);
        if played.is_err() || !current() {
            discard(next.take());
            played?;
        }
    }
    Ok(())
}

/// Pergunta digitada (texto ou colagem), não o resultado de uma ferramenta.
fn is_prompt(entry: &serde_json::Value) -> bool {
    if entry.get("isMeta").and_then(serde_json::Value::as_bool) == Some(true) {
        return false;
    }
    match entry.pointer("/message/content") {
        Some(serde_json::Value::String(_)) => true,
        Some(serde_json::Value::Array(parts)) => {
            !parts.iter().any(|p| p.get("type").and_then(serde_json::Value::as_str) == Some("tool_result"))
        }
        _ => false,
    }
}

/// Texto da resposta à última pergunta do histórico. None se ela ainda não foi
/// gravada: o hook de "terminou" pode chegar antes do Claude escrever o arquivo,
/// e aí a última resposta em texto seria a da pergunta anterior.
pub(crate) fn final_reply(jsonl: &str) -> Option<String> {
    let mut reply = None;
    for entry in jsonl.lines().filter_map(|l| serde_json::from_str::<serde_json::Value>(l).ok()) {
        // Subagentes gravam no mesmo arquivo em versões antigas: não são a resposta.
        if entry.get("isSidechain").and_then(serde_json::Value::as_bool) == Some(true) {
            continue;
        }
        match entry.get("type").and_then(serde_json::Value::as_str) {
            Some("user") if is_prompt(&entry) => reply = None,
            Some("assistant") => {
                let text: Vec<&str> = entry
                    .pointer("/message/content")
                    .and_then(serde_json::Value::as_array)
                    .into_iter()
                    .flatten()
                    .filter(|c| c.get("type").and_then(serde_json::Value::as_str) == Some("text"))
                    .filter_map(|c| c.get("text")?.as_str())
                    .collect();
                if !text.is_empty() {
                    reply = Some(text.join("\n"));
                }
            }
            _ => {}
        }
    }
    reply
}

/// Resposta à última pergunta do Claude que roda no processo `pid`, esperando
/// até 3 s ela aparecer no histórico.
fn last_reply(pid: u32) -> Option<String> {
    let proc = suggest::read_proc(pid)?;
    let path = suggest::transcript(pid, &proc)?;
    for _ in 0..15 {
        if let Some(reply) = suggest::read_tail(&path).and_then(|t| final_reply(&t)) {
            return Some(reply);
        }
        thread::sleep(Duration::from_millis(200));
    }
    None
}

/// Fala um texto qualquer (o "Testar voz" das configurações).
#[tauri::command]
pub async fn speak_text(app: AppHandle, text: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || speak(&app, &speakable(&text))).await.map_err(|e| e.to_string())?
}

/// Lê em voz alta a última resposta do Claude da aba.
#[tauri::command]
pub async fn speak_claude_reply(app: AppHandle, state: State<'_, PtyState>, id: u32) -> Result<(), String> {
    let Some(fg) = crate::pty::foreground_pid(&state, id) else { return Ok(()) };
    tauri::async_runtime::spawn_blocking(move || {
        // Sem resposta em texto (o "terminou" veio de um Esc ou de um /comando): nada a ler.
        match last_reply(fg) {
            Some(reply) => speak(&app, &speakable(&reply)),
            None => Ok(()),
        }
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub fn speak_stop(state: State<'_, SpeechState>) {
    stop_playing(&state);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn limpa_o_markdown_da_resposta() {
        // Formato real de uma resposta final do Claude Code (anonimizada).
        let reply = "Pronto, o teste passou.\n\n## Mudanças\n\n- **`src/lib/voice.ts`**: tira o *enviar* do fim\n1. Veja [o PR](https://github.com/org/repo/pull/1)\n\n```sh\nnpm test\n```\n\n| Arquivo | Linhas |\n|---|---|\n| a.ts | 3 |\n\n---\nDetalhes em https://example.com/x, se quiser";
        assert_eq!(
            speakable(reply),
            "Pronto, o teste passou. Mudanças. voice: tira o enviar do fim. Veja o PR. Detalhes em se quiser."
        );
    }

    #[test]
    fn le_so_o_nome_do_arquivo() {
        assert_eq!(
            speakable("Mudei src-tauri/src/speech.rs e voice.test.ts, rode scripts/run-test.sh. Gera o .deb na versão 3.5."),
            "Mudei speech e voice, rode run-test. Gera o deb na versão 3.5."
        );
    }

    #[test]
    fn divide_em_trechos_por_frase() {
        let text = "Primeira frase. Segunda frase! Terceira?";
        assert_eq!(chunks(text, 20, 20, 1000), ["Primeira frase.", "Segunda frase!", "Terceira?"]);
        assert_eq!(chunks(text, 35, 35, 1000), ["Primeira frase. Segunda frase!", "Terceira?"]);
        // Frase maior que o trecho quebra entre palavras.
        assert_eq!(chunks("um dois três quatro", 9, 9, 1000), ["um dois", "três", "quatro"]);
        // Passou do limite: para no fim da frase anterior.
        assert_eq!(chunks(text, 200, 200, 32), ["Primeira frase. Segunda frase!"]);
        assert!(chunks("   ", 80, 200, 1000).is_empty());
    }

    #[test]
    fn primeiro_trecho_e_curto() {
        // Frases curtas: o primeiro trecho junta só o que cabe nele.
        let text = "Primeira frase. Segunda frase! Terceira?";
        assert_eq!(chunks(text, 16, 200, 1000), ["Primeira frase.", "Segunda frase! Terceira?"]);
        // Primeira frase longa: corta entre palavras e o resto vira o segundo trecho.
        assert_eq!(chunks("um dois três quatro. Fim.", 8, 200, 1000), ["um dois", "três quatro.", "Fim."]);
    }

    #[test]
    fn so_a_resposta_da_ultima_pergunta() {
        // Linhas no formato real do histórico do Claude Code (anonimizadas, campos de sobra cortados).
        let old = r#"{"type":"user","message":{"role":"user","content":"primeira pergunta"},"timestamp":"2026-11-13T17:49:44.900Z"}
{"type":"assistant","message":{"role":"assistant","content":[{"type":"thinking","thinking":"..."}]},"timestamp":"2026-11-13T17:49:46.358Z"}
{"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"Resposta antiga."}]},"timestamp":"2026-11-13T17:49:48.363Z"}
{"type":"system","subtype":"turn_duration","timestamp":"2026-11-13T17:49:48.399Z"}"#;
        assert_eq!(final_reply(old).as_deref(), Some("Resposta antiga."));

        // Pergunta nova sem a resposta gravada ainda: nada (e não a antiga).
        let asked = format!("{old}\n{}", r#"{"type":"user","message":{"role":"user","content":"Novo teste"},"timestamp":"2026-11-13T17:50:18.425Z"}"#);
        assert_eq!(final_reply(&asked), None);

        // Ferramenta no meio: o resultado dela não é pergunta; vale o texto final.
        let done = format!(
            "{asked}\n{}\n{}\n{}\n{}",
            r#"{"type":"assistant","message":{"content":[{"type":"text","text":"Vou olhar."},{"type":"tool_use","id":"t1","name":"Bash","input":{}}]}}"#,
            r#"{"type":"user","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"t1","content":"ok"}]}}"#,
            r#"{"type":"user","isMeta":true,"message":{"role":"user","content":"<system-reminder>x</system-reminder>"}}"#,
            r#"{"type":"assistant","message":{"content":[{"type":"text","text":"Recebi o Novo teste."}]}}"#,
        );
        assert_eq!(final_reply(&done).as_deref(), Some("Recebi o Novo teste."));

        // Colagem com imagem também é pergunta.
        let pasted = format!("{done}\n{}", r#"{"type":"user","message":{"role":"user","content":[{"type":"text","text":"olha"},{"type":"image","source":{}}]}}"#);
        assert_eq!(final_reply(&pasted), None);
    }

    #[test]
    fn espera_e_tenta_de_novo_so_quando_vale() {
        // Cabeçalhos no formato do Groq (resposta 429 real, anonimizada).
        let headers = "HTTP/2 429 \r\ncontent-type: application/json\r\nretry-after: 3\r\nx-ratelimit-limit-tokens: 1200\r\n\r\n";
        assert_eq!(retry_delay("429", headers), Some(Duration::from_secs(3)));
        assert_eq!(retry_delay("429", "Retry-After: 0.5\r\n"), Some(Duration::from_millis(500)));
        // Cota do dia: esperar horas não faz sentido, desiste na hora.
        assert_eq!(retry_delay("429", "retry-after: 26000\r\n"), None);
        assert_eq!(retry_delay("429", ""), Some(Duration::from_secs(2)));
        assert_eq!(retry_delay("500", ""), Some(Duration::from_secs(1)));
        assert_eq!(retry_delay("400", headers), None);
        assert_eq!(retry_delay("401", ""), None);
    }

    #[test]
    fn mesma_api_do_ditado_ou_outra() {
        let voice = VoiceSettings { api_url: "https://ditado".into(), api_key: "k1".into(), ..Default::default() };
        let mut speech = SpeechSettings { api_url: "https://voz".into(), api_key: "k2".into(), ..Default::default() };
        assert_eq!(speech.endpoint(&voice), ("https://ditado", "k1"));
        speech.same_api = false;
        assert_eq!(speech.endpoint(&voice), ("https://voz", "k2"));
        speech.api_key.clear();
        assert!(config(&voice, &speech).err().unwrap().contains("API de voz"));
    }
}
