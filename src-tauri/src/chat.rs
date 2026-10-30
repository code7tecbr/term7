//! Modo chat (experimental): o Claude da aba roda sem a interface de terminal,
//! conversando em JSON pelo stdin/stdout (`--input-format/--output-format
//! stream-json`), e o frontend desenha a conversa.
//!
//! Pra entrar, o Claude que está no terminal é encerrado e o novo processo
//! continua a mesma sessão (`--resume`), com o mesmo binário, pasta e ambiente.
//! Os pedidos de permissão chegam como `control_request` e o frontend responde.

use std::{
    collections::HashMap,
    io::{BufRead, BufReader, Write},
    path::PathBuf,
    process::{Child, ChildStdin, Command, Stdio},
    sync::{
        atomic::{AtomicU32, Ordering},
        Mutex,
    },
    thread,
    time::{Duration, Instant},
};

use serde::Serialize;
use serde_json::Value;
use tauri::{ipc::Channel, AppHandle, Manager, State};

use crate::{agents, pty::PtyState, suggest};

struct ChatProc {
    stdin: ChildStdin,
    pid: u32,
}

/// Log do último chat (`chat.log` na pasta de dados do app), pra investigar quando
/// algo trava: o que foi mandado (>), o que o Claude respondeu (<) e o stderr (!).
/// Tem a conversa: fica com permissão 600 e é recriado a cada chat.
#[derive(Clone)]
struct ChatLog(std::sync::Arc<Mutex<Option<std::fs::File>>>);

impl ChatLog {
    fn open(app: &AppHandle) -> Self {
        use std::os::unix::fs::OpenOptionsExt;
        let file = app.path().app_data_dir().ok().and_then(|dir| {
            std::fs::create_dir_all(&dir).ok()?;
            std::fs::OpenOptions::new().write(true).create(true).truncate(true).mode(0o600).open(dir.join("chat.log")).ok()
        });
        Self(std::sync::Arc::new(Mutex::new(file)))
    }

    fn write(&self, mark: char, text: &str) {
        let mut guard = self.0.lock().unwrap();
        let Some(f) = guard.as_mut() else { return };
        let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs_f64()).unwrap_or(0.0);
        // Linhas enormes (histórico, resultados) só atrapalham a leitura.
        let text: String = if text.chars().count() > 1500 { text.chars().take(1500).chain(" …".chars()).collect() } else { text.to_string() };
        let _ = writeln!(f, "{secs:.3} {mark} {text}");
    }
}

#[derive(Default)]
pub struct ChatState {
    procs: Mutex<HashMap<u32, ChatProc>>,
    log: Mutex<Option<ChatLog>>,
    next_id: AtomicU32,
}

#[derive(Serialize)]
pub struct ChatStarted {
    id: u32,
    session_id: String,
    /// Comando pra voltar pro terminal na mesma sessão, com a pasta de configuração
    /// do processo (um alias como `CLAUDE_CONFIG_DIR=~/.outra claude` é outra conta).
    resume_command: String,
    cwd: String,
    /// A conversa até aqui, no mesmo formato do popup dos agentes.
    history: Vec<agents::Step>,
}

/// O que é preciso pra recriar o Claude da aba fora do terminal.
struct Handoff {
    pid: u32,
    exe: PathBuf,
    cwd: PathBuf,
    env: HashMap<String, String>,
    session_id: String,
    transcript: Option<PathBuf>,
}

fn handoff(pid: u32) -> Result<Handoff, String> {
    let proc = suggest::read_proc(pid).ok_or("o Claude não está rodando nesta aba")?;
    let exe = std::fs::read_link(format!("/proc/{pid}/exe")).map_err(|e| format!("não achei o binário do Claude: {e}"))?;
    let transcript = suggest::transcript(pid, &proc);
    let session_id = suggest::session_id(pid, &proc).ok_or("o Claude desta aba ainda não tem sessão (mande uma mensagem antes)")?;
    Ok(Handoff { pid, exe, cwd: proc.cwd, env: proc.env, session_id, transcript })
}

