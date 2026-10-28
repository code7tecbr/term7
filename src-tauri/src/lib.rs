mod claude;
mod files;
mod gh;
mod git;
mod pty;
mod session;
mod update;

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
            git::git_repo_info,
            gh::gh_pr,
            gh::gh_runs,
            gh::open_url,
            claude::claude_hooks_status,
            claude::claude_hooks_install,
            claude::claude_mcp_list,
            files::read_text_file,
            files::resolve_paths,
            session::session_load,
            session::session_save,
            update::update_check,
            update::update_install,
            update::update_restart,
        ])
        .run(tauri::generate_context!())
        .expect("erro ao iniciar o term7");
}
