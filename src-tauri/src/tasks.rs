//! Comandos que o Claude da aba roda em segundo plano (Bash com `run_in_background`),
//! pro menu "Tarefas" da barra.
//!
//! Tudo sai do histórico da sessão: a chamada do Bash tem o comando e a descrição;
//! o resultado dela, o id da tarefa e o arquivo de saída (`.../tasks/<id>.output`);
//! e quando acaba entra um `<task-notification>` com o status e o código de saída.
//! Se ainda roda, um filho do processo `claude` está com o arquivo de saída aberto.

use std::{
    collections::{HashMap, HashSet},
    io::{Read, Seek, SeekFrom},
    path::{Path, PathBuf},
    sync::Mutex,
    time::SystemTime,
};

use serde::Serialize;
use serde_json::Value;
use tauri::State;

use crate::{pty::PtyState, suggest};

const MAX_TASKS: usize = 60;
/// Do log só o fim interessa (servidores de dev escrevem sem parar).
const LOG_TAIL_BYTES: u64 = 128 * 1024;

#[derive(Serialize, Clone, Copy, PartialEq, Debug)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    Running,
    Done,
    Failed,
    /// Parada pelo Claude ou pelo usuário (killed/stopped).
    Stopped,
    /// Sem aviso de fim e ninguém segurando o log: o Claude que rodou fechou antes.
    Ended,
}

#[derive(Serialize, Clone)]
pub struct Task {
    id: String,
    command: String,
    description: String,
    status: TaskStatus,
    exit_code: Option<i32>,
    /// ISO 8601, como o Claude grava.
    started: Option<String>,
    /// Última escrita no log (segundos desde 1970): quando terminou, ou a última saída.
    updated: Option<u64>,
    size: u64,
}

/// O que o histórico diz de cada tarefa.
#[derive(Clone, Default, Debug)]
struct Entry {
    command: String,
    description: String,
    started: Option<String>,
    output: Option<String>,
    /// Status do `<task-notification>` (completed, failed, killed, stopped).
    finished: Option<String>,
    exit_code: Option<i32>,
}

/// Leitura incremental: o histórico só cresce, então guarda onde parou.
#[derive(Default)]
struct Scan {
    offset: u64,
    /// tool_use_id -> (comando, descrição) dos Bash em segundo plano.
    calls: HashMap<String, (String, String)>,
    /// id da tarefa -> dados, na ordem em que apareceram.
    order: Vec<String>,
    tasks: HashMap<String, Entry>,
    /// Avisos de fim que chegaram antes do resultado (não deveria, mas não custa).
    early: HashMap<String, (String, Option<i32>)>,
}

static CACHE: Mutex<Option<HashMap<PathBuf, Scan>>> = Mutex::new(None);

fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 64 && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// Texto entre `<tag>` e `</tag>`.
fn tag<'a>(text: &'a str, name: &str) -> Option<&'a str> {
    let start = text.find(&format!("<{name}>"))? + name.len() + 2;
    let end = text[start..].find(&format!("</{name}>"))? + start;
    Some(&text[start..end])
}

/// "... completed (exit code 0)" / "... failed with exit code 1".
fn exit_code(summary: &str) -> Option<i32> {
    let rest = &summary[summary.rfind("exit code ")? + "exit code ".len()..];
    rest.chars().take_while(|c| c.is_ascii_digit()).collect::<String>().parse().ok()
}

impl Scan {
    fn line(&mut self, line: &str) {
        // Filtro barato antes do JSON: o histórico tem dezenas de MB.
        if line.contains("<task-notification>") {
            self.notifications(line);
        }
        if line.contains("\"run_in_background\":true") {
            self.call(line);
        }
        if line.contains("\"backgroundTaskId\"") {
            self.result(line);
        }
    }

    /// Avisos de fim: vêm na fila de mensagens ou como mensagem do usuário. O texto
    /// fica dentro de uma string JSON, mas as tags e o id não têm nada pra escapar.
    fn notifications(&mut self, line: &str) {
        for block in line.split("<task-notification>").skip(1) {
            let block = block.split("</task-notification>").next().unwrap_or(block);
            let (Some(id), Some(status)) = (tag(block, "task-id"), tag(block, "status")) else { continue };
            let code = tag(block, "summary").and_then(exit_code);
            match self.tasks.get_mut(id) {
                Some(e) => {
                    e.finished = Some(status.to_string());
                    e.exit_code = code;
                }
                None if valid_id(id) => {
                    self.early.insert(id.to_string(), (status.to_string(), code));
                }
                None => {}
            }
        }
    }

