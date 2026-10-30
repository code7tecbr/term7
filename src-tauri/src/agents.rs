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

fn subagents_dir(pid: u32) -> Option<PathBuf> {
    let proc = suggest::read_proc(pid)?;
    Some(suggest::transcript(pid, &proc)?.with_extension("").join("subagents"))
}

fn agents(pid: u32) -> Vec<Agent> {
    let Some(dir) = subagents_dir(pid) else { return Vec::new() };
    let Ok(entries) = std::fs::read_dir(dir) else { return Vec::new() };
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

/// Um passo do agente, pro popup de detalhe.
#[derive(Serialize, Clone)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Step {
    /// O pedido que o agente recebeu (no chat, cada mensagem do usuário).
    Prompt { text: String },
    Text { text: String },
    Tool {
        name: String,
        /// O essencial do pedido: a descrição do comando, o arquivo, o padrão buscado.
        summary: String,
        /// O comando completo, quando o resumo é só a descrição dele.
        detail: Option<String>,
        /// Começo do resultado; None enquanto a ferramenta roda.
        result: Option<String>,
        error: bool,
    },
}

const MAX_STEPS: usize = 300;
// Os relatórios finais dos agentes passam fácil de 4 mil caracteres.
const MAX_TEXT: usize = 30_000;
const MAX_RESULT: usize = 800;

fn clip(s: &str, max: usize) -> String {
    let s = s.trim();
    if s.chars().count() <= max {
        return s.to_string();
    }
    let mut out: String = s.chars().take(max).collect();
    out.push_str(" …");
    out
}

/// Caminhos dentro da pasta do agente ficam relativos: numa worktree, o prefixo
/// absoluto ocupava a linha toda e escondia o arquivo.
fn relative(text: &str, cwd: &str) -> String {
    let mut out = text.to_string();
    if cwd.len() > 1 {
        out = out.replace(&format!("{}/", cwd.trim_end_matches('/')), "");
    }
    // Fora da pasta do agente, a pasta pessoal vira "~".
    match std::env::var("HOME") {
        Ok(home) if home.len() > 1 => out.replace(&format!("{}/", home.trim_end_matches('/')), "~/"),
        _ => out,
    }
}

/// O campo que diz o que a ferramenta foi fazer; nas desconhecidas, o JSON todo.
/// No Bash, a descrição dada pelo agente; o comando vai no detalhe.
fn tool_summary(name: &str, input: &Value, cwd: &str) -> (String, Option<String>) {
    let field = |k: &str| input.get(k).and_then(Value::as_str);
    if name == "Bash" {
        let command = field("command").map(|c| clip(&relative(c, cwd), 2000));
        if let Some(desc) = field("description").filter(|d| !d.trim().is_empty()) {
            return (clip(desc, 600), command);
        }
        return (command.unwrap_or_default(), None);
    }
    let main = match name {
        "Read" | "Edit" | "Write" | "NotebookEdit" => field("file_path").or(field("notebook_path")),
        "Grep" | "Glob" => field("pattern"),
        "WebFetch" => field("url"),
        "WebSearch" => field("query"),
        "Agent" | "Task" => field("description"),
        "Skill" => field("skill"),
        _ => None,
    };
    (clip(&relative(&main.map(str::to_string).unwrap_or_else(|| input.to_string()), cwd), 600), None)
}

fn result_text(content: &Value) -> String {
    match content {
        Value::String(s) => s.clone(),
        Value::Array(items) => items
            .iter()
            .map(|i| match i.get("type").and_then(Value::as_str) {
                Some("text") => i.get("text").and_then(Value::as_str).unwrap_or("").to_string(),
                Some(other) => format!("[{other}]"),
                None => String::new(),
            })
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    }
}

/// Texto escrito pelo usuário; os `<system-reminder>`, `<command-name>` etc. são do próprio Claude.
fn user_text(content: &Value) -> Option<String> {
    let text = match content {
        Value::String(s) => s.clone(),
        Value::Array(items) => items
            .iter()
            .filter(|i| i.get("type").and_then(Value::as_str) == Some("text"))
            .filter_map(|i| i.get("text").and_then(Value::as_str))
            .filter(|t| !t.trim_start().starts_with('<'))
            .collect::<Vec<_>>()
            .join("\n"),
        _ => return None,
    };
    (!text.trim().is_empty() && !text.trim_start().starts_with('<')).then_some(text)
}

