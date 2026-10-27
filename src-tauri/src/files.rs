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