    fn call(&mut self, line: &str) {
        let Ok(d) = serde_json::from_str::<Value>(line) else { return };
        if d.get("type").and_then(Value::as_str) != Some("assistant") {
            return;
        }
        for c in d.pointer("/message/content").and_then(Value::as_array).into_iter().flatten() {
            let input = c.get("input").unwrap_or(&Value::Null);
            if c.get("type").and_then(Value::as_str) != Some("tool_use")
                || c.get("name").and_then(Value::as_str) != Some("Bash")
                || input.get("run_in_background").and_then(Value::as_bool) != Some(true)
            {
                continue;
            }
            let Some(id) = c.get("id").and_then(Value::as_str) else { continue };
            let text = |k: &str| input.get(k).and_then(Value::as_str).unwrap_or("").to_string();
            self.calls.insert(id.to_string(), (text("command"), text("description")));
        }
    }

    fn result(&mut self, line: &str) {
        let Ok(d) = serde_json::from_str::<Value>(line) else { return };
        if d.get("type").and_then(Value::as_str) != Some("user") {
            return;
        }
        let Some(task) = d.pointer("/toolUseResult/backgroundTaskId").and_then(Value::as_str).filter(|t| valid_id(t)) else { return };
        let Some(result) = d
            .pointer("/message/content")
            .and_then(Value::as_array)
            .and_then(|items| items.iter().find(|i| i.get("type").and_then(Value::as_str) == Some("tool_result")))
        else {
            return;
        };
        let use_id = result.get("tool_use_id").and_then(Value::as_str).unwrap_or("");
        let (command, description) = self.calls.remove(use_id).unwrap_or_default();
        let text = match result.get("content") {
            Some(Value::String(s)) => s.clone(),
            Some(Value::Array(items)) => items.iter().filter_map(|i| i.get("text").and_then(Value::as_str)).collect::<Vec<_>>().join("\n"),
            _ => String::new(),
        };
        let output = text
            .split("Output is being written to: ")
            .nth(1)
            .and_then(|rest| rest.split(".output").next())
            .map(|p| format!("{p}.output"));
        let (finished, exit_code) = self.early.remove(task).map(|(s, c)| (Some(s), c)).unwrap_or((None, None));
        if !self.tasks.contains_key(task) {
            self.order.push(task.to_string());
        }
        self.tasks.insert(
            task.to_string(),
            Entry {
                command,
                description,
                started: d.get("timestamp").and_then(Value::as_str).map(str::to_string),
                output,
                finished,
                exit_code,
            },
        );
    }
}

/// Lê o que entrou no histórico desde a última vez e devolve as tarefas, mais novas primeiro.
fn scan(path: &Path) -> Vec<(String, Entry)> {
    let mut cache = CACHE.lock().unwrap();
    let cache = cache.get_or_insert_with(HashMap::new);
    let state = cache.entry(path.to_path_buf()).or_default();
    if let Ok(mut file) = std::fs::File::open(path) {
        let len = file.metadata().map(|m| m.len()).unwrap_or(0);
        if len < state.offset {
            *state = Scan::default();
        }
        let mut buf = Vec::new();
        if file.seek(SeekFrom::Start(state.offset)).is_ok() && file.read_to_end(&mut buf).is_ok() {
            // Só linhas completas: a última pode estar sendo escrita agora.
            if let Some(end) = buf.iter().rposition(|&b| b == b'\n') {
                for line in String::from_utf8_lossy(&buf[..end]).lines() {
                    state.line(line);
                }
                state.offset += end as u64 + 1;
            }
        }
    }
    state.order.iter().rev().filter_map(|id| Some((id.clone(), state.tasks.get(id)?.clone()))).collect()
}

