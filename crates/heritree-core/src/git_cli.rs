//! Runs the user's installed `git`.

use std::ffi::OsStr;
use std::path::Path;
use std::process::Command;

use crate::{Error, Result};

/// Runs `git <args>` in `dir` and returns its stdout.
pub(crate) fn run(dir: &Path, args: &[&str]) -> Result<Vec<u8>> {
    let mut cmd = Command::new("git");
    cmd.arg("-C")
        .arg(dir)
        // Never block on a credential or editor prompt; the app has no terminal.
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_OPTIONAL_LOCKS", "0")
        .args(args.iter().map(OsStr::new));
    hide_console_window(&mut cmd);

    let output = cmd.output().map_err(Error::GitNotFound)?;
    if !output.status.success() {
        return Err(Error::GitCommand {
            args: args.join(" "),
            stderr: String::from_utf8_lossy(&output.stderr).trim().to_owned(),
        });
    }
    Ok(output.stdout)
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
