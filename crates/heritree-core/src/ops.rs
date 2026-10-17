//! Everything that changes a repository. Each operation runs the user's own
//! `git` (so hooks, credentials and config behave as on the command line),
//! shows up in the command log, and returns how to undo it where that is safe.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::diff::{self, FileDiff, LineRef, PatchMode};
use crate::{ChangeKind, Error, RefKind, RepoState, Repository, Result, StashEntry, git_cli};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ResetMode {
    /// Keep the index and working tree.
    Soft,
    /// Keep the working tree, reset the index.
    Mixed,
    /// Throw away local changes.
    Hard,
    /// Like hard, but refuses to touch files with local changes.
    Keep,
}

impl ResetMode {
    fn flag(self) -> &'static str {
        match self {
            ResetMode::Soft => "--soft",
            ResetMode::Mixed => "--mixed",
            ResetMode::Hard => "--hard",
            ResetMode::Keep => "--keep",
        }
    }
}

/// A saved copy of a working-tree file, taken before discarding changes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SavedFile {
    pub path: String,
    /// Blob with the file's bytes, `None` if the file did not exist.
    pub blob: Option<String>,
}

/// How to take an operation back. The UI keeps a stack of these per repository.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum UndoAction {
    /// Point HEAD (and the current branch) back at `to`; `None` means the
    /// branch had no commits yet. `restore` is a stash of local changes to
    /// re-apply afterwards.
    MoveHead {
        to: Option<String>,
        mode: ResetMode,
        restore: Option<String>,
    },
    Checkout {
        target: String,
    },
    DeleteBranch {
        name: String,
    },
    RestoreBranch {
        name: String,
        id: String,
    },
    RenameBranch {
        from: String,
        to: String,
    },
    DeleteTag {
        name: String,
    },
    RestoreTag {
        name: String,
        id: String,
    },
    RestoreFiles {
        files: Vec<SavedFile>,
    },
    PopStash,
    RestoreStash {
        id: String,
        message: String,
    },
    /// Several steps, run in order.
    Sequence {
        steps: Vec<UndoAction>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Undo {
    /// What the user did, e.g. `Commit "Fix login"`.
    pub label: String,
    pub action: UndoAction,
}

impl Undo {
    fn new(label: impl Into<String>, action: UndoAction) -> Self {
        Self {
            label: label.into(),
            action,
        }
    }
}

/// Conflict resolution that takes one side's version of a whole file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ConflictSide {
    Ours,
    Theirs,
}

impl Repository {
    // ---- Working tree and index ------------------------------------------

    /// The diff of one file: staged (index against HEAD) or unstaged
    /// (working tree against index). Untracked files show as all-new.
    pub fn file_diff(
        &self,
        path: &str,
        old_path: Option<&str>,
        staged: bool,
        untracked: bool,
    ) -> Result<FileDiff> {
        if untracked {
            let content = std::fs::read(self.workdir()?.join(path))
                .map_err(|e| Error::Git(format!("could not read {path}: {e}")))?;
            return Ok(diff::untracked(path, &content));
        }
        let mut args = vec![
            "-c",
            "core.quotepath=false",
            "diff",
            "--no-color",
            "--no-ext-diff",
            "--src-prefix=a/",
            "--dst-prefix=b/",
            "-M",
        ];
        if staged {
            args.push("--cached");
        }
        args.push("--");
        if let Some(old) = old_path {
            args.push(old);
        }
        args.push(path);
        let output = self.git(&args)?;
        Ok(diff::parse(&output)?
            .into_iter()
            .find(|f| f.path == path)
            .unwrap_or_else(|| empty_diff(path)))
    }

    pub fn stage(&self, paths: &[String]) -> Result<()> {
        let mut args = vec!["add", "-A", "--"];
        args.extend(paths.iter().map(String::as_str));
        self.exec(&args).map(drop)
    }

