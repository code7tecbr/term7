//! Atualização do term7 pela própria interface: consulta a última release no
//! GitHub (via `gh`, o repositório é privado), baixa o .deb e instala com
//! `pkexec apt install`, que mostra a janela de senha do sistema.

use std::{
    path::PathBuf,
    process::{Command, Stdio},
};

use serde::Serialize;

const REPO: &str = "code7tecbr/term7";

#[derive(Serialize)]
pub struct UpdateInfo {
    /// Versão da última release, sem o "v".
    latest: String,
    /// Se é mais nova que a que está rodando.
    newer: bool,
    /// O term7 rodando é o do apt (/usr/bin). O build de teste em ~/.local
    /// não seria trocado pelo apt, então não oferece o botão.
    installed: bool,
}

fn gh(args: &[&str]) -> Result<String, String> {
    let out = Command::new("gh")
        .args(args)
        .env("GH_PROMPT_DISABLED", "1")
        .env("GH_NO_UPDATE_NOTIFIER", "1")
        .env("NO_COLOR", "1")
        .stdin(Stdio::null())
        .output()
        .map_err(|e| e.to_string())?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
}

/// "1.10.0" > "1.9.2": compara número a número, não como texto.
fn is_newer(latest: &str, current: &str) -> bool {
    let parse = |v: &str| -> Vec<u64> { v.split(['.', '-']).map_while(|p| p.parse().ok()).collect() };
    parse(latest) > parse(current)
}

fn running_from_apt() -> bool {
    std::env::current_exe().is_ok_and(|exe| exe.starts_with("/usr/"))
}

#[tauri::command]
pub async fn update_check(app: tauri::AppHandle) -> Result<UpdateInfo, String> {
    let current = app.package_info().version.to_string();
    tauri::async_runtime::spawn_blocking(move || {
        // `gh api` com --jq existe no gh 2.4; `release view --json` não.
        let tag = gh(&["api", &format!("repos/{REPO}/releases/latest"), "--jq", ".tag_name"])?;
        let latest = tag.trim_start_matches('v').to_string();
        if latest.is_empty() {
            return Err("nenhuma release encontrada".into());
        }
        Ok(UpdateInfo { newer: is_newer(&latest, &current), latest, installed: running_from_apt() })
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Baixa o .deb da versão e instala. O term7 aberto continua rodando a versão
/// antiga até reiniciar.
#[tauri::command]
pub async fn update_install(version: String) -> Result<(), String> {
    if !version.chars().all(|c| c.is_ascii_digit() || c == '.') {
        return Err("versão inválida".into());
    }
    tauri::async_runtime::spawn_blocking(move || {
        let dir = std::env::temp_dir().join(format!("term7-update-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let deb = format!("term7_{version}_amd64.deb");
        let dir_str = dir.to_string_lossy();
        gh(&["release", "download", &format!("v{version}"), "--repo", REPO, "--pattern", &deb, "--dir", &dir_str])
            .map_err(|e| format!("não consegui baixar o .deb: {e}"))?;
        let path: PathBuf = dir.join(&deb);

        let out = Command::new("pkexec")
            .args(["apt-get", "install", "-y", "--allow-downgrades"])
            .arg(&path)
            .stdin(Stdio::null())
            .output()
            .map_err(|e| e.to_string());
        let _ = std::fs::remove_dir_all(&dir);
        let out = out?;
        match out.status.code() {
            Some(0) => Ok(()),
            // 126/127: a janela de senha foi cancelada ou recusada.
            Some(126 | 127) => Err("instalação cancelada".into()),
            _ => {
                let err = String::from_utf8_lossy(&out.stderr);
                let last = err.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("falhou");
                Err(format!("o apt falhou: {}", last.trim()))
            }
        }
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Reinicia o term7 pra usar a versão nova (o layout das abas é restaurado).
#[tauri::command]
pub fn update_restart(app: tauri::AppHandle) {
    app.restart();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compara_versoes() {
        assert!(is_newer("1.10.0", "1.9.2"));
        assert!(is_newer("1.6.0", "1.5.0"));
        assert!(!is_newer("1.5.0", "1.5.0"));
        assert!(!is_newer("1.4.0", "1.5.0"));
    }
}
