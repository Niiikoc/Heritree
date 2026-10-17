//! Tauri shell: exposes heritree-core to the web UI as commands.
//!
//! Every command names the repository it works on, so each tab in the UI is
//! independent, and every command runs on a blocking worker thread so a slow
//! repository or network never freezes the window.

use std::path::{Path, PathBuf};

use heritree_core::{
    BlameLine, BranchLineage, CommandRecord, CommitDetails, CommitInfo, ConflictSide, FileDiff,
    FileStatus, GraphRow, LineRef, RefInfo, RepoInfo, Repository, ResetMode, StashEntry, Undo,
    UndoAction, graph,
};
use serde::Serialize;

/// Errors reach the UI as plain messages.
type CmdResult<T> = Result<T, String>;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct LogRow {
    commit: CommitInfo,
    graph: GraphRow,
}

/// Opens `repo` and runs `f` on a blocking thread.
async fn with_repo<T, F>(repo: String, f: F) -> CmdResult<T>
where
    T: Send + 'static,
    F: FnOnce(&Repository) -> heritree_core::Result<T> + Send + 'static,
{
    blocking(move || f(&Repository::open(&repo)?)).await
}

async fn blocking<T, F>(f: F) -> CmdResult<T>
where
    T: Send + 'static,
    F: FnOnce() -> heritree_core::Result<T> + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(f)
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())
}

// ---- Reading ------------------------------------------------------------------

#[tauri::command]
async fn open_repository(path: String) -> CmdResult<RepoInfo> {
    with_repo(path, |r| r.info()).await
}

#[tauri::command]
async fn repo_info(repo: String) -> CmdResult<RepoInfo> {
    with_repo(repo, |r| r.info()).await
}

#[tauri::command]
async fn list_refs(repo: String) -> CmdResult<Vec<RefInfo>> {
    with_repo(repo, |r| r.refs()).await
}