    pub fn unstage(&self, paths: &[String]) -> Result<()> {
        let unborn = self.inner.head().map(|h| h.is_unborn()).unwrap_or(false);
        let mut args = if unborn {
            vec!["rm", "--cached", "-r", "-q", "--ignore-unmatch", "--"]
        } else {
            vec!["reset", "-q", "HEAD", "--"]
        };
        args.extend(paths.iter().map(String::as_str));
        self.exec(&args).map(drop)
    }

    /// Throws away unstaged changes (and deletes untracked files), keeping a
    /// copy of every file so the discard can be undone.
    pub fn discard(&self, paths: &[String]) -> Result<Undo> {
        let status = self.status()?;
        let saved = self.save_files(paths)?;
        let workdir = self.workdir()?;
        let mut tracked = Vec::new();
        for path in paths {
            let untracked = status
                .iter()
                .any(|f| &f.path == path && f.unstaged == Some(ChangeKind::Untracked));
            if untracked {
                std::fs::remove_file(workdir.join(path))
                    .map_err(|e| Error::Git(format!("could not delete {path}: {e}")))?;
            } else {
                tracked.push(path.as_str());
            }
        }
        if !tracked.is_empty() {
            let mut args = vec!["checkout", "-q", "--"];
            args.extend(tracked);
            self.exec(&args)?;
        }
        Ok(Undo::new(
            describe_files("Discard changes in", paths),
            UndoAction::RestoreFiles { files: saved },
        ))
    }

    /// Stages only the picked lines of a file's unstaged diff.
    pub fn stage_lines(&self, path: &str, picked: &[LineRef], untracked: bool) -> Result<()> {
        if untracked {
            let diff = self.file_diff(path, None, false, true)?;
            let content = diff::picked_content(&diff, picked);
            let blob =
                self.exec_with_input(&["hash-object", "-w", "--stdin", "--path", path], &content)?;
            let mode = if is_executable(&self.workdir()?.join(path)) {
                "100755"
            } else {
                "100644"
            };
            let info = format!("{mode},{},{path}", blob.trim());
            return self
                .exec(&["update-index", "--add", "--cacheinfo", &info])
                .map(drop);
        }
        let diff = self.file_diff(path, None, false, false)?;
        self.apply(&diff, picked, PatchMode::Forward, &["--cached"])
    }

    /// Takes the picked lines of a file's staged diff back out of the index.
    pub fn unstage_lines(&self, path: &str, picked: &[LineRef]) -> Result<()> {
        let diff = self.file_diff(path, None, true, false)?;
        self.apply(
            &diff,
            picked,
            PatchMode::Reverse,
            &["--cached", "--reverse"],
        )
    }

    /// Throws away the picked lines of a file's unstaged diff.
    pub fn discard_lines(&self, path: &str, picked: &[LineRef]) -> Result<Undo> {
        let saved = self.save_files(&[path.to_owned()])?;
        let diff = self.file_diff(path, None, false, false)?;
        self.apply(&diff, picked, PatchMode::Reverse, &["--reverse"])?;
        Ok(Undo::new(
            format!("Discard lines in {path}"),
            UndoAction::RestoreFiles { files: saved },
        ))
    }

    fn apply(
        &self,
        diff: &FileDiff,
        picked: &[LineRef],
        mode: PatchMode,
        flags: &[&str],
    ) -> Result<()> {
        let Some(patch) = diff::partial_patch(diff, picked, mode) else {
            return Ok(());
        };
        let mut args = vec!["apply", "--recount", "--whitespace=nowarn"];
        args.extend_from_slice(flags);
        args.push("-");
        self.exec_with_input(&args, patch.as_bytes()).map(drop)
    }

