//! API local pras extensões (pasta `extensions/`): socket Unix em
//! `$XDG_RUNTIME_DIR/term7/api.sock`, só do usuário. Desligada por padrão.
//!
//! Cada linha recebida é um pedido `{"method": "...", "params": {...}}` e cada
//! resposta é uma linha `{"result": ...}` ou `{"error": "..."}`. Quem responde é
//! o frontend, onde estão as abas e as telas: o pedido vai como evento
//! `api-request` e volta pelo comando `api_respond`.

use std::{
    collections::HashMap,
    io::{BufRead, BufReader, Read, Write},
    os::unix::{
        fs::PermissionsExt,
        net::{UnixListener, UnixStream},
    },
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc, Arc, Mutex,
    },
    time::Duration,
};

use serde::Serialize;
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter, Manager, State};

/// Muda quando um método muda de forma incompatível; as extensões conferem.
const API_VERSION: u32 = 1;
const MAX_REQUEST_BYTES: u64 = 1 << 20;
const FRONTEND_TIMEOUT: Duration = Duration::from_secs(10);

type Reply = Result<Value, String>;

#[derive(Default)]
pub struct ApiState {
    pending: Mutex<HashMap<u64, mpsc::Sender<Reply>>>,
    next_id: AtomicU64,
    /// Sinal de parada do servidor ligado agora (None = desligado).
    running: Mutex<Option<Arc<AtomicBool>>>,
}

pub fn socket_path() -> PathBuf {
    crate::claude::state_dir().join("api.sock")
}

#[derive(Serialize, Clone)]
struct ApiRequest {
    id: u64,
    method: String,
    params: Value,
}

/// Aba de quem chamou: a extensão repassa o `TERM7_PANE` que herdou do shell.
/// Só vale se for desta instância do app (o PID vem na frente).
fn caller_pty(pane: &str, pid: u32) -> Option<u32> {
    let (p, id) = pane.split_once('-')?;
    (p.parse::<u32>().ok()? == pid).then(|| id.parse().ok())?
}

/// Lê o pedido e devolve (método, params), já trocando `pane` por `callerPty`.
fn parse_request(line: &str, pid: u32) -> Result<(String, Value), String> {
    let req: Value = serde_json::from_str(line).map_err(|e| format!("JSON inválido: {e}"))?;
    let method = req.get("method").and_then(Value::as_str).ok_or("falta \"method\"")?.to_string();
    let mut params = req.get("params").cloned().unwrap_or_else(|| json!({}));
    if !params.is_object() {
        return Err("\"params\" precisa ser um objeto".into());
    }
    let caller = params.get("pane").and_then(Value::as_str).and_then(|p| caller_pty(p, pid));
    let obj = params.as_object_mut().unwrap();
    obj.remove("pane");
    obj.insert("callerPty".into(), json!(caller));
    Ok((method, params))
}

fn ask_frontend(app: &AppHandle, method: String, params: Value) -> Reply {
    let state = app.state::<ApiState>();
    let id = state.next_id.fetch_add(1, Ordering::Relaxed);
    let (tx, rx) = mpsc::channel();
    state.pending.lock().unwrap().insert(id, tx);
    let sent = app.emit("api-request", ApiRequest { id, method, params });
    let reply = match sent {
        Ok(()) => rx.recv_timeout(FRONTEND_TIMEOUT).unwrap_or_else(|_| Err("o term7 não respondeu a tempo".into())),
        Err(e) => Err(e.to_string()),
    };
    state.pending.lock().unwrap().remove(&id);
    reply
}

fn handle(app: &AppHandle, line: &str) -> Value {
    let reply = parse_request(line, std::process::id()).and_then(|(method, params)| match method.as_str() {
        "version" => Ok(json!({ "api": API_VERSION, "app": app.package_info().version.to_string() })),
        _ => ask_frontend(app, method, params),
    });
    match reply {
        Ok(result) => json!({ "result": result }),
        Err(error) => json!({ "error": error }),
    }
}

