//! Tells the UI when a repository changes outside the app (an editor saves a
//! file, a terminal commits), so it can refresh without being asked.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

use heritree_core::Repository;
use notify_debouncer_mini::notify::{RecommendedWatcher, RecursiveMode};
use notify_debouncer_mini::{DebounceEventResult, Debouncer, new_debouncer};
use tauri::{AppHandle, Emitter, State};

/// Event sent to the UI; the payload is the repository path.
const EVENT: &str = "repo-changed";

#[derive(Default)]
pub struct Watchers(Mutex<HashMap<String, Debouncer<RecommendedWatcher>>>);

#[tauri::command]
pub fn watch_repository(
    app: AppHandle,
    watchers: State<'_, Watchers>,
    repo: String,
) -> Result<(), String> {
    let mut map = watchers.0.lock().unwrap();
    if map.contains_key(&repo) {
        return Ok(());
    }
    let root = PathBuf::from(&repo);
    let name = repo.clone();
    let mut debouncer = new_debouncer(
        Duration::from_millis(400),
        move |result: DebounceEventResult| {
            let Ok(events) = result else { return };
            if matters(&root, events.iter().map(|e| e.path.as_path())) {
                let _ = app.emit(EVENT, &name);
            }
        },
    )
    .map_err(|e| e.to_string())?;
    debouncer
        .watcher()
        .watch(Path::new(&repo), RecursiveMode::Recursive)
        .map_err(|e| e.to_string())?;
    map.insert(repo, debouncer);
    Ok(())
}

#[tauri::command]
pub fn unwatch_repository(watchers: State<'_, Watchers>, repo: String) {
    watchers.0.lock().unwrap().remove(&repo);
}

/// Whether a batch of changed paths affects what the UI shows: refs, HEAD, the
/// index, an operation in progress, or a working-tree file git does not ignore.
fn matters<'a>(root: &Path, paths: impl Iterator<Item = &'a Path>) -> bool {
    let mut files = Vec::new();
    for path in paths {
        let Ok(rel) = path.strip_prefix(root) else {
            continue;
        };
        let rel = rel.to_string_lossy().replace('\\', "/");
        if rel.is_empty() {
            continue;
        }
        if let Some(inside) = rel.strip_prefix(".git/").or((rel == ".git").then_some("")) {
            if git_dir_change(inside) {
                return true;
            }
            continue;
        }
        files.push(rel);
    }
    if files.is_empty() {
        return false;
    }
    // Build output and other ignored files change all the time; skip them.
    match Repository::open(root).and_then(|r| r.ignored(&files)) {
        Ok(ignored) => files.len() > ignored.len(),
        Err(_) => true,
    }
}

fn git_dir_change(inside: &str) -> bool {
    if inside.ends_with(".lock") {
        return false;
    }
    matches!(
        inside,
        "HEAD" | "index" | "packed-refs" | "MERGE_HEAD" | "CHERRY_PICK_HEAD" | "REVERT_HEAD"
    ) || inside.starts_with("refs/")
        || inside.starts_with("rebase-merge")
        || inside.starts_with("rebase-apply")
}

#[cfg(test)]
mod tests {
    use super::git_dir_change;

    #[test]
    fn only_meaningful_git_dir_files_count() {
        assert!(git_dir_change("HEAD"));
        assert!(git_dir_change("refs/heads/main"));
        assert!(git_dir_change("index"));
        assert!(!git_dir_change("index.lock"));
        assert!(!git_dir_change("objects/ab/cdef"));
        assert!(!git_dir_change("FETCH_HEAD"));
        assert!(!git_dir_change("logs/HEAD"));
    }
}
