mod agents;
mod clipboard;
mod definition;
mod claude;
mod files;
mod gh;
mod git;
mod pty;
mod session;
mod settings;
mod suggest;
mod update;
mod usage;
mod voice;

pub fn run() {
    tauri::Builder::default()
        .manage(pty::PtyState::default())
        .manage(voice::VoiceState::default())
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
            usage::claude_usage,
            usage::claude_usage_setup,
            suggest::claude_suggestions,
            agents::claude_agents,
            agents::claude_agent_steps,
            definition::find_definitions,
            definition::resolve_import_path,
            files::read_text_file,
            files::resolve_paths,
            files::pick_files,
            settings::settings_load,
            settings::settings_save,
            voice::voice_start,
            voice::voice_stop,
            voice::voice_cancel,
            session::session_load,
            session::session_save,
            update::update_check,
            update::update_install,
            update::update_restart,
            clipboard::clipboard_write,
        ])
        .run(tauri::generate_context!())
        .expect("erro ao iniciar o term7");
}