    /// Copies files into the object database so they can be restored later.
    fn save_files(&self, paths: &[String]) -> Result<Vec<SavedFile>> {
        let workdir = self.workdir()?;
        let existing: Vec<&str> = paths
            .iter()
            .filter(|p| workdir.join(p).is_file())
            .map(String::as_str)
            .collect();
        let mut blobs = Vec::new();
        if !existing.is_empty() {
            let mut args = vec!["hash-object", "-w", "--no-filters", "--"];
            args.extend(existing.iter().copied());
            blobs = self
                .git(&args)?
                .lines()
                .map(str::to_owned)
                .collect::<Vec<_>>();
        }
        let mut blobs = blobs.into_iter();
        Ok(paths
            .iter()
            .map(|path| SavedFile {
                path: path.clone(),
                blob: existing
                    .contains(&path.as_str())
                    .then(|| blobs.next())
                    .flatten(),
            })
            .collect())
    }

    /// Takes one side of a conflicted file and marks it resolved.
    pub fn resolve_conflict(&self, path: &str, side: ConflictSide) -> Result<()> {
        let flag = match side {
            ConflictSide::Ours => "--ours",
            ConflictSide::Theirs => "--theirs",
        };
        self.exec(&["checkout", flag, "--", path])?;
        self.exec(&["add", "--", path]).map(drop)
    }

    // ---- Commits -----------------------------------------------------------

    pub fn commit(&self, message: &str, amend: bool) -> Result<Undo> {
        let before = self.head_id();
        // `-m` rather than stdin, so the Git log panel shows the message.
        let mut args = vec!["commit", "-q", "--cleanup=strip", "-m", message];
        if amend {
            args.push("--amend");
        }
        self.exec(&args)?;
        let summary = message.lines().next().unwrap_or_default();
        let label = if amend {
            format!("Amend \"{summary}\"")
        } else {
            format!("Commit \"{summary}\"")
        };
        Ok(Undo::new(
            label,
            UndoAction::MoveHead {
                to: before,
                mode: ResetMode::Soft,
                restore: None,
            },
        ))
    }

    /// The message template from `commit.template`, if the user set one.
    pub fn commit_template(&self) -> Option<String> {
        let path = self
            .git(&["config", "--path", "--get", "commit.template"])
            .ok()?;
        let path = PathBuf::from(path.trim());
        let path = if path.is_relative() {
            self.dir().join(path)
        } else {
            path
        };
        std::fs::read_to_string(path).ok()
    }

    pub fn cherry_pick(&self, id: &str) -> Result<Undo> {
        let before = self.head_id();
        let mut args = vec!["cherry-pick"];
        if self.is_merge(id) {
            args.extend(["-m", "1"]);
        }
        args.push(id);
        self.exec(&args)?;
        Ok(self.head_move(format!("Cherry-pick {}", short(id)), before))
    }

    pub fn revert(&self, id: &str) -> Result<Undo> {
        let before = self.head_id();
        let mut args = vec!["revert", "--no-edit"];
        if self.is_merge(id) {
            args.extend(["-m", "1"]);
        }
        args.push(id);
        self.exec(&args)?;
        Ok(self.head_move(format!("Revert {}", short(id)), before))
    }

    /// Moves the current branch to `id`. A hard reset first saves local changes
    /// under `refs/heritree/backups/`, so undo can bring them back.
    pub fn reset(&self, id: &str, mode: ResetMode) -> Result<Undo> {
        let before = self.head_id();
        let mut restore = None;
        if mode == ResetMode::Hard {
            let stash = self.exec(&["stash", "create"])?.trim().to_owned();
            if !stash.is_empty() {
                let name = format!("refs/heritree/backups/{}", unix_now());
                self.exec(&[
                    "update-ref",
                    "-m",
                    "heritree: backup before reset",
                    &name,
                    &stash,
                ])?;
                restore = Some(stash);
            }
        }
        self.exec(&["reset", "-q", mode.flag(), id])?;
        Ok(Undo::new(
            format!("Reset to {}", short(id)),
            UndoAction::MoveHead {
                to: before,
                mode,
                restore,
            },
        ))
    }

    // ---- Branches ----------------------------------------------------------

