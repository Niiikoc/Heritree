//! Runs the user's installed `git`.

use std::collections::VecDeque;
use std::ffi::OsStr;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use serde::Serialize;

use crate::{Error, Result};

/// One git command the app ran on the user's behalf, for the "Git log" panel.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandRecord {
    /// Folder the command ran in.
    pub dir: String,
    /// The command as you would type it, e.g. `git commit -m "Fix login"`.
    pub command: String,
    pub ok: bool,
    /// What git printed when the command failed.
    pub error: Option<String>,
    /// Seconds since the Unix epoch.
    pub time: i64,
    pub duration_ms: u64,
}

const LOG_CAPACITY: usize = 500;
static COMMAND_LOG: Mutex<VecDeque<CommandRecord>> = Mutex::new(VecDeque::new());

/// The commands that changed something, oldest first.
pub fn command_log() -> Vec<CommandRecord> {
    COMMAND_LOG.lock().unwrap().iter().cloned().collect()
}

/// Runs a read-only `git <args>` in `dir` and returns its stdout. Not logged.
pub(crate) fn run(dir: &Path, args: &[&str]) -> Result<Vec<u8>> {
    output(dir, args, None, false)
}

/// Like [`run`] with input on stdin, for commands such as `check-ignore` that
/// exit with 1 to mean "no match" rather than failure.
pub(crate) fn run_lenient(dir: &Path, args: &[&str], stdin: &[u8]) -> Result<Vec<u8>> {
    output(dir, args, Some(stdin), true)
}

/// Runs a `git` command that changes the repository and records it in the
/// command log, whether it succeeds or not.
pub(crate) fn exec(dir: &Path, args: &[&str], stdin: Option<&[u8]>) -> Result<Vec<u8>> {
    let started = Instant::now();
    let result = output(dir, args, stdin, false);
    let record = CommandRecord {
        dir: dir.display().to_string(),
        command: shell_words(args),
        ok: result.is_ok(),
        error: result.as_ref().err().map(ToString::to_string),
        time: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or_default(),
        duration_ms: started.elapsed().as_millis() as u64,
    };
    let mut log = COMMAND_LOG.lock().unwrap();
    if log.len() == LOG_CAPACITY {
        log.pop_front();
    }
    log.push_back(record);
    result
}

fn output(dir: &Path, args: &[&str], stdin: Option<&[u8]>, exit_one_ok: bool) -> Result<Vec<u8>> {
    let mut cmd = Command::new("git");
    cmd.arg("-C")
        .arg(dir)
        // Never block on a credential or editor prompt; the app has no terminal.
        // Credential helpers with their own window (Git Credential Manager) still work.
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_EDITOR", "true")
        .env("GIT_SEQUENCE_EDITOR", "true")
        .env("GIT_MERGE_AUTOEDIT", "no")
        .env("GIT_OPTIONAL_LOCKS", "0")
        .args(args.iter().map(OsStr::new))
        .stdin(if stdin.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    hide_console_window(&mut cmd);

    let mut child = cmd.spawn().map_err(Error::GitNotFound)?;
    if let Some(input) = stdin {
        let mut pipe = child.stdin.take().expect("stdin is piped");
        let input = input.to_vec();
        // Write from another thread so a large output can't deadlock us.
        std::thread::spawn(move || pipe.write_all(&input));
    }
    let output = child.wait_with_output().map_err(Error::GitNotFound)?;
    if !output.status.success() && !(exit_one_ok && output.status.code() == Some(1)) {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        // Merge and cherry-pick report conflicts on stdout.
        let message = if stderr.is_empty() {
            String::from_utf8_lossy(&output.stdout).trim().to_owned()
        } else {
            stderr
        };
        return Err(Error::GitCommand {
            args: args.join(" "),
            stderr: message,
        });
    }
    Ok(output.stdout)
}

/// Quotes arguments the way a reader would retype them in a shell.
fn shell_words(args: &[&str]) -> String {
    let mut line = String::from("git");
    for arg in args {
        line.push(' ');
        if !arg.is_empty()
            && arg
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || "-_./:=@^~{}+,".contains(c))
        {
            line.push_str(arg);
        } else {
            line.push('"');
            line.push_str(&arg.replace('"', "\\\""));
            line.push('"');
        }
    }
    line
}

/// Stops a console window from flashing up on Windows for every git call.
#[cfg(windows)]
fn hide_console_window(cmd: &mut Command) {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    cmd.creation_flags(CREATE_NO_WINDOW);
}

#[cfg(not(windows))]
fn hide_console_window(_cmd: &mut Command) {}

#[cfg(test)]
mod tests {
    use super::shell_words;

    #[test]
    fn quotes_only_what_needs_it() {
        assert_eq!(
            shell_words(&["commit", "-m", "Fix \"login\" bug"]),
            r#"git commit -m "Fix \"login\" bug""#
        );
        assert_eq!(
            shell_words(&["push", "-u", "origin", "feature/x"]),
            "git push -u origin feature/x"
        );
    }
}