/// Arquivos `.output` abertos pelos filhos do processo do Claude (as tarefas vivas).
fn open_outputs(claude: u32) -> HashSet<PathBuf> {
    let mut out = HashSet::new();
    let Ok(procs) = std::fs::read_dir("/proc") else { return out };
    for p in procs.flatten() {
        let Some(pid) = p.file_name().to_str().and_then(|s| s.parse::<u32>().ok()) else { continue };
        let Ok(stat) = std::fs::read_to_string(format!("/proc/{pid}/stat")) else { continue };
        // O nome do processo vem entre parênteses e pode ter espaço: o pai vem depois do último ")".
        let ppid = stat.rfind(')').and_then(|i| stat[i + 1..].split_whitespace().nth(1)?.parse::<u32>().ok());
        if ppid != Some(claude) {
            continue;
        }
        let Ok(fds) = std::fs::read_dir(format!("/proc/{pid}/fd")) else { continue };
        for fd in fds.flatten() {
            if let Ok(target) = std::fs::read_link(fd.path()) {
                if target.extension().is_some_and(|e| e == "output") {
                    out.insert(target);
                }
            }
        }
    }
    out
}

/// Caminho do log, só se tiver a cara de um: `.../tasks/<id>.output`, sem "..".
fn output_path(id: &str, entry: &Entry) -> Option<PathBuf> {
    let path = PathBuf::from(entry.output.as_deref()?);
    let ok = path.is_absolute()
        && path.file_name().and_then(|n| n.to_str()) == Some(&format!("{id}.output"))
        && path.parent().and_then(Path::file_name).and_then(|n| n.to_str()) == Some("tasks")
        && !path.components().any(|c| matches!(c, std::path::Component::ParentDir));
    ok.then_some(path)
}

fn status(entry: &Entry, alive: bool) -> TaskStatus {
    match entry.finished.as_deref() {
        Some("completed") if entry.exit_code.unwrap_or(0) == 0 => TaskStatus::Done,
        Some("completed" | "failed") => TaskStatus::Failed,
        Some(_) => TaskStatus::Stopped,
        None if alive => TaskStatus::Running,
        None => TaskStatus::Ended,
    }
}

fn tasks(pid: u32) -> Vec<Task> {
    let Some(proc) = suggest::read_proc(pid) else { return Vec::new() };
    let Some(transcript) = suggest::transcript(pid, &proc) else { return Vec::new() };
    let entries = scan(&transcript);
    // Varrer o /proc só quando há tarefa sem aviso de fim.
    let alive = if entries.iter().any(|(_, e)| e.finished.is_none()) { open_outputs(pid) } else { HashSet::new() };
    let mut list: Vec<Task> = entries
        .into_iter()
        .map(|(id, e)| {
            let path = output_path(&id, &e);
            let meta = path.as_ref().and_then(|p| std::fs::metadata(p).ok());
            let updated = meta.as_ref().and_then(|m| m.modified().ok()).and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok()).map(|d| d.as_secs());
            let status = status(&e, path.as_ref().is_some_and(|p| alive.contains(p)));
            Task { id, command: e.command, description: e.description, status, exit_code: e.exit_code, started: e.started, updated, size: meta.map(|m| m.len()).unwrap_or(0) }
        })
        .collect();
    // Rodando primeiro; a ordem do histórico (mais nova em cima) fica dentro de cada grupo.
    list.sort_by_key(|t| t.status != TaskStatus::Running);
    list.truncate(MAX_TASKS);
    list
}