pub(crate) fn steps(jsonl: &str) -> Vec<Step> {
    let mut out: Vec<Step> = Vec::new();
    // tool_use_id -> posição do passo, pra encaixar o resultado que vem depois.
    let mut pending: HashMap<String, usize> = HashMap::new();
    let mut cwd = String::new();
    for line in jsonl.lines() {
        let Ok(d) = serde_json::from_str::<Value>(line) else { continue };
        if let Some(c) = d.get("cwd").and_then(Value::as_str) {
            cwd = c.to_string();
        }
        let Some(content) = d.pointer("/message/content") else { continue };
        match d.get("type").and_then(Value::as_str) {
            Some("user") => {
                // Mensagens geradas pelo Claude (resumo de compactação, saída de comando) não são do usuário.
                let meta = d.get("isMeta").and_then(Value::as_bool).unwrap_or(false) || d.get("isCompactSummary").and_then(Value::as_bool).unwrap_or(false);
                if let Some(text) = user_text(content).filter(|_| !meta) {
                    out.push(Step::Prompt { text: clip(&text, MAX_TEXT) });
                }
                if let Value::Array(items) = content {
                    for i in items {
                        if i.get("type").and_then(Value::as_str) != Some("tool_result") {
                            continue;
                        }
                        let id = i.get("tool_use_id").and_then(Value::as_str).unwrap_or("");
                        if let Some(Step::Tool { result, error, .. }) = pending.remove(id).and_then(|n| out.get_mut(n)) {
                            *result = Some(clip(&result_text(i.get("content").unwrap_or(&Value::Null)), MAX_RESULT));
                            *error = i.get("is_error").and_then(Value::as_bool).unwrap_or(false);
                        }
                    }
                }
            }
            Some("assistant") => {
                for c in content.as_array().into_iter().flatten() {
                    match c.get("type").and_then(Value::as_str) {
                        Some("text") => {
                            let text = c.get("text").and_then(Value::as_str).unwrap_or("");
                            if !text.trim().is_empty() {
                                out.push(Step::Text { text: clip(text, MAX_TEXT) });
                            }
                        }
                        Some("tool_use") => {
                            let name = c.get("name").and_then(Value::as_str).unwrap_or("?").to_string();
                            let (summary, detail) = tool_summary(&name, c.get("input").unwrap_or(&Value::Null), &cwd);
                            if let Some(id) = c.get("id").and_then(Value::as_str) {
                                pending.insert(id.to_string(), out.len());
                            }
                            out.push(Step::Tool { name, summary, detail, result: None, error: false });
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }
    // Mantém o pedido e os passos mais recentes.
    if out.len() > MAX_STEPS {
        let has_prompt = matches!(out.first(), Some(Step::Prompt { .. }));
        let cut = out.len() - MAX_STEPS + usize::from(has_prompt);
        out.drain(usize::from(has_prompt)..cut);
    }
    out
}

/// Linha do tempo de um subagente da aba.
#[tauri::command]
pub async fn claude_agent_steps(state: State<'_, PtyState>, id: u32, agent: String) -> Result<Vec<Step>, String> {
    // O id vira nome de arquivo: nada de "/" ou "..".
    if agent.is_empty() || !agent.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
        return Err("agente inválido".into());
    }
    let Some(fg) = crate::pty::foreground_pid(&state, id) else { return Ok(Vec::new()) };
    tauri::async_runtime::spawn_blocking(move || {
        let Some(dir) = subagents_dir(fg) else { return Ok(Vec::new()) };
        match std::fs::read_to_string(dir.join(format!("agent-{agent}.jsonl"))) {
            Ok(text) => Ok(steps(&text)),
            Err(e) => Err(format!("não consegui ler o histórico do agente: {e}")),
        }
    })
    .await
    .map_err(|e| e.to_string())?
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

    #[test]
    fn monta_os_passos() {
        let jsonl = r#"{"type":"user","message":{"content":"Revise o código"}}
{"type":"user","message":{"content":"<system-reminder>x</system-reminder>"}}
{"type":"user","isMeta":true,"message":{"content":"Caveat: gerado pelo Claude"}}
{"type":"assistant","cwd":"/proj","message":{"content":[{"type":"thinking","thinking":""},{"type":"text","text":"Vou ler."},{"type":"tool_use","id":"t1","name":"Read","input":{"file_path":"/proj/src/a.rs"}}]}}
{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"t1","is_error":true,"content":[{"type":"text","text":"não existe"}]}]}}
{"type":"assistant","message":{"content":[{"type":"tool_use","id":"t2","name":"Bash","input":{"command":"ls /proj/src","description":"Lista os fontes"}}]}}"#;
        let s = steps(jsonl);
        assert_eq!(s.len(), 4);
        assert!(matches!(&s[0], Step::Prompt { text } if text == "Revise o código"));
        assert!(matches!(&s[1], Step::Text { text } if text == "Vou ler."));
        assert!(matches!(&s[2], Step::Tool { name, summary, result: Some(r), error: true, .. } if name == "Read" && summary == "src/a.rs" && r == "não existe"));
        assert!(matches!(&s[3], Step::Tool { summary, detail: Some(d), result: None, .. } if summary == "Lista os fontes" && d == "ls src"));
    }
}