fn serve(app: AppHandle, stream: UnixStream) {
    let Ok(mut writer) = stream.try_clone() else { return };
    let mut reader = BufReader::new(stream);
    loop {
        let mut line = String::new();
        // Limite por pedido: uma linha sem fim não come a memória do app.
        match (&mut reader).take(MAX_REQUEST_BYTES).read_line(&mut line) {
            Ok(0) | Err(_) => return,
            Ok(_) if line.trim().is_empty() => continue,
            Ok(_) => {}
        }
        let mut out = handle(&app, line.trim()).to_string();
        out.push('\n');
        if writer.write_all(out.as_bytes()).is_err() {
            return;
        }
    }
}

/// Liga o servidor. Erro se outra janela do term7 já estiver com o socket.
pub fn start(app: &AppHandle) -> Result<(), String> {
    let state = app.state::<ApiState>();
    let mut running = state.running.lock().unwrap();
    if running.is_some() {
        return Ok(());
    }
    let path = socket_path();
    let dir = path.parent().unwrap();
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700)).map_err(|e| e.to_string())?;
    if path.exists() {
        if UnixStream::connect(&path).is_ok() {
            return Err("outra janela do term7 já está com a API ligada".into());
        }
        // Sobrou de um app que fechou sem apagar.
        let _ = std::fs::remove_file(&path);
    }
    let listener = UnixListener::bind(&path).map_err(|e| e.to_string())?;
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).map_err(|e| e.to_string())?;

    let stop = Arc::new(AtomicBool::new(false));
    *running = Some(stop.clone());
    let app = app.clone();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            if stop.load(Ordering::Relaxed) {
                break;
            }
            if let Ok(stream) = stream {
                let app = app.clone();
                std::thread::spawn(move || serve(app, stream));
            }
        }
    });
    Ok(())
}

pub fn is_running(app: &AppHandle) -> bool {
    app.state::<ApiState>().running.lock().unwrap().is_some()
}

pub fn stop(app: &AppHandle) {
    let Some(flag) = app.state::<ApiState>().running.lock().unwrap().take() else { return };
    flag.store(true, Ordering::Relaxed);
    let path = socket_path();
    // Acorda o accept() pra thread ver o sinal e sair.
    let _ = UnixStream::connect(&path);
    let _ = std::fs::remove_file(path);
}

/// Resposta do frontend a um `api-request`.
#[tauri::command]
pub fn api_respond(state: State<'_, ApiState>, id: u64, result: Option<Value>, error: Option<String>) {
    if let Some(tx) = state.pending.lock().unwrap().remove(&id) {
        let _ = tx.send(match error {
            Some(e) => Err(e),
            None => Ok(result.unwrap_or(Value::Null)),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aba_de_quem_chamou_so_desta_instancia() {
        assert_eq!(caller_pty("4242-7", 4242), Some(7));
        assert_eq!(caller_pty("999-7", 4242), None);
        assert_eq!(caller_pty("lixo", 4242), None);
        assert_eq!(caller_pty("4242-x", 4242), None);
    }

    #[test]
    fn le_o_pedido_e_troca_pane_por_caller() {
        let (method, params) =
            parse_request(r#"{"method":"tabs.send","params":{"tab":3,"text":"oi","pane":"10-2"}}"#, 10).unwrap();
        assert_eq!(method, "tabs.send");
        assert_eq!(params, json!({ "tab": 3, "text": "oi", "callerPty": 2 }));

        let (_, params) = parse_request(r#"{"method":"tabs.list"}"#, 10).unwrap();
        assert_eq!(params, json!({ "callerPty": null }));
    }

    #[test]
    fn recusa_pedido_mal_formado() {
        assert!(parse_request("não é json", 1).is_err());
        assert!(parse_request(r#"{"params":{}}"#, 1).is_err());
        assert!(parse_request(r#"{"method":"x","params":[1]}"#, 1).is_err());
    }
}
