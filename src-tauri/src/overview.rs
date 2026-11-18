//! Visão geral das abas: o que o Claude de cada aba está fazendo agora, tirado
//! do fim do histórico da sessão (a mesma leitura do suggest.rs).

use serde::Serialize;
use serde_json::Value;
use tauri::State;

use crate::{pty::PtyState, suggest};

/// Corta textos longos: o card mostra uma linha, o popup algumas.
const MAX_TEXT: usize = 400;

#[derive(Serialize, Debug, PartialEq)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Activity {
    /// Última fala do Claude.
    Text { text: String },
    /// Ferramenta chamada depois da última fala, com o detalhe mais útil dela.
    Tool { name: String, detail: Option<String> },
}

#[derive(Serialize, Debug, PartialEq)]
pub struct Peek {
    /// Pasta de config do Claude (CLAUDE_CONFIG_DIR ou ~/.claude): diz de qual conta é.
    pub config_dir: Option<String>,
    pub activity: Option<Activity>,
}

fn clip(text: &str) -> String {
    let text = text.trim();
    match text.char_indices().nth(MAX_TEXT) {
        Some((i, _)) => format!("{}…", &text[..i]),
        None => text.to_string(),
    }
}

/// O campo que melhor diz o que a ferramenta está fazendo (comando, arquivo, destino...).
fn tool_detail(name: &str, input: &Value) -> Option<String> {
    let field = |k: &str| input.get(k).and_then(Value::as_str).filter(|s| !s.trim().is_empty());
    let detail = match name {
        "SendMessage" => field("to").map(|to| format!("→ {to}: {}", field("summary").unwrap_or(""))),
        _ => ["description", "command", "file_path", "pattern", "url", "query", "prompt"]
            .iter()
            .find_map(|k| field(k))
            .map(str::to_string),
    }?;
    // Comando de várias linhas: a primeira já diz o que é.
    Some(clip(detail.lines().next().unwrap_or_default()))
}

/// O que veio por último no histórico: uma fala ou uma ferramenta.
pub(crate) fn last_activity(jsonl: &str) -> Option<Activity> {
    let mut last = None;
    for entry in jsonl.lines().filter_map(|l| serde_json::from_str::<Value>(l).ok()) {
        // Subagentes gravam no mesmo arquivo em versões antigas: aparecem como filhos, não aqui.
        if entry.get("isSidechain").and_then(Value::as_bool) == Some(true) {
            continue;
        }
        if entry.get("type").and_then(Value::as_str) != Some("assistant") {
            continue;
        }
        for part in entry.pointer("/message/content").and_then(Value::as_array).into_iter().flatten() {
            match part.get("type").and_then(Value::as_str) {
                Some("text") => {
                    if let Some(text) = part.get("text").and_then(Value::as_str).filter(|t| !t.trim().is_empty()) {
                        last = Some(Activity::Text { text: clip(text) });
                    }
                }
                Some("tool_use") => {
                    let name = part.get("name").and_then(Value::as_str).unwrap_or("ferramenta").to_string();
                    let detail = part.get("input").and_then(|i| tool_detail(&name, i));
                    last = Some(Activity::Tool { name, detail });
                }
                _ => {}
            }
        }
    }
    last
}

fn peek(pid: u32) -> Option<Peek> {
    let proc = suggest::read_proc(pid)?;
    let config_dir = suggest::config_dir(&proc.env).map(|p| p.to_string_lossy().into_owned());
    let activity = suggest::transcript(pid, &proc).and_then(|p| suggest::read_tail(&p)).and_then(|t| last_activity(&t));
    Some(Peek { config_dir, activity })
}

/// O que o Claude da aba está fazendo (None sem Claude na aba).
#[tauri::command]
pub async fn claude_peek(state: State<'_, PtyState>, id: u32) -> Result<Option<Peek>, String> {
    let Some(fg) = crate::pty::foreground_pid(&state, id) else { return Ok(None) };
    tauri::async_runtime::spawn_blocking(move || peek(fg)).await.map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ferramenta_depois_da_fala() {
        let jsonl = concat!(
            r#"{"type":"user","message":{"content":"roda os testes"}}"#,
            "\n",
            r#"{"type":"assistant","message":{"content":[{"type":"text","text":"Vou rodar."}]}}"#,
            "\n",
            r#"{"type":"assistant","message":{"content":[{"type":"tool_use","name":"Bash","input":{"command":"npm test\nnpm run build"}}]}}"#,
        );
        assert_eq!(last_activity(jsonl), Some(Activity::Tool { name: "Bash".into(), detail: Some("npm test".into()) }));
    }

    #[test]
    fn fala_depois_da_ferramenta() {
        let jsonl = concat!(
            r#"{"type":"assistant","message":{"content":[{"type":"tool_use","name":"Read","input":{"file_path":"/x/a.rs"}}]}}"#,
            "\n",
            r#"{"type":"user","message":{"content":[{"type":"tool_result","content":"..."}]}}"#,
            "\n",
            r#"{"type":"assistant","message":{"content":[{"type":"text","text":"Posso rodar o migrate?"}]}}"#,
        );
        assert_eq!(last_activity(jsonl), Some(Activity::Text { text: "Posso rodar o migrate?".into() }));
    }

    #[test]
    fn mensagem_pra_outra_sessao() {
        let jsonl = r#"{"type":"assistant","message":{"content":[{"type":"tool_use","name":"SendMessage","input":{"to":"webhooks","summary":"Tirar excluir conta do escopo","message":"..."}}]}}"#;
        assert_eq!(
            last_activity(jsonl),
            Some(Activity::Tool { name: "SendMessage".into(), detail: Some("→ webhooks: Tirar excluir conta do escopo".into()) })
        );
    }

    #[test]
    fn ignora_subagente_e_linha_cortada() {
        let jsonl = concat!(
            r#"ype":"assistant","message":{"#,
            "\n",
            r#"{"type":"assistant","message":{"content":[{"type":"text","text":"Pronto."}]}}"#,
            "\n",
            r#"{"type":"assistant","isSidechain":true,"message":{"content":[{"type":"text","text":"do subagente"}]}}"#,
        );
        assert_eq!(last_activity(jsonl), Some(Activity::Text { text: "Pronto.".into() }));
    }

    #[test]
    fn corta_texto_longo() {
        let text = "á".repeat(MAX_TEXT + 10);
        assert_eq!(clip(&text).chars().count(), MAX_TEXT + 1);
    }
}
