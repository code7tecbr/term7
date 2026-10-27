//! Salva/restaura o layout (abas, grupos, pastas) em <app_data_dir>/session.json.
//! O conteúdo é um JSON opaco montado pelo frontend.

use std::{fs, path::PathBuf};

use tauri::{AppHandle, Manager};

fn session_path(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir.join("session.json"))
}

#[tauri::command]
pub fn session_load(app: AppHandle) -> Option<String> {
    fs::read_to_string(session_path(&app).ok()?).ok()
}

#[tauri::command]
pub fn session_save(app: AppHandle, data: String) -> Result<(), String> {
    let path = session_path(&app)?;
    // Escreve num temporário e renomeia: nunca fica um arquivo pela metade.
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, data).map_err(|e| e.to_string())?;
    fs::rename(tmp, path).map_err(|e| e.to_string())
}
