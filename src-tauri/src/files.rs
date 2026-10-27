//! Leitura de arquivos pro visualizador.

use serde::Serialize;

/// Acima disso o visualizador não abre (fica lento e não serve pra ler).
const MAX_FILE_BYTES: u64 = 4 * 1024 * 1024;

#[derive(Serialize)]
pub struct FileContent {
    text: String,
    binary: bool,
}

/// Texto do arquivo, ou `binary: true` se tiver bytes nulos (imagem, executável...).
pub fn to_content(bytes: Vec<u8>) -> FileContent {
    if bytes.iter().take(8000).any(|&b| b == 0) {
        return FileContent { text: String::new(), binary: true };
    }
    FileContent { text: String::from_utf8_lossy(&bytes).into_owned(), binary: false }
}

fn read(path: &str) -> Result<FileContent, String> {
    let meta = std::fs::metadata(path).map_err(|e| e.to_string())?;
    if meta.len() > MAX_FILE_BYTES {
        return Err("Arquivo muito grande para visualizar".into());
    }
    std::fs::read(path).map(to_content).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn read_text_file(path: String) -> Result<FileContent, String> {
    tauri::async_runtime::spawn_blocking(move || read(&path))
        .await
        .map_err(|e| e.to_string())?
}

// ---- caminhos na saída do terminal ----

/// Resolve caminhos vistos na saída do terminal: `~/x`, absolutos, ou relativos
/// à pasta da aba. Devolve o caminho absoluto só se for um arquivo que existe
/// (o link não aparece em texto que só parece caminho).
fn resolve(cwd: Option<&str>, candidate: &str) -> Option<String> {
    let path = if let Some(rest) = candidate.strip_prefix("~/") {
        std::path::PathBuf::from(std::env::var_os("HOME")?).join(rest)
    } else if candidate.starts_with('/') {
        std::path::PathBuf::from(candidate)
    } else {
        std::path::Path::new(cwd?).join(candidate)
    };
    std::fs::metadata(&path)
        .ok()
        .filter(|m| m.is_file())
        .map(|_| path.to_string_lossy().into_owned())
}

#[tauri::command]
pub async fn resolve_paths(cwd: Option<String>, candidates: Vec<String>) -> Result<Vec<Option<String>>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        candidates.iter().map(|c| resolve(cwd.as_deref(), c)).collect()
    })
    .await
    .map_err(|e| e.to_string())
}