    /// Creates a branch at `start` (default: HEAD) and remembers which branch
    /// it came from, the first and most reliable lineage signal.
    pub fn create_branch(&self, name: &str, start: Option<&str>, checkout: bool) -> Result<Undo> {
        let previous = self.head_target();
        let parent = self.lineage_parent_for(start);
        let mut args = if checkout {
            vec!["checkout", "-q", "-b", name]
        } else {
            vec!["branch", name]
        };
        if let Some(start) = start {
            args.push(start);
        }
        self.exec(&args)?;
        if let Some(parent) = parent {
            let key = format!("branch.{name}.heritree-parent");
            self.exec(&["config", &key, &parent])?;
        }
        let delete = UndoAction::DeleteBranch {
            name: name.to_owned(),
        };
        let action = match (checkout, previous) {
            (true, Some(previous)) => UndoAction::Sequence {
                steps: vec![UndoAction::Checkout { target: previous }, delete],
            },
            _ => delete,
        };
        Ok(Undo::new(format!("Create branch {name}"), action))
    }

    /// The branch a new branch starting at `start` was made from, if `start`
    /// names one (or is empty, meaning the current branch).
    fn lineage_parent_for(&self, start: Option<&str>) -> Option<String> {
        let Some(start) = start else {
            return self.current_branch();
        };
        let refs = self.refs().ok()?;
        refs.iter().find_map(|r| match r.kind {
            RefKind::LocalBranch if r.short_name == start => Some(start.to_owned()),
            RefKind::RemoteBranch if r.short_name == start => {
                // `origin/develop` → `develop` when there is a local one.
                let local = start.split_once('/').map(|(_, rest)| rest)?;
                let has_local = refs
                    .iter()
                    .any(|l| l.kind == RefKind::LocalBranch && l.short_name == local);
                Some(if has_local { local } else { start }.to_owned())
            }
            _ => None,
        })
    }

    /// Switches to a local branch.
    pub fn checkout_branch(&self, name: &str) -> Result<Undo> {
        let previous = self.head_target();
        self.exec(&["checkout", "-q", name])?;
        Ok(self.checkout_undo(format!("Check out {name}"), previous, None))
    }

    /// Checks out a remote branch as a new local tracking branch, or switches
    /// to the local one if it already exists.
    pub fn checkout_remote_branch(&self, remote_branch: &str) -> Result<Undo> {
        let local = remote_branch
            .split_once('/')
            .map(|(_, rest)| rest)
            .ok_or_else(|| Error::Git(format!("{remote_branch} is not a remote branch")))?;
        let exists = self
            .refs()?
            .iter()
            .any(|r| r.kind == RefKind::LocalBranch && r.short_name == local);
        if exists {
            return self.checkout_branch(local);
        }
        let previous = self.head_target();
        self.exec(&["checkout", "-q", "-b", local, "--track", remote_branch])?;
        Ok(self.checkout_undo(
            format!("Check out {remote_branch}"),
            previous,
            Some(local.to_owned()),
        ))
    }

    pub fn checkout_commit(&self, id: &str) -> Result<Undo> {
        let previous = self.head_target();
        self.exec(&["checkout", "-q", "--detach", id])?;
        Ok(self.checkout_undo(format!("Check out {}", short(id)), previous, None))
    }

    fn checkout_undo(
        &self,
        label: String,
        previous: Option<String>,
        created: Option<String>,
    ) -> Undo {
        let mut steps = Vec::new();
        if let Some(target) = previous {
            steps.push(UndoAction::Checkout { target });
        }
        if let Some(name) = created {
            steps.push(UndoAction::DeleteBranch { name });
        }
        Undo::new(label, UndoAction::Sequence { steps })
    }

    pub fn rename_branch(&self, from: &str, to: &str) -> Result<Undo> {
        self.exec(&["branch", "-m", from, to])?;
        Ok(Undo::new(
            format!("Rename {from} to {to}"),
            UndoAction::RenameBranch {
                from: to.to_owned(),
                to: from.to_owned(),
            },
        ))
    }