/// Aspas simples só quando precisa.
fn shell_quote(s: &str) -> String {
    if !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric() || "@%+=:,./~_-".contains(c)) {
        s.to_string()
    } else {
        format!("'{}'", s.replace('\'', "'\\''"))
    }
}

fn resume_command(h: &Handoff) -> String {
    let base = format!("claude --resume {}", h.session_id);
    let Some(dir) = h.env.get("CLAUDE_CONFIG_DIR").filter(|d| !d.is_empty()) else { return base };
    // A padrão (~/.claude) não precisa ser repetida.
    let default = h.env.get("HOME").map(|home| format!("{}/.claude", home.trim_end_matches('/')));
    if default.as_deref() == Some(dir.trim_end_matches('/')) {
        return base;
    }
    format!("CLAUDE_CONFIG_DIR={} {base}", shell_quote(dir))
}

fn alive(pid: u32) -> bool {
    std::path::Path::new(&format!("/proc/{pid}")).exists()
}

/// SIGTERM e espera sair: os dois processos não podem escrever na mesma sessão.
fn terminate(pid: u32) -> Result<(), String> {
    let _ = Command::new("kill").args(["-TERM", &pid.to_string()]).status();
    let deadline = Instant::now() + Duration::from_secs(5);
    while alive(pid) {
        if Instant::now() > deadline {
            return Err("o Claude do terminal não fechou".into());
        }
        thread::sleep(Duration::from_millis(50));
    }
    Ok(())
}

fn spawn(h: &Handoff) -> Result<Child, String> {
    Command::new(&h.exe)
        .args([
            "-p",
            "--input-format",
            "stream-json",
            "--output-format",
            "stream-json",
            "--verbose",
            "--include-partial-messages",
            // Os pedidos de permissão vêm pro term7 como control_request.
            "--permission-prompts",
            "host",
            "--permission-prompt-tool",
            "stdio",
            "--resume",
            &h.session_id,
        ])
        .current_dir(&h.cwd)
        .env_clear()
        .envs(&h.env)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("não consegui abrir o Claude em modo chat: {e}"))
}

