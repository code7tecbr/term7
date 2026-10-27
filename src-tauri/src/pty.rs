use std::{
    collections::HashMap,
    io::{Read, Write},
    sync::{
        atomic::{AtomicU32, Ordering},
        Mutex,
    },
    thread,
};

use serde::Serialize;
use portable_pty::{native_pty_system, ChildKiller, CommandBuilder, MasterPty, PtySize};
use tauri::{
    ipc::{Channel, InvokeResponseBody},
    AppHandle, Manager, State,
};

struct Session {
    master: Box<dyn MasterPty + Send>,
    writer: Box<dyn Write + Send>,
    killer: Box<dyn ChildKiller + Send + Sync>,
    /// PID do shell, usado pra descobrir a pasta atual.
    pid: Option<u32>,
}

#[derive(Serialize)]
pub struct PtyInfo {
    /// Pasta atual do shell.
    cwd: Option<String>,
    /// Processo em primeiro plano (ex.: "zsh", "vim", "npm").
    process: Option<String>,
}

/// Todas as sessões de shell abertas, indexadas por id (uma por aba/painel).
#[derive(Default)]
pub struct PtyState {
    sessions: Mutex<HashMap<u32, Session>>,
    next_id: AtomicU32,
}

fn default_shell() -> String {
    if cfg!(windows) {
        return "powershell.exe".into();
    }
    std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".into())
}

fn size(cols: u16, rows: u16) -> PtySize {
    PtySize { rows, cols, pixel_width: 0, pixel_height: 0 }
}

#[tauri::command]
pub fn pty_spawn(
    app: AppHandle,
    state: State<'_, PtyState>,
    cols: u16,
    rows: u16,
    cwd: Option<String>,
    on_data: Channel<InvokeResponseBody>,
    on_exit: Channel<i32>,
) -> Result<u32, String> {
    let pair = native_pty_system()
        .openpty(size(cols, rows))
        .map_err(|e| e.to_string())?;

    let shell = default_shell();
    let mut cmd = CommandBuilder::new(&shell);
    // Shell de login: carrega ~/.zprofile / ~/.profile (PATH completo) além do ~/.zshrc,
    // o que importa quando o app é aberto pelo menu e não por outro terminal.
    if cfg!(unix) && ["zsh", "bash", "fish"].iter().any(|s| shell.ends_with(s)) {
        cmd.arg("-l");
    }
    cmd.env("TERM", "xterm-256color");
    cmd.env("COLORTERM", "truecolor");
    cmd.env("TERM_PROGRAM", "term7");
    // Pasta salva que não existe mais (ex.: repo apagado) cai pra home.
    let cwd = cwd.filter(|d| std::path::Path::new(d).is_dir());
    if let Some(dir) = cwd.or_else(|| std::env::var("HOME").ok()) {
        cmd.cwd(dir);
    }

    let mut child = pair.slave.spawn_command(cmd).map_err(|e| e.to_string())?;
    // Sem fechar o lado "slave" aqui, o reader nunca recebe EOF quando o shell sai.
    drop(pair.slave);

    let mut reader = pair.master.try_clone_reader().map_err(|e| e.to_string())?;
    let writer = pair.master.take_writer().map_err(|e| e.to_string())?;
    let killer = child.clone_killer();
    let pid = child.process_id();

    let id = state.next_id.fetch_add(1, Ordering::Relaxed);
    state
        .sessions
        .lock()
        .unwrap()
        .insert(id, Session { master: pair.master, writer, killer, pid });

    // Lê a saída do shell e manda pro frontend como bytes brutos.
    // Quando o shell termina, avisa o código de saída e limpa a sessão.
    thread::spawn(move || {
        let mut buf = [0u8; 16 * 1024];
        loop {
            match reader.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    if on_data.send(InvokeResponseBody::Raw(buf[..n].to_vec())).is_err() {
                        break;
                    }
                }
            }
        }
        let code = child.wait().map(|s| s.exit_code() as i32).unwrap_or(-1);
        app.state::<PtyState>().sessions.lock().unwrap().remove(&id);
        let _ = on_exit.send(code);
    });

    Ok(id)
}

#[tauri::command]
pub fn pty_write(state: State<'_, PtyState>, id: u32, data: String) -> Result<(), String> {
    let mut sessions = state.sessions.lock().unwrap();
    let session = sessions.get_mut(&id).ok_or("sessão não encontrada")?;
    session.writer.write_all(data.as_bytes()).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn pty_resize(state: State<'_, PtyState>, id: u32, cols: u16, rows: u16) -> Result<(), String> {
    let sessions = state.sessions.lock().unwrap();
    let session = sessions.get(&id).ok_or("sessão não encontrada")?;
    session.master.resize(size(cols, rows)).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn pty_kill(state: State<'_, PtyState>, id: u32) -> Result<(), String> {
    if let Some(mut session) = state.sessions.lock().unwrap().remove(&id) {
        let _ = session.killer.kill();
    }
    Ok(())
}

#[tauri::command]
pub fn pty_info(state: State<'_, PtyState>, id: u32) -> Result<PtyInfo, String> {
    let sessions = state.sessions.lock().unwrap();
    let session = sessions.get(&id).ok_or("sessão não encontrada")?;

    #[cfg(unix)]
    let foreground = session.master.process_group_leader().map(|p| p as u32);
    #[cfg(not(unix))]
    let foreground: Option<u32> = None;

    Ok(PtyInfo {
        cwd: session.pid.and_then(proc_cwd),
        process: foreground.or(session.pid).and_then(proc_name),
    })
}

// Por enquanto só Linux (/proc). macOS/Windows ficam pra depois.
#[cfg(target_os = "linux")]
fn proc_cwd(pid: u32) -> Option<String> {
    std::fs::read_link(format!("/proc/{pid}/cwd"))
        .ok()
        .map(|p| p.to_string_lossy().into_owned())
}

#[cfg(target_os = "linux")]
fn proc_name(pid: u32) -> Option<String> {
    std::fs::read_to_string(format!("/proc/{pid}/comm"))
        .ok()
        .map(|s| s.trim().to_string())
}

#[cfg(not(target_os = "linux"))]
fn proc_cwd(_pid: u32) -> Option<String> {
    None
}

#[cfg(not(target_os = "linux"))]
fn proc_name(_pid: u32) -> Option<String> {
    None
}