/// Tira as cores e os controles do terminal, e resolve o `\r` das barras de progresso
/// (fica só o último redesenho de cada linha).
fn clean_log(raw: &str) -> String {
    let mut text = String::with_capacity(raw.len());
    let mut chars = raw.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\x1b' => match chars.next() {
                // CSI: ESC [ ... letra final.
                Some('[') => {
                    for n in chars.by_ref() {
                        if ('@'..='~').contains(&n) {
                            break;
                        }
                    }
                }
                // OSC: ESC ] ... BEL ou ESC \.
                Some(']') => {
                    while let Some(n) = chars.next() {
                        if n == '\x07' || (n == '\x1b' && chars.next_if_eq(&'\\').is_some()) {
                            break;
                        }
                    }
                }
                _ => {}
            },
            '\n' | '\r' | '\t' => text.push(c),
            c if c.is_control() => {}
            c => text.push(c),
        }
    }
    text.split('\n')
        .map(|line| {
            let line = line.strip_suffix('\r').unwrap_or(line);
            line.rsplit('\r').next().unwrap_or(line)
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Fim do log, limpo pra mostrar. O primeiro trecho pode vir cortado no meio da linha.
fn read_log(path: &Path) -> std::io::Result<(String, bool)> {
    let mut file = std::fs::File::open(path)?;
    let len = file.metadata()?.len();
    let start = len.saturating_sub(LOG_TAIL_BYTES);
    file.seek(SeekFrom::Start(start))?;
    let mut buf = Vec::new();
    file.read_to_end(&mut buf)?;
    let mut text = String::from_utf8_lossy(&buf).into_owned();
    if start > 0 {
        if let Some(i) = text.find('\n') {
            text.drain(..=i);
        }
    }
    Ok((clean_log(&text), start > 0))
}

/// Comandos em segundo plano da sessão do Claude rodando na aba (vazio sem Claude).
#[tauri::command]
pub async fn claude_tasks(state: State<'_, PtyState>, id: u32) -> Result<Vec<Task>, String> {
    let Some(fg) = crate::pty::foreground_pid(&state, id) else { return Ok(Vec::new()) };
    tauri::async_runtime::spawn_blocking(move || tasks(fg)).await.map_err(|e| e.to_string())
}

#[derive(Serialize)]
pub struct TaskLog {
    text: String,
    /// O log é maior que o trecho mostrado.
    truncated: bool,
}

/// Fim do log de uma tarefa. O caminho sai do histórico, nunca do frontend.
#[tauri::command]
pub async fn claude_task_log(state: State<'_, PtyState>, id: u32, task: String) -> Result<TaskLog, String> {
    if !valid_id(&task) {
        return Err("tarefa inválida".into());
    }
    let Some(fg) = crate::pty::foreground_pid(&state, id) else { return Err("sem Claude nesta aba".into()) };
    tauri::async_runtime::spawn_blocking(move || {
        let proc = suggest::read_proc(fg).ok_or("sem Claude nesta aba")?;
        let transcript = suggest::transcript(fg, &proc).ok_or("histórico da sessão não encontrado")?;
        let path = scan(&transcript)
            .into_iter()
            .find(|(t, _)| *t == task)
            .and_then(|(t, e)| output_path(&t, &e))
            .ok_or("tarefa não encontrada")?;
        match read_log(&path) {
            Ok((text, truncated)) => Ok(TaskLog { text, truncated }),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Err("o log não existe mais (o Claude apaga ao fechar a sessão)".to_string()),
            Err(e) => Err(format!("não consegui ler o log: {e}")),
        }
    })
    .await
    .map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;

    // Linhas no formato real do histórico (encurtadas e anonimizadas).
    const CALL: &str = r#"{"parentUuid":"08b6","isSidechain":false,"message":{"model":"claude-opus-5-5","id":"msg_1","type":"message","role":"assistant","content":[{"type":"tool_use","id":"toolu_016Q","name":"Bash","input":{"command":"npm run dev > /tmp/dev.log","description":"Sobe o servidor de dev","run_in_background":true},"caller":{"type":"direct"}}],"stop_reason":"tool_use"},"type":"assistant","timestamp":"2026-11-11T17:25:05.100Z"}"#;
    const RESULT: &str = r#"{"parentUuid":"079a","isSidechain":false,"type":"user","message":{"role":"user","content":[{"tool_use_id":"toolu_016Q","type":"tool_result","content":"Command running in background with ID: bc4qdzrtz. Output is being written to: /tmp/claude-1000/-home-user-proj/7bed/tasks/bc4qdzrtz.output. You will be notified when it completes — do not poll that file or sleep while waiting.","is_error":false}]},"uuid":"6c8b","timestamp":"2026-11-11T17:25:06.843Z","toolUseResult":{"stdout":"","stderr":"","interrupted":false,"isImage":false,"noOutputExpected":false,"backgroundTaskId":"bc4qdzrtz"}}"#;
    const DONE: &str = r#"{"type":"queue-operation","operation":"enqueue","timestamp":"2026-11-11T17:25:16.255Z","sessionId":"f62f","content":"<task-notification>\n<task-id>bc4qdzrtz</task-id>\n<tool-use-id>toolu_016Q</tool-use-id>\n<output-file>/tmp/claude-1000/-home-user-proj/7bed/tasks/bc4qdzrtz.output</output-file>\n<status>failed</status>\n<summary>Background command \"Sobe o servidor de dev\" failed with exit code 143</summary>\n</task-notification>"}"#;
    // Bash comum (sem segundo plano) e uma tarefa de agente: ficam de fora.
    const OTHER: &str = r#"{"type":"assistant","message":{"content":[{"type":"tool_use","id":"toolu_2","name":"Bash","input":{"command":"ls","run_in_background":false}}]}}"#;

    fn run(lines: &[&str]) -> Vec<(String, Entry)> {
        let mut s = Scan::default();
        for l in lines {
            s.line(l);
        }
        s.order.iter().rev().map(|id| (id.clone(), s.tasks[id].clone())).collect()
    }

    #[test]
    fn le_a_tarefa_do_historico() {
        let t = run(&[CALL, OTHER, RESULT]);
        assert_eq!(t.len(), 1);
        let (id, e) = &t[0];
        assert_eq!(id, "bc4qdzrtz");
        assert_eq!(e.command, "npm run dev > /tmp/dev.log");
        assert_eq!(e.description, "Sobe o servidor de dev");
        assert_eq!(e.started.as_deref(), Some("2026-11-11T17:25:06.843Z"));
        assert_eq!(e.output.as_deref(), Some("/tmp/claude-1000/-home-user-proj/7bed/tasks/bc4qdzrtz.output"));
        assert_eq!(status(e, true), TaskStatus::Running);
        assert_eq!(status(e, false), TaskStatus::Ended);
        assert!(output_path(id, e).is_some());
    }

    #[test]
    fn le_o_aviso_de_fim() {
        let (_, e) = run(&[CALL, RESULT, DONE]).remove(0);
        assert_eq!((e.finished.as_deref(), e.exit_code), (Some("failed"), Some(143)));
        // Mesmo com o processo vivo, o aviso manda.
        assert_eq!(status(&e, true), TaskStatus::Failed);

        // O aviso também chega como mensagem do usuário, e às vezes antes do resultado.
        let user = r#"{"type":"user","message":{"role":"user","content":"<task-notification>\n<task-id>bc4qdzrtz</task-id>\n<status>completed</status>\n<summary>Background command \"x\" completed (exit code 0)</summary>\n</task-notification>"}}"#;
        let (_, e) = run(&[CALL, user, RESULT]).remove(0);
        assert_eq!(status(&e, false), TaskStatus::Done);

        let killed = DONE.replace("<status>failed</status>", "<status>killed</status>").replace("failed with exit code 143", "was stopped");
        let (_, e) = run(&[CALL, RESULT, &killed]).remove(0);
        assert_eq!(status(&e, false), TaskStatus::Stopped);
    }

    #[test]
    fn so_aceita_log_com_cara_de_log() {
        let entry = |p: &str| Entry { output: Some(p.into()), ..Default::default() };
        assert!(output_path("b1", &entry("/tmp/claude-1000/x/tasks/b1.output")).is_some());
        assert!(output_path("b1", &entry("/tmp/claude-1000/x/tasks/b2.output")).is_none());
        assert!(output_path("b1", &entry("/home/user/.ssh/b1.output")).is_none());
        assert!(output_path("b1", &entry("/tmp/x/../tasks/b1.output")).is_none());
        assert!(output_path("b1", &entry("tasks/b1.output")).is_none());
        assert!(!valid_id("../x"));
    }

    #[test]
    fn limpa_o_log() {
        let raw = "\x1b[32m✓\x1b[0m teste ok\n\x1b]0;título\x07baixando 10%\rbaixando 50%\rbaixando 100%\nfim\r\n";
        assert_eq!(clean_log(raw), "✓ teste ok\nbaixando 100%\nfim\n");
    }

    #[test]
    fn le_o_codigo_de_saida() {
        assert_eq!(exit_code("Background command \"x\" completed (exit code 0)"), Some(0));
        assert_eq!(exit_code("Background command \"x\" failed with exit code 124"), Some(124));
        assert_eq!(exit_code("Background command \"x\" was stopped"), None);
    }
}