/// Encerra o Claude do terminal da aba e abre o mesmo em modo chat.
/// Cada linha JSON do Claude vai por `on_line`; `on_exit` recebe o código e o fim do stderr.
#[tauri::command]
pub async fn chat_start(
    app: AppHandle,
    pty: State<'_, PtyState>,
    pty_id: u32,
    on_line: Channel<String>,
    on_exit: Channel<(i32, String)>,
) -> Result<ChatStarted, String> {
    let fg = crate::pty::foreground_pid(&pty, pty_id).ok_or("aba sem processo")?;
    tauri::async_runtime::spawn_blocking(move || {
        let log = ChatLog::open(&app);
        *app.state::<ChatState>().log.lock().unwrap() = Some(log.clone());
        let h = match handoff(fg) {
            Ok(h) => h,
            Err(e) => {
                log.write('!', &format!("handoff: {e}"));
                return Err(e);
            }
        };
        log.write('-', &format!("sessão {} em {} ({})", h.session_id, h.cwd.display(), h.exe.display()));
        let history = h.transcript.as_deref().and_then(|t| std::fs::read_to_string(t).ok()).map(|t| agents::steps(&t)).unwrap_or_default();
        if let Err(e) = terminate(h.pid) {
            log.write('!', &e);
            return Err(e);
        }
        log.write('-', "Claude do terminal encerrado");
        let mut child = spawn(&h).inspect_err(|e| log.write('!', e))?;
        log.write('-', &format!("chat aberto, pid {}", child.id()));
        let stdin = child.stdin.take().ok_or("sem stdin")?;
        let stdout = child.stdout.take().ok_or("sem stdout")?;
        let stderr = child.stderr.take().ok_or("sem stderr")?;
        let state = app.state::<ChatState>();
        let id = state.next_id.fetch_add(1, Ordering::Relaxed);
        state.procs.lock().unwrap().insert(id, ChatProc { stdin, pid: child.id() });

        // O stderr só interessa se o processo morrer: guarda o fim dele.
        let err_tail = std::sync::Arc::new(Mutex::new(String::new()));
        let tail = err_tail.clone();
        let err_log = log.clone();
        thread::spawn(move || {
            for line in BufReader::new(stderr).lines().map_while(Result::ok) {
                err_log.write('!', &line);
                let mut t = tail.lock().unwrap();
                t.push_str(&line);
                t.push('\n');
                if t.len() > 4000 {
                    let cut = t.len() - 2000;
                    let cut = (cut..t.len()).find(|&i| t.is_char_boundary(i)).unwrap_or(0);
                    t.drain(..cut);
                }
            }
        });
        thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                if line.trim().is_empty() {
                    continue;
                }
                // As fatias do texto em streaming são muitas: o log fica com as mensagens inteiras.
                if !line.starts_with(r#"{"type":"stream_event""#) {
                    log.write('<', &line);
                }
                if on_line.send(line).is_err() {
                    log.write('!', "a tela do chat parou de receber");
                    break;
                }
            }
            let code = child.wait().ok().and_then(|s| s.code()).unwrap_or(-1);
            log.write('-', &format!("chat fechou com código {code}"));
            app.state::<ChatState>().procs.lock().unwrap().remove(&id);
            let tail = err_tail.lock().unwrap().clone();
            let _ = on_exit.send((code, tail));
        });

        Ok(ChatStarted {
            id,
            resume_command: resume_command(&h),
            session_id: h.session_id,
            cwd: h.cwd.to_string_lossy().into_owned(),
            history,
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Manda uma mensagem JSON (já pronta, uma linha) pro Claude do chat.
#[tauri::command]
pub fn chat_send(state: State<'_, ChatState>, id: u32, message: Value) -> Result<(), String> {
    let mut procs = state.procs.lock().unwrap();
    let p = procs.get_mut(&id).ok_or("o chat já foi encerrado")?;
    let mut line = serde_json::to_string(&message).map_err(|e| e.to_string())?;
    if let Some(log) = state.log.lock().unwrap().as_ref() {
        log.write('>', &line);
    }
    line.push('\n');
    p.stdin.write_all(line.as_bytes()).and_then(|_| p.stdin.flush()).map_err(|e| e.to_string())
}

/// Encerra o Claude do chat e espera ele sair (pra o terminal poder retomar a sessão).
#[tauri::command]
pub async fn chat_stop(state: State<'_, ChatState>, id: u32) -> Result<(), String> {
    // Tirar do mapa fecha o stdin: o Claude termina sozinho no fim da entrada.
    let Some(p) = state.procs.lock().unwrap().remove(&id) else { return Ok(()) };
    let pid = p.pid;
    drop(p);
    tauri::async_runtime::spawn_blocking(move || {
        let deadline = Instant::now() + Duration::from_secs(2);
        while alive(pid) && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(50));
        }
        if alive(pid) {
            terminate(pid)?;
        }
        Ok(())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;

    fn handoff_with(env: &[(&str, &str)]) -> Handoff {
        Handoff {
            pid: 1,
            exe: PathBuf::from("/bin/claude"),
            cwd: PathBuf::from("/"),
            env: env.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect(),
            session_id: "abc-123".into(),
            transcript: None,
        }
    }

    #[test]
    fn retoma_com_a_pasta_de_configuracao() {
        assert_eq!(resume_command(&handoff_with(&[("HOME", "/home/u")])), "claude --resume abc-123");
        assert_eq!(resume_command(&handoff_with(&[("HOME", "/home/u"), ("CLAUDE_CONFIG_DIR", "/home/u/.claude")])), "claude --resume abc-123");
        assert_eq!(
            resume_command(&handoff_with(&[("HOME", "/home/u"), ("CLAUDE_CONFIG_DIR", "/home/u/.claude-code7")])),
            "CLAUDE_CONFIG_DIR=/home/u/.claude-code7 claude --resume abc-123"
        );
        assert_eq!(
            resume_command(&handoff_with(&[("CLAUDE_CONFIG_DIR", "/tmp/com espaço")])),
            "CLAUDE_CONFIG_DIR='/tmp/com espaço' claude --resume abc-123"
        );
    }
}
