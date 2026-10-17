//! The engine behind Heritree.
//!
//! The UI never touches a repository directly: it asks this crate. Reads go
//! through [gitoxide](https://github.com/GitoxideLabs/gitoxide) for speed,
//! while anything that writes (and, for now, working-tree status) goes through
//! the user's installed `git` so hooks, credentials and config behave exactly
//! as they do on the command line.

mod error;
mod git_cli;
pub mod graph;
mod model;
mod repo;
pub mod status;

pub use error::{Error, Result};
pub use graph::{Edge, EdgeKind, GraphRow};
pub use model::{ChangeKind, CommitInfo, FileStatus, HeadInfo, RefInfo, RefKind, RepoInfo};
pub use repo::Repository;
