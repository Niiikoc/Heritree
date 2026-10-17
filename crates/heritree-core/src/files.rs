//! Raw file contents at a given version, which paths git ignores, and finding
//! repositories on disk.

use std::path::Path;

use serde::Deserialize;

use crate::{Error, Repository, Result, git_cli};

/// Where to read a file from.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum FileVersion {
    /// The file on disk.
    Worktree { path: String },
    /// The staged version.
    Index { path: String },
    /// The version in a commit; `rev` can be anything git accepts (`HEAD`, `abc123^`).
    Commit { rev: String, path: String },
}

impl Repository {
    /// The bytes of a file at `version`, or `None` if it does not exist there.
    pub fn file_version(&self, version: &FileVersion) -> Result<Option<Vec<u8>>> {
        let spec = match version {
            FileVersion::Worktree { path } => {
                return match std::fs::read(self.workdir()?.join(path)) {
                    Ok(bytes) => Ok(Some(bytes)),
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
                    Err(e) => Err(Error::Git(format!("could not read {path}: {e}"))),
                };
            }
            FileVersion::Index { path } => format!(":{path}"),
            FileVersion::Commit { rev, path } => format!("{rev}:{path}"),
        };
        // A missing path or a root commit's parent simply has no version.
        if self.git(&["cat-file", "-e", &spec]).is_err() {
            return Ok(None);
        }
        git_cli::run(self.dir(), &["cat-file", "-p", &spec]).map(Some)
    }

    /// The subset of `paths` (relative to the working tree) that git ignores.
    pub fn ignored(&self, paths: &[String]) -> Result<Vec<String>> {
        if paths.is_empty() {
            return Ok(Vec::new());
        }
        let mut input = paths.join("\0").into_bytes();
        input.push(0);
        let out = git_cli::run_lenient(self.dir(), &["check-ignore", "--stdin", "-z"], &input)?;
        Ok(String::from_utf8_lossy(&out)
            .split('\0')
            .filter(|p| !p.is_empty())
            .map(str::to_owned)
            .collect())
    }
}

/// Folders that never hold repositories worth listing, and are slow to walk.
const SKIP: &[&str] = &[
    "node_modules",
    "target",
    "bin",
    "obj",
    "dist",
    "build",
    "vendor",
];

/// Git repositories at or below `root`, at most `depth` folders down. Does not
/// look inside a repository once found.
pub fn find_repositories(root: &Path, depth: usize) -> Vec<String> {
    let mut found = Vec::new();
    walk(root, depth, &mut found);
    found.sort();
    found
}

fn walk(dir: &Path, depth: usize, found: &mut Vec<String>) {
    if dir.join(".git").exists() {
        found.push(dir.display().to_string());
        return;
    }
    if depth == 0 {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with('.') || SKIP.contains(&name.as_ref()) {
            continue;
        }
        if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
            walk(&entry.path(), depth - 1, found);
        }
    }
}