    /// Deletes a local branch. Without `force`, git refuses if it has commits
    /// that no other branch has.
    pub fn delete_branch(&self, name: &str, force: bool) -> Result<Undo> {
        let id = self
            .git(&["rev-parse", "--verify", &format!("refs/heads/{name}")])?
            .trim()
            .to_owned();
        self.exec(&["branch", if force { "-D" } else { "-d" }, name])?;
        Ok(Undo::new(
            format!("Delete branch {name}"),
            UndoAction::RestoreBranch {
                name: name.to_owned(),
                id,
            },
        ))
    }

    /// Merges `name` into the current branch.
    pub fn merge(&self, name: &str) -> Result<Undo> {
        let before = self.head_id();
        self.exec(&["merge", "--no-edit", name])?;
        Ok(self.head_move(format!("Merge {name}"), before))
    }

    /// Rebases the current branch onto `onto`.
    pub fn rebase(&self, onto: &str) -> Result<Undo> {
        let before = self.head_id();
        self.exec(&["rebase", onto])?;
        Ok(self.head_move(format!("Rebase onto {onto}"), before))
    }

    /// Stops a merge, rebase, cherry-pick or revert that hit conflicts.
    pub fn abort_operation(&self) -> Result<()> {
        let command = match self.state() {
            Some(RepoState::Merging) => "merge",
            Some(RepoState::Rebasing) => "rebase",
            Some(RepoState::CherryPicking) => "cherry-pick",
            Some(RepoState::Reverting) => "revert",
            None => return Ok(()),
        };
        self.exec(&[command, "--abort"]).map(drop)
    }

    /// Finishes a merge, rebase, cherry-pick or revert once every conflict is staged.
    pub fn continue_operation(&self) -> Result<()> {
        match self.state() {
            Some(RepoState::Merging) => self.exec(&["commit", "--no-edit"]),
            Some(RepoState::Rebasing) => self.exec(&["rebase", "--continue"]),
            Some(RepoState::CherryPicking) => self.exec(&["cherry-pick", "--continue"]),
            Some(RepoState::Reverting) => self.exec(&["revert", "--continue"]),
            None => return Ok(()),
        }
        .map(drop)
    }

    /// Local and remote branches whose history contains `id`.
    pub fn branches_containing(&self, id: &str) -> Result<Vec<String>> {
        Ok(self
            .git(&[
                "branch",
                "-a",
                "--contains",
                id,
                "--format=%(refname:short)",
            ])?
            .lines()
            .filter(|l| !l.is_empty() && !l.ends_with("/HEAD") && !l.starts_with('('))
            .map(str::to_owned)
            .collect())
    }

    // ---- Tags --------------------------------------------------------------

    /// Creates a tag; with a message it is an annotated tag.
    pub fn create_tag(&self, name: &str, target: &str, message: Option<&str>) -> Result<Undo> {
        match message.filter(|m| !m.trim().is_empty()) {
            Some(message) => self.exec(&["tag", "-a", name, "-m", message, target])?,
            None => self.exec(&["tag", name, target])?,
        };
        Ok(Undo::new(
            format!("Create tag {name}"),
            UndoAction::DeleteTag {
                name: name.to_owned(),
            },
        ))
    }

    pub fn delete_tag(&self, name: &str) -> Result<Undo> {
        let id = self
            .git(&["rev-parse", "--verify", &format!("refs/tags/{name}")])?
            .trim()
            .to_owned();
        self.exec(&["tag", "-d", name])?;
        Ok(Undo::new(
            format!("Delete tag {name}"),
            UndoAction::RestoreTag {
                name: name.to_owned(),
                id,
            },
        ))
    }

    pub fn push_tag(&self, name: &str) -> Result<()> {
        let remote = self.default_remote()?;
        self.exec(&["push", &remote, &format!("refs/tags/{name}")])
            .map(drop)
    }

    // ---- Remotes -----------------------------------------------------------

    pub fn remotes(&self) -> Result<Vec<String>> {
        Ok(self
            .git(&["remote"])?
            .lines()
            .filter(|l| !l.is_empty())
            .map(str::to_owned)
            .collect())
    }