#[tauri::command]
async fn load_log(repo: String, limit: usize) -> CmdResult<Vec<LogRow>> {
    with_repo(repo, move |r| {
        let commits = r.log(limit)?;
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
async fn working_tree_status(repo: String) -> CmdResult<Vec<FileStatus>> {
    with_repo(repo, |r| r.status()).await
}

#[tauri::command]
async fn file_diff(
    repo: String,
    path: String,
    old_path: Option<String>,
    staged: bool,
    untracked: bool,
) -> CmdResult<FileDiff> {
    with_repo(repo, move |r| {
        r.file_diff(&path, old_path.as_deref(), staged, untracked)
    })
    .await
}

#[tauri::command]
async fn commit_details(repo: String, id: String) -> CmdResult<CommitDetails> {
    with_repo(repo, move |r| r.commit_details(&id)).await
}

#[tauri::command]
async fn commit_file_diff(
    repo: String,
    id: String,
    path: String,
    old_path: Option<String>,
) -> CmdResult<FileDiff> {
    with_repo(repo, move |r| {
        r.commit_file_diff(&id, &path, old_path.as_deref())
    })
    .await
}

#[tauri::command]
async fn file_history(repo: String, path: String, limit: usize) -> CmdResult<Vec<CommitInfo>> {
    with_repo(repo, move |r| r.file_history(&path, limit)).await
}

#[tauri::command]
async fn blame(repo: String, path: String) -> CmdResult<Vec<BlameLine>> {
    with_repo(repo, move |r| r.blame(&path)).await
}

#[tauri::command]
async fn branches_containing(repo: String, id: String) -> CmdResult<Vec<String>> {
    with_repo(repo, move |r| r.branches_containing(&id)).await
}

#[tauri::command]
async fn lineage(repo: String) -> CmdResult<Vec<BranchLineage>> {
    with_repo(repo, |r| r.lineage()).await
}

#[tauri::command]
async fn stashes(repo: String) -> CmdResult<Vec<StashEntry>> {
    with_repo(repo, |r| r.stashes()).await
}

#[tauri::command]
async fn remotes(repo: String) -> CmdResult<Vec<String>> {
    with_repo(repo, |r| r.remotes()).await
}

#[tauri::command]
async fn commit_template(repo: String) -> CmdResult<Option<String>> {
    with_repo(repo, |r| Ok(r.commit_template())).await
}

#[tauri::command]
fn command_log() -> Vec<CommandRecord> {
    heritree_core::command_log()
}

// ---- Working tree -----------------------------------------------------------

#[tauri::command]
async fn stage(repo: String, paths: Vec<String>) -> CmdResult<()> {
    with_repo(repo, move |r| r.stage(&paths)).await
}

#[tauri::command]
async fn unstage(repo: String, paths: Vec<String>) -> CmdResult<()> {
    with_repo(repo, move |r| r.unstage(&paths)).await
}

#[tauri::command]
async fn discard(repo: String, paths: Vec<String>) -> CmdResult<Undo> {
    with_repo(repo, move |r| r.discard(&paths)).await
}

#[tauri::command]
async fn stage_lines(
    repo: String,
    path: String,
    picked: Vec<LineRef>,
    untracked: bool,
) -> CmdResult<()> {
    with_repo(repo, move |r| r.stage_lines(&path, &picked, untracked)).await
}

#[tauri::command]
async fn unstage_lines(repo: String, path: String, picked: Vec<LineRef>) -> CmdResult<()> {
    with_repo(repo, move |r| r.unstage_lines(&path, &picked)).await
}

#[tauri::command]
async fn discard_lines(repo: String, path: String, picked: Vec<LineRef>) -> CmdResult<Undo> {
    with_repo(repo, move |r| r.discard_lines(&path, &picked)).await
}

#[tauri::command]
async fn resolve_conflict(repo: String, path: String, side: ConflictSide) -> CmdResult<()> {
    with_repo(repo, move |r| r.resolve_conflict(&path, side)).await
}

#[tauri::command]
async fn commit(repo: String, message: String, amend: bool) -> CmdResult<Undo> {
    with_repo(repo, move |r| r.commit(&message, amend)).await
}

// ---- History ------------------------------------------------------------------

#[tauri::command]
async fn cherry_pick(repo: String, id: String) -> CmdResult<Undo> {
    with_repo(repo, move |r| r.cherry_pick(&id)).await
}

#[tauri::command]
async fn revert(repo: String, id: String) -> CmdResult<Undo> {
    with_repo(repo, move |r| r.revert(&id)).await
}

#[tauri::command]
async fn reset(repo: String, id: String, mode: ResetMode) -> CmdResult<Undo> {
    with_repo(repo, move |r| r.reset(&id, mode)).await
}

#[tauri::command]
async fn abort_operation(repo: String) -> CmdResult<()> {
    with_repo(repo, |r| r.abort_operation()).await
}

#[tauri::command]
async fn continue_operation(repo: String) -> CmdResult<()> {
    with_repo(repo, |r| r.continue_operation()).await
}

#[tauri::command]
async fn undo(repo: String, action: UndoAction) -> CmdResult<()> {
    with_repo(repo, move |r| r.undo(&action)).await
}

// ---- Branches and tags --------------------------------------------------------

#[tauri::command]
async fn create_branch(
    repo: String,
    name: String,
    start: Option<String>,
    checkout: bool,
) -> CmdResult<Undo> {
    with_repo(repo, move |r| {
        r.create_branch(&name, start.as_deref(), checkout)
    })
    .await
}

#[tauri::command]
async fn checkout_branch(repo: String, name: String) -> CmdResult<Undo> {
    with_repo(repo, move |r| r.checkout_branch(&name)).await
}

#[tauri::command]
async fn checkout_remote_branch(repo: String, name: String) -> CmdResult<Undo> {
    with_repo(repo, move |r| r.checkout_remote_branch(&name)).await
}

#[tauri::command]
async fn checkout_commit(repo: String, id: String) -> CmdResult<Undo> {
    with_repo(repo, move |r| r.checkout_commit(&id)).await
}

#[tauri::command]
async fn rename_branch(repo: String, from: String, to: String) -> CmdResult<Undo> {
    with_repo(repo, move |r| r.rename_branch(&from, &to)).await
}

#[tauri::command]
async fn delete_branch(repo: String, name: String, force: bool) -> CmdResult<Undo> {
    with_repo(repo, move |r| r.delete_branch(&name, force)).await
}

#[tauri::command]
async fn merge(repo: String, name: String) -> CmdResult<Undo> {
    with_repo(repo, move |r| r.merge(&name)).await
}

#[tauri::command]
async fn rebase(repo: String, onto: String) -> CmdResult<Undo> {
    with_repo(repo, move |r| r.rebase(&onto)).await
}

#[tauri::command]
async fn create_tag(
    repo: String,
    name: String,
    target: String,
    message: Option<String>,
) -> CmdResult<Undo> {
    with_repo(repo, move |r| {
        r.create_tag(&name, &target, message.as_deref())
    })
    .await
}

#[tauri::command]
async fn delete_tag(repo: String, name: String) -> CmdResult<Undo> {
    with_repo(repo, move |r| r.delete_tag(&name)).await
}

#[tauri::command]
async fn push_tag(repo: String, name: String) -> CmdResult<()> {
    with_repo(repo, move |r| r.push_tag(&name)).await
}

// ---- Remotes and stash ----------------------------------------------------------

#[tauri::command]
async fn fetch(repo: String) -> CmdResult<()> {
    with_repo(repo, |r| r.fetch()).await
}

#[tauri::command]
async fn pull(repo: String, rebase: bool) -> CmdResult<Undo> {
    with_repo(repo, move |r| r.pull(rebase)).await
}

#[tauri::command]
async fn push(repo: String, force: bool) -> CmdResult<()> {
    with_repo(repo, move |r| r.push(force)).await
}

#[tauri::command]
async fn stash_push(
    repo: String,
    message: Option<String>,
    include_untracked: bool,
) -> CmdResult<Undo> {
    with_repo(repo, move |r| {
        r.stash_push(message.as_deref(), include_untracked)
    })
    .await
}

#[tauri::command]
async fn stash_apply(repo: String, index: usize, pop: bool) -> CmdResult<()> {
    with_repo(repo, move |r| r.stash_apply(index, pop)).await
}

#[tauri::command]
async fn stash_drop(repo: String, index: usize) -> CmdResult<Undo> {
    with_repo(repo, move |r| r.stash_drop(index)).await
}

// ---- New repositories -------------------------------------------------------------

/// Clones into `parent/name` and returns the new repository's path.
#[tauri::command]
async fn clone_repository(url: String, parent: String, name: String) -> CmdResult<String> {
    blocking(move || {
        heritree_core::clone_repository(&url, Path::new(&parent), &name)
            .map(|p: PathBuf| p.display().to_string())
    })
    .await
}

#[tauri::command]
async fn init_repository(path: String) -> CmdResult<()> {
    blocking(move || heritree_core::init_repository(Path::new(&path))).await
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            open_repository,
            repo_info,
            list_refs,
            load_log,
            working_tree_status,
            file_diff,
            commit_details,
            commit_file_diff,
            file_history,
            blame,
            branches_containing,
            lineage,
            stashes,
            remotes,
            commit_template,
            command_log,
            stage,
            unstage,
            discard,
            stage_lines,
            unstage_lines,
            discard_lines,
            resolve_conflict,
            commit,
            cherry_pick,
            revert,
            reset,
            abort_operation,
            continue_operation,
            undo,
            create_branch,
            checkout_branch,
            checkout_remote_branch,
            checkout_commit,
            rename_branch,
            delete_branch,
            merge,
            rebase,
            create_tag,
            delete_tag,
            push_tag,
            fetch,
            pull,
            push,
            stash_push,
            stash_apply,
            stash_drop,
            clone_repository,
            init_repository,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Heritree");
}
