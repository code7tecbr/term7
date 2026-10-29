//! Subagentes do Claude da aba, pro menu "Agentes" da barra.
//!
//! O Claude grava cada subagente ao lado do histórico da sessão:
//! `projects/<pasta>/<sessão>/subagents/agent-<id>.meta.json` (tipo, descrição,
//! modelo, se terminou) e `agent-<id>.jsonl` (as mensagens, com o uso de tokens).

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::Mutex,
    time::SystemTime,
};

use serde::Serialize;
use serde_json::Value;
use tauri::State;

use crate::{pty::PtyState, suggest};

/// Sem resposta nova há esse tempo e sem "terminou": o agente foi interrompido.
const STALLED_SECS: u64 = 10 * 60;
const MAX_AGENTS: usize = 60;

#[derive(Serialize, Clone, Copy, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum AgentStatus {
    Running,
    Done,
    Stalled,
}

#[derive(Serialize, Clone)]
pub struct Agent {
    id: String,
    kind: String,
    description: String,
    model: Option<String>,
    background: bool,
    status: AgentStatus,
    /// ISO 8601, como o Claude grava.
    started: Option<String>,
    updated: Option<String>,
    /// Tamanho do contexto na última resposta (entrada + cache + saída).
    context_tokens: u64,
    output_tokens: u64,
    tool_calls: u32,
    last_tool: Option<String>,
}

/// O que sai do .jsonl; o arquivo só cresce, então guarda por tamanho e data.
#[derive(Clone, Default)]
struct Parsed {
    model: Option<String>,
    started: Option<String>,
    updated: Option<String>,
    context_tokens: u64,
    output_tokens: u64,
    tool_calls: u32,
    last_tool: Option<String>,
    /// A última resposta acabou sem pedir ferramenta: o agente respondeu.
    ended: bool,
}

static CACHE: Mutex<Option<HashMap<PathBuf, (SystemTime, u64, Parsed)>>> = Mutex::new(None);

fn tokens(usage: &Value, key: &str) -> u64 {
    usage.get(key).and_then(Value::as_u64).unwrap_or(0)
}

fn parse(jsonl: &str) -> Parsed {
    let mut p = Parsed::default();
    for line in jsonl.lines() {
        let Ok(d) = serde_json::from_str::<Value>(line) else { continue };
        if let Some(ts) = d.get("timestamp").and_then(Value::as_str) {
            if p.started.is_none() {
                p.started = Some(ts.to_string());
            }
            p.updated = Some(ts.to_string());
        }
        if d.get("type").and_then(Value::as_str) != Some("assistant") {
            continue;
        }
        let Some(msg) = d.get("message") else { continue };
        if let Some(m) = msg.get("model").and_then(Value::as_str) {
            p.model = Some(m.to_string());
        }
        if let Some(u) = msg.get("usage") {
            let input = tokens(u, "input_tokens") + tokens(u, "cache_read_input_tokens") + tokens(u, "cache_creation_input_tokens");
            let output = tokens(u, "output_tokens");
            p.context_tokens = input + output;
            p.output_tokens += output;
        }
        for c in msg.get("content").and_then(Value::as_array).into_iter().flatten() {
            if c.get("type").and_then(Value::as_str) == Some("tool_use") {
                p.tool_calls += 1;
                p.last_tool = c.get("name").and_then(Value::as_str).map(str::to_string);
            }
        }
        p.ended = msg.get("stop_reason").and_then(Value::as_str) == Some("end_turn");
    }
    p
}

fn parsed(path: &Path) -> Option<(Parsed, SystemTime)> {
    let meta = std::fs::metadata(path).ok()?;
    let (mtime, len) = (meta.modified().unwrap_or(SystemTime::UNIX_EPOCH), meta.len());
    let mut cache = CACHE.lock().unwrap();
    let cache = cache.get_or_insert_with(HashMap::new);
    if let Some((t, l, p)) = cache.get(path) {
        if *t == mtime && *l == len {
            return Some((p.clone(), mtime));
        }
    }
    let p = parse(&std::fs::read_to_string(path).ok()?);
    cache.insert(path.to_path_buf(), (mtime, len, p.clone()));
    Some((p, mtime))
}

