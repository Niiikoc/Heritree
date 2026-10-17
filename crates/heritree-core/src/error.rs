use std::fmt::Display;
use std::path::PathBuf;

pub type Result<T, E = Error> = std::result::Result<T, E>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("no git repository found at or above {path}: {reason}")]
    NotARepository { path: PathBuf, reason: String },

    #[error("the repository has no working tree")]
    Bare,

    #[error("git error: {0}")]
    Git(String),

    #[error("could not run git: {0}")]
    GitNotFound(#[source] std::io::Error),

    #[error("`git {args}` failed: {stderr}")]
    GitCommand { args: String, stderr: String },

    #[error("could not parse git output: {0}")]
    Parse(String),
}

impl Error {
    /// Wraps any gitoxide error. Gitoxide has one error type per operation, and
    /// callers of this crate only ever need the message.
    pub(crate) fn git(err: impl Display) -> Self {
        Error::Git(err.to_string())
    }
}

impl serde::Serialize for Error {
    /// Errors cross the Tauri boundary as plain strings.
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}
