//! PR e actions da branch atual via `gh` (GitHub CLI), que já cuida do login
//! e de achar o repositório a partir dos remotes.

use std::process::{Command, Stdio};

use serde::Serialize;

// reviewDecision, reviewRequests e latestReviews já existem no gh 2.4.
const PR_FIELDS: &str =
    "number,title,state,isDraft,url,statusCheckRollup,reviewDecision,reviewRequests,latestReviews";
// Só campos que existem no gh 2.4 (o do Ubuntu 22.04), que também não tem `run list --branch`.
const RUN_FIELDS: &str = "databaseId,name,status,conclusion,url,headSha,headBranch,event";

/// Resultado de uma consulta ao `gh`. O JSON vai cru: quem interpreta é o frontend.
#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum GhResult {
    /// `gh` não instalado.
    Missing,
    /// Instalado, mas sem `gh auth login`.
    Unauthenticated,
    /// Nada encontrado (sem PR pra branch, ou o remote não é do GitHub).
    None,
    Ok { json: String },
    Error { message: String },
}

fn gh(dir: &str, args: &[&str]) -> GhResult {
    let out = Command::new("gh")
        .args(args)
        .current_dir(dir)
        // Nunca pergunta nada: não há terminal pra responder.
        .env("GH_PROMPT_DISABLED", "1")
        .env("GH_NO_UPDATE_NOTIFIER", "1")
        .env("NO_COLOR", "1")
        .env("GIT_OPTIONAL_LOCKS", "0")
        .stdin(Stdio::null())
        .output();
    let out = match out {
        Ok(out) => out,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return GhResult::Missing,
        Err(e) => return GhResult::Error { message: e.to_string() },
    };
    if out.status.success() {
        return GhResult::Ok { json: String::from_utf8_lossy(&out.stdout).into_owned() };
    }
    let err = String::from_utf8_lossy(&out.stderr);
    // O gh não tem códigos de saída distintos pra esses casos; só a mensagem.
    if err.contains("gh auth login") {
        GhResult::Unauthenticated
    } else if err.contains("no pull requests found")
        || err.contains("none of the git remotes")
        || err.contains("no git remotes")
    {
        GhResult::None
    } else {
        GhResult::Error { message: err.trim().to_string() }
    }
}

#[tauri::command]
pub async fn gh_pr(root: String) -> Result<GhResult, String> {
    tauri::async_runtime::spawn_blocking(move || gh(&root, &["pr", "view", "--json", PR_FIELDS]))
        .await
        .map_err(|e| e.to_string())
}

/// Últimas execuções de actions do repositório (pra branch sem PR, como a main).
/// O filtro por branch fica com o frontend.
#[tauri::command]
pub async fn gh_runs(root: String) -> Result<GhResult, String> {
    tauri::async_runtime::spawn_blocking(move || {
        gh(&root, &["run", "list", "--limit", "30", "--json", RUN_FIELDS])
    })
    .await
    .map_err(|e| e.to_string())
}

/// Abre um link no navegador padrão.
#[tauri::command]
pub fn open_url(url: String) -> Result<(), String> {
    // Só links web: o xdg-open também abriria arquivos e executáveis locais.
    if !url.starts_with("https://") && !url.starts_with("http://") {
        return Err("só links http(s) podem ser abertos".into());
    }
    let mut child = Command::new("xdg-open")
        .arg(&url)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| e.to_string())?;
    // Espera em outra thread só pra não deixar processo zumbi.
    std::thread::spawn(move || child.wait());
    Ok(())
}
