//! Plain data types shared with the UI. They serialize to camelCase JSON so the
//! TypeScript side can use them as-is.

use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RepoInfo {
    /// Display name, taken from the working-tree folder.
    pub name: String,
    /// Path of the `.git` directory.
    pub git_dir: String,
    /// Path of the working tree, `None` for bare repositories.
    pub workdir: Option<String>,
    pub head: HeadInfo,
    /// Set while a merge, rebase, cherry-pick or revert waits on conflicts.
    pub state: Option<RepoState>,
    /// Tracking branch of the current branch, if it has one.
    pub upstream: Option<BranchUpstream>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HeadInfo {
    /// Short branch name (`main`), `None` when detached.
    pub branch: Option<String>,
    /// Commit HEAD points to, `None` on a branch with no commits yet.
    pub id: Option<String>,
    pub detached: bool,
    pub unborn: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum RefKind {
    LocalBranch,
    RemoteBranch,
    Tag,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RefInfo {
    /// Full name, e.g. `refs/heads/release/2.4`.
    pub name: String,
    /// Name as people write it, e.g. `release/2.4` or `origin/main`.
    pub short_name: String,
    pub kind: RefKind,
    /// The commit the ref resolves to (annotated tags are peeled).
    pub target: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommitInfo {
    pub id: String,
    pub short_id: String,
    pub parents: Vec<String>,
    pub summary: String,
    pub author_name: String,
    pub author_email: String,
    /// Seconds since the Unix epoch.
    pub author_time: i64,
    /// Seconds since the Unix epoch. Used to order the log.
    pub commit_time: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ChangeKind {
    Added,
    Modified,
    Deleted,
    Renamed,
    Copied,
    TypeChanged,
    Untracked,
    Conflicted,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileStatus {
    /// Path relative to the working tree, always with `/` separators.
    pub path: String,
    /// For renames and copies, where the file came from.
    pub original_path: Option<String>,
    /// Change recorded in the index (what a commit would include).
    pub staged: Option<ChangeKind>,
    /// Change in the working tree that is not staged yet.
    pub unstaged: Option<ChangeKind>,
}

/// An operation git is in the middle of, waiting for conflicts to be resolved.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum RepoState {
    Merging,
    Rebasing,
    CherryPicking,
    Reverting,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BranchUpstream {
    /// e.g. `origin/main`.
    pub name: String,
    /// Commits on the local branch that the upstream lacks.
    pub ahead: u32,
    /// Commits on the upstream that the local branch lacks.
    pub behind: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StashEntry {
    /// Position in the stash list: `stash@{index}`.
    pub index: usize,
    pub id: String,
    pub message: String,
    pub time: i64,
}

/// A file touched by a commit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangedFile {
    pub path: String,
    pub original_path: Option<String>,
    pub kind: ChangeKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommitDetails {
    pub id: String,
    /// The whole message, not only the summary line.
    pub message: String,
    pub committer_name: String,
    pub committer_email: String,
    pub commit_time: i64,
    /// Compared with the first parent (or nothing, for a root commit).
    pub files: Vec<ChangedFile>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BlameLine {
    pub line: u32,
    pub commit: String,
    pub author_name: String,
    pub author_time: i64,
    pub summary: String,
    pub text: String,
}