    pub fn fetch(&self) -> Result<()> {
        self.exec(&["fetch", "--all", "--prune"]).map(drop)
    }

    pub fn pull(&self, rebase: bool) -> Result<Undo> {
        let before = self.head_id();
        self.exec(&["pull", if rebase { "--rebase" } else { "--no-rebase" }])?;
        Ok(self.head_move("Pull".to_owned(), before))
    }

    /// Pushes the current branch, setting its upstream on the first push.
    /// Force pushes only ever use `--force-with-lease`.
    pub fn push(&self, force: bool) -> Result<()> {
        let branch = self
            .current_branch()
            .ok_or_else(|| Error::Git("check out a branch before pushing".into()))?;
        let has_upstream = self
            .git(&["rev-parse", "--abbrev-ref", "@{upstream}"])
            .is_ok();
        let mut args = vec!["push"];
        if force {
            args.push("--force-with-lease");
        }
        let remote;
        if !has_upstream {
            remote = self.default_remote()?;
            args.extend(["-u", remote.as_str(), branch.as_str()]);
        }
        self.exec(&args).map(drop)
    }

    pub(crate) fn default_remote(&self) -> Result<String> {
        if let Ok(remote) = self.git(&["config", "--get", "remote.pushDefault"]) {
            return Ok(remote.trim().to_owned());
        }
        let remotes = self.remotes()?;
        remotes
            .iter()
            .find(|r| r.as_str() == "origin")
            .or(remotes.first())
            .cloned()
            .ok_or_else(|| Error::Git("this repository has no remote to push to".into()))
    }

    // ---- Stash -------------------------------------------------------------

    pub fn stashes(&self) -> Result<Vec<StashEntry>> {
        Ok(self
            .git(&["stash", "list", "--format=%H%x1f%ct%x1f%gs"])?
            .lines()
            .enumerate()
            .filter_map(|(index, line)| {
                let mut parts = line.splitn(3, '\u{1f}');
                Some(StashEntry {
                    index,
                    id: parts.next()?.to_owned(),
                    time: parts.next()?.parse().unwrap_or_default(),
                    message: parts.next()?.to_owned(),
                })
            })
            .collect())
    }

    pub fn stash_push(&self, message: Option<&str>, include_untracked: bool) -> Result<Undo> {
        let count = self.stashes()?.len();
        let mut args = vec!["stash", "push"];
        if include_untracked {
            args.push("--include-untracked");
        }
        if let Some(message) = message.filter(|m| !m.trim().is_empty()) {
            args.extend(["-m", message]);
        }
        self.exec(&args)?;
        if self.stashes()?.len() == count {
            return Err(Error::Git("there are no local changes to stash".into()));
        }
        Ok(Undo::new("Stash changes", UndoAction::PopStash))
    }

    pub fn stash_apply(&self, index: usize, pop: bool) -> Result<()> {
        let name = format!("stash@{{{index}}}");
        self.exec(&["stash", if pop { "pop" } else { "apply" }, &name])
            .map(drop)
    }

    pub fn stash_drop(&self, index: usize) -> Result<Undo> {
        let entry = self
            .stashes()?
            .into_iter()
            .nth(index)
            .ok_or_else(|| Error::Git(format!("no stash at position {index}")))?;
        self.exec(&["stash", "drop", "-q", &format!("stash@{{{index}}}")])?;
        Ok(Undo::new(
            format!("Drop stash \"{}\"", entry.message),
            UndoAction::RestoreStash {
                id: entry.id,
                message: entry.message,
            },
        ))
    }

    // ---- Undo --------------------------------------------------------------

