//! The engine behind Heritree.
//!
//! The UI never touches a repository directly: it asks this crate. Reads go
//! through [gitoxide](https://github.com/GitoxideLabs/gitoxide) for speed,
//! while anything that writes (and, for now, working-tree status and diffs)
//! goes through the user's installed `git` so hooks, credentials and config
//! behave exactly as they do on the command line.

pub mod diff;
mod error;
mod files;
mod git_cli;
pub mod graph;
mod history;
pub mod hosting;
pub mod lineage;
mod model;
pub mod ops;
mod repo;
pub mod status;

pub use diff::{DiffLine, FileDiff, Hunk, LineKind, LineRef};
pub use error::{Error, Result};
pub use files::{FileVersion, find_repositories};
pub use git_cli::{CommandRecord, command_log};
pub use graph::{Edge, EdgeKind, GraphRow};
pub use hosting::{
    HostKind, HostedRepo, NewPullRequest, OpenPullRequest, PullRequest, PullRequestDraft,
    PullRequestOutcome,
};
pub use lineage::{BranchLineage, LineageSource};
pub use model::{
    BlameLine, BranchUpstream, ChangeKind, ChangedFile, CommitDetails, CommitInfo, FileStatus,
    HeadInfo, RefInfo, RefKind, RepoInfo, RepoState, StashEntry,
};
pub use ops::{
    ConflictSide, ResetMode, SavedFile, Undo, UndoAction, clone_repository, init_repository,
};
pub use repo::Repository;
