//! Cópia pedida por programa rodando no terminal (OSC 52, ex.: Claude CLI).
//!
//! No WebKitGTK, `navigator.clipboard.writeText` só funciona logo depois de um
//! clique ou tecla; o OSC 52 chega depois, pela saída do PTY, e era recusado.
//! O GTK grava em nome da janela, sem essa restrição.

use tauri::AppHandle;

#[tauri::command]
pub fn clipboard_write(app: AppHandle, text: String) -> Result<(), String> {
    if text.is_empty() {
        return Ok(());
    }
    // O GTK só pode ser usado na thread principal.
    app.run_on_main_thread(move || {
        let clipboard = gtk::Clipboard::get(&gtk::gdk::SELECTION_CLIPBOARD);
        clipboard.set_text(&text);
        clipboard.store();
    })
    .map_err(|e| e.to_string())
}