fn agent(meta_path: &Path) -> Option<Agent> {
    let name = meta_path.file_name()?.to_str()?;
    let id = name.strip_prefix("agent-")?.strip_suffix(".meta.json")?.to_string();
    let meta: Value = serde_json::from_str(&std::fs::read_to_string(meta_path).ok()?).ok()?;
    let text = |k: &str| meta.get(k).and_then(Value::as_str).map(str::to_string);
    let (p, mtime) = parsed(&meta_path.with_file_name(format!("agent-{id}.jsonl"))).unwrap_or((Parsed::default(), SystemTime::UNIX_EPOCH));

    let finished = meta.get("finished").and_then(Value::as_bool).unwrap_or(false);
    let idle = SystemTime::now().duration_since(mtime).map(|d| d.as_secs()).unwrap_or(0);
    let status = if finished || p.ended {
        AgentStatus::Done
    } else if idle > STALLED_SECS {
        AgentStatus::Stalled
    } else {
        AgentStatus::Running
    };
    Some(Agent {
        id,
        kind: text("agentType").unwrap_or_else(|| "agente".into()),
        description: text("description").unwrap_or_default(),
        // O .jsonl tem o modelo exato; o meta só o apelido ("sonnet").
        model: p.model.or_else(|| text("model")),
        background: text("requestShape").as_deref() == Some("background"),
        status,
        started: p.started,
        updated: p.updated,
        context_tokens: p.context_tokens,
        output_tokens: p.output_tokens,
        tool_calls: p.tool_calls,
        last_tool: p.last_tool,
    })
}

fn agents(pid: u32) -> Vec<Agent> {
    let Some(proc) = suggest::read_proc(pid) else { return Vec::new() };
    let Some(transcript) = suggest::transcript(pid, &proc) else { return Vec::new() };
    let Ok(entries) = std::fs::read_dir(transcript.with_extension("").join("subagents")) else { return Vec::new() };
    let mut list: Vec<Agent> = entries
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.to_str().is_some_and(|s| s.ends_with(".meta.json")))
        .filter_map(|p| agent(&p))
        .collect();
    // Rodando primeiro; dentro de cada grupo, o mais recente em cima.
    list.sort_by(|a, b| {
        (a.status != AgentStatus::Running).cmp(&(b.status != AgentStatus::Running)).then_with(|| b.started.cmp(&a.started))
    });
    list.truncate(MAX_AGENTS);
    list
}

/// Subagentes da sessão do Claude rodando na aba (vazio sem Claude).
#[tauri::command]
pub async fn claude_agents(state: State<'_, PtyState>, id: u32) -> Result<Vec<Agent>, String> {
    let Some(fg) = crate::pty::foreground_pid(&state, id) else { return Ok(Vec::new()) };
    tauri::async_runtime::spawn_blocking(move || agents(fg)).await.map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn le_o_historico() {
        let jsonl = r#"{"type":"user","timestamp":"2026-10-14T17:32:18Z"}
{"type":"assistant","timestamp":"2026-10-14T17:32:20Z","message":{"model":"claude-sonnet-5-5","stop_reason":"tool_use","usage":{"input_tokens":2,"cache_read_input_tokens":1000,"output_tokens":50},"content":[{"type":"tool_use","name":"Read"}]}}
{"type":"assistant","timestamp":"2026-10-14T17:32:48Z","message":{"model":"claude-sonnet-5-5","stop_reason":"end_turn","usage":{"input_tokens":3,"cache_read_input_tokens":2000,"output_tokens":100},"content":[{"type":"text","text":"ok"}]}}"#;
        let p = parse(jsonl);
        assert_eq!(p.model.as_deref(), Some("claude-sonnet-5-5"));
        assert_eq!(p.started.as_deref(), Some("2026-10-14T17:32:18Z"));
        assert_eq!(p.updated.as_deref(), Some("2026-10-14T17:32:48Z"));
        assert_eq!((p.context_tokens, p.output_tokens, p.tool_calls), (2103, 150, 1));
        assert_eq!(p.last_tool.as_deref(), Some("Read"));
        assert!(p.ended);
    }
}
