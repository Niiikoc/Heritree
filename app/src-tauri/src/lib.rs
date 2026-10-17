//! Tauri shell: exposes heritree-core to the web UI as commands.
//!
//! Every command runs on a blocking worker thread, so a slow repository never
//! freezes the window.

use std::path::PathBuf;
use std::sync::Mutex;

use heritree_core::{CommitInfo, FileStatus, GraphRow, RefInfo, RepoInfo, Repository, graph};
use serde::Serialize;
use tauri::State;

/// Errors reach the UI as plain messages.
type CmdResult<T> = Result<T, String>;

#[derive(Default)]
struct AppState {
    /// Working tree (or git dir) of the open repository.
    repo_path: Mutex<Option<PathBuf>>,
}

impl AppState {
    fn current(&self) -> CmdResult<PathBuf> {
        self.repo_path
            .lock()
            .unwrap()
            .clone()
            .ok_or_else(|| "No repository is open".to_owned())
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct LogRow {
    commit: CommitInfo,
    graph: GraphRow,
}

/// Runs `f` against the open repository on a blocking thread.
async fn with_repo<T, F>(state: &AppState, f: F) -> CmdResult<T>
where
    T: Send + 'static,
    F: FnOnce(&Repository) -> heritree_core::Result<T> + Send + 'static,
{
    let path = state.current()?;
    tauri::async_runtime::spawn_blocking(move || {
        let repo = Repository::open(&path).map_err(|e| e.to_string())?;
        f(&repo).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn open_repository(path: String, state: State<'_, AppState>) -> CmdResult<RepoInfo> {
    let info = tauri::async_runtime::spawn_blocking(move || Repository::open(&path)?.info())
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())?;
    let root = info.workdir.clone().unwrap_or_else(|| info.git_dir.clone());
    *state.repo_path.lock().unwrap() = Some(PathBuf::from(root));
    Ok(info)
}

#[tauri::command]
async fn repo_info(state: State<'_, AppState>) -> CmdResult<RepoInfo> {
    with_repo(&state, |repo| repo.info()).await
}

#[tauri::command]
async fn list_refs(state: State<'_, AppState>) -> CmdResult<Vec<RefInfo>> {
    with_repo(&state, |repo| repo.refs()).await
}

#[tauri::command]
async fn load_log(limit: usize, state: State<'_, AppState>) -> CmdResult<Vec<LogRow>> {
    with_repo(&state, move |repo| {
        let commits = repo.log(limit)?;
        let rows = graph::layout(&commits);
        Ok(commits
            .into_iter()
            .zip(rows)
            .map(|(commit, graph)| LogRow { commit, graph })
            .collect())
    })
    .await
}

#[tauri::command]
async fn working_tree_status(state: State<'_, AppState>) -> CmdResult<Vec<FileStatus>> {
    with_repo(&state, |repo| repo.status()).await
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![
            open_repository,
            repo_info,
            list_refs,
            load_log,
            working_tree_status
        ])
        .run(tauri::generate_context!())
        .expect("error while running Heritree");
}