    pub fn undo(&self, action: &UndoAction) -> Result<()> {
        match action {
            UndoAction::MoveHead { to, mode, restore } => {
                match to {
                    Some(id) => self.exec(&["reset", "-q", mode.flag(), id])?,
                    // Back to a branch with no commits: drop the branch ref, keep the index.
                    None => self.exec(&["update-ref", "-d", "HEAD"])?,
                };
                if let Some(stash) = restore {
                    self.exec(&["stash", "apply", stash])?;
                }
            }
            UndoAction::Checkout { target } => {
                self.exec(&["checkout", "-q", target])?;
            }
            UndoAction::DeleteBranch { name } => {
                self.exec(&["branch", "-D", name])?;
            }
            UndoAction::RestoreBranch { name, id } => {
                self.exec(&["branch", name, id])?;
            }
            UndoAction::RenameBranch { from, to } => {
                self.exec(&["branch", "-m", from, to])?;
            }
            UndoAction::DeleteTag { name } => {
                self.exec(&["tag", "-d", name])?;
            }
            UndoAction::RestoreTag { name, id } => {
                self.exec(&["update-ref", &format!("refs/tags/{name}"), id])?;
            }
            UndoAction::RestoreFiles { files } => {
                let workdir = self.workdir()?;
                for file in files {
                    let target = workdir.join(&file.path);
                    match &file.blob {
                        Some(blob) => {
                            let bytes = git_cli::run(&workdir, &["cat-file", "blob", blob])?;
                            if let Some(parent) = target.parent() {
                                std::fs::create_dir_all(parent).map_err(io_error)?;
                            }
                            std::fs::write(&target, bytes).map_err(io_error)?;
                        }
                        None if target.exists() => {
                            std::fs::remove_file(&target).map_err(io_error)?
                        }
                        None => {}
                    }
                }
            }
            UndoAction::PopStash => {
                self.exec(&["stash", "pop"])?;
            }
            UndoAction::RestoreStash { id, message } => {
                self.exec(&["stash", "store", "-m", message, id])?;
            }
            UndoAction::Sequence { steps } => {
                for step in steps {
                    self.undo(step)?;
                }
            }
        }
        Ok(())
    }

    // ---- Helpers -----------------------------------------------------------

    fn head_id(&self) -> Option<String> {
        self.inner.head_id().ok().map(|id| id.to_string())
    }

    pub(crate) fn current_branch(&self) -> Option<String> {
        let head = self.inner.head().ok()?;
        if head.is_detached() {
            return None;
        }
        head.referent_name()
            .map(|n| String::from_utf8_lossy(n.shorten()).into_owned())
    }

    /// What `git checkout` needs to come back here: the branch, or the commit when detached.
    fn head_target(&self) -> Option<String> {
        self.current_branch().or_else(|| self.head_id())
    }

    fn head_move(&self, label: String, before: Option<String>) -> Undo {
        Undo::new(
            label,
            UndoAction::MoveHead {
                to: before,
                mode: ResetMode::Keep,
                restore: None,
            },
        )
    }

    fn is_merge(&self, id: &str) -> bool {
        self.git(&["rev-list", "--parents", "-n", "1", id])
            .map(|line| line.split_whitespace().count() > 2)
            .unwrap_or(false)
    }
}

/// Clones `url` into `parent/name` and returns the new working tree.
pub fn clone_repository(url: &str, parent: &Path, name: &str) -> Result<PathBuf> {
    git_cli::exec(parent, &["clone", "--", url, name], None)?;
    Ok(parent.join(name))
}

/// Creates an empty repository in `dir`.
pub fn init_repository(dir: &Path) -> Result<()> {
    git_cli::exec(dir, &["init"], None).map(drop)
}

fn empty_diff(path: &str) -> FileDiff {
    let mut diff = diff::untracked(path, b"");
    diff.is_new = false;
    diff
}

fn describe_files(verb: &str, paths: &[String]) -> String {
    match paths {
        [one] => format!("{verb} {one}"),
        many => format!("{verb} {} files", many.len()),
    }
}

fn short(id: &str) -> &str {
    &id[..id.len().min(7)]
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or_default()
}

fn io_error(e: std::io::Error) -> Error {
    Error::Git(e.to_string())
}

#[cfg(unix)]
fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path)
        .map(|m| m.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

#[cfg(not(unix))]
fn is_executable(_path: &Path) -> bool {
    false
}
