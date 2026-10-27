mod files;
mod git;
mod pty;
mod session;

pub fn run() {
    tauri::Builder::default()
        .manage(pty::PtyState::default())
        .invoke_handler(tauri::generate_handler![
            pty::pty_spawn,
            pty::pty_write,
            pty::pty_resize,
            pty::pty_kill,
            pty::pty_info,
            git::git_status,
            git::git_ls_files,
            git::git_show_head,
            git::git_branches,
            git::git_checkout,
            git::git_fetch,
            files::read_text_file,
            session::session_load,
            session::session_save,
        ])
        .run(tauri::generate_context!())
        .expect("erro ao iniciar o term7");
}
