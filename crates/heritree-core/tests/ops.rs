//! Write operations, undo and lineage, run against real repositories.

use std::fs;
use std::path::Path;
use std::process::Command;

use heritree_core::{ChangeKind, LineKind, LineRef, LineageSource, Repository, ResetMode};

fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .expect("git must be installed to run these tests");
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_owned()
}

/// A repository with one commit of `file.txt` (five lines), and an identity
/// in its own config so commits made through the library work anywhere.
fn repo() -> (tempfile::TempDir, Repository) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path();
    git(path, &["init", "-q", "-b", "main"]);
    git(path, &["config", "user.name", "Ada"]);
    git(path, &["config", "user.email", "ada@example.com"]);
    git(path, &["config", "commit.gpgsign", "false"]);
    git(path, &["config", "core.autocrlf", "false"]);
    fs::write(path.join("file.txt"), "one\ntwo\nthree\nfour\nfive\n").unwrap();
    git(path, &["add", "."]);
    git(path, &["commit", "-q", "-m", "first"]);
    let repo = Repository::open(path).unwrap();
    (dir, repo)
}

fn status_of(repo: &Repository, path: &str) -> (Option<ChangeKind>, Option<ChangeKind>) {
    repo.status()
        .unwrap()
        .into_iter()
        .find(|f| f.path == path)
        .map(|f| (f.staged, f.unstaged))
        .unwrap_or((None, None))
}

#[test]
fn stages_single_lines() {
    let (dir, repo) = repo();
    fs::write(dir.path().join("file.txt"), "one\nTWO\nthree\nfour\nFIVE\n").unwrap();

    let diff = repo.file_diff("file.txt", None, false, false).unwrap();
    let hunk = &diff.hunks[0];
    // Pick the "-two" / "+TWO" pair only.
    let picked: Vec<LineRef> = hunk
        .lines
        .iter()
        .enumerate()
        .filter(|(_, l)| l.kind != LineKind::Context && l.text.eq_ignore_ascii_case("two"))
        .map(|(line, _)| LineRef { hunk: 0, line })
        .collect();
    assert_eq!(picked.len(), 2);
    repo.stage_lines("file.txt", &picked, false).unwrap();

    let staged = git(dir.path(), &["show", ":file.txt"]);
    assert_eq!(staged, "one\nTWO\nthree\nfour\nfive");
    assert_eq!(
        status_of(&repo, "file.txt"),
        (Some(ChangeKind::Modified), Some(ChangeKind::Modified))
    );

    // And back out again.
    let staged_diff = repo.file_diff("file.txt", None, true, false).unwrap();
    let all: Vec<LineRef> = (0..staged_diff.hunks[0].lines.len())
        .map(|line| LineRef { hunk: 0, line })
        .collect();
    repo.unstage_lines("file.txt", &all).unwrap();
    assert_eq!(
        git(dir.path(), &["show", ":file.txt"]),
        "one\ntwo\nthree\nfour\nfive"
    );
}

#[test]
fn stages_part_of_an_untracked_file() {
    let (dir, repo) = repo();
    fs::write(dir.path().join("new.txt"), "keep\nskip\n").unwrap();
    repo.stage_lines("new.txt", &[LineRef { hunk: 0, line: 0 }], true)
        .unwrap();
    assert_eq!(git(dir.path(), &["show", ":new.txt"]), "keep");
}

#[test]
fn discard_can_be_undone() {
    let (dir, repo) = repo();
    let file = dir.path().join("file.txt");
    fs::write(&file, "changed\n").unwrap();
    fs::write(dir.path().join("scratch.txt"), "notes").unwrap();

    let undo = repo
        .discard(&["file.txt".to_owned(), "scratch.txt".to_owned()])
        .unwrap();
    assert_eq!(
        fs::read_to_string(&file).unwrap(),
        "one\ntwo\nthree\nfour\nfive\n"
    );
    assert!(!dir.path().join("scratch.txt").exists());

    repo.undo(&undo.action).unwrap();
    assert_eq!(fs::read_to_string(&file).unwrap(), "changed\n");
    assert_eq!(
        fs::read_to_string(dir.path().join("scratch.txt")).unwrap(),
        "notes"
    );
}

#[test]
fn commit_and_undo_keeps_changes_staged() {
    let (dir, repo) = repo();
    fs::write(dir.path().join("b.txt"), "b").unwrap();
    repo.stage(&["b.txt".to_owned()]).unwrap();
    let undo = repo.commit("Add b\n\nWith a body.", false).unwrap();
    assert_eq!(
        git(dir.path(), &["log", "-1", "--format=%B"]),
        "Add b\n\nWith a body."
    );

    repo.undo(&undo.action).unwrap();
    assert_eq!(git(dir.path(), &["log", "-1", "--format=%s"]), "first");
    assert_eq!(status_of(&repo, "b.txt").0, Some(ChangeKind::Added));
}

#[test]
fn hard_reset_undo_brings_back_local_changes() {
    let (dir, repo) = repo();
    let first = git(dir.path(), &["rev-parse", "HEAD"]);
    fs::write(dir.path().join("file.txt"), "second\n").unwrap();
    git(dir.path(), &["commit", "-qam", "second"]);
    fs::write(dir.path().join("file.txt"), "work in progress\n").unwrap();

    let undo = repo.reset(&first, ResetMode::Hard).unwrap();
    assert_eq!(git(dir.path(), &["rev-parse", "HEAD"]), first);

    repo.undo(&undo.action).unwrap();
    assert_eq!(git(dir.path(), &["log", "-1", "--format=%s"]), "second");
    assert_eq!(
        fs::read_to_string(dir.path().join("file.txt")).unwrap(),
        "work in progress\n"
    );
}

#[test]
fn branch_create_checkout_delete_round_trip() {
    let (dir, repo) = repo();
    let undo = repo.create_branch("feature/login", None, true).unwrap();
    assert_eq!(
        repo.info().unwrap().head.branch.as_deref(),
        Some("feature/login")
    );
    assert_eq!(
        git(
            dir.path(),
            &["config", "branch.feature/login.heritree-parent"]
        ),
        "main"
    );
    repo.undo(&undo.action).unwrap();
    assert_eq!(repo.info().unwrap().head.branch.as_deref(), Some("main"));
    assert!(
        repo.refs()
            .unwrap()
            .iter()
            .all(|r| r.short_name != "feature/login")
    );

    repo.create_branch("tmp", None, false).unwrap();
    let undo = repo.delete_branch("tmp", false).unwrap();
    repo.undo(&undo.action).unwrap();
    assert!(repo.refs().unwrap().iter().any(|r| r.short_name == "tmp"));
}

#[test]
fn merge_conflict_reports_state_and_aborts() {
    let (dir, repo) = repo();
    repo.create_branch("other", None, true).unwrap();
    fs::write(dir.path().join("file.txt"), "theirs\n").unwrap();
    git(dir.path(), &["commit", "-qam", "theirs"]);
    repo.checkout_branch("main").unwrap();
    fs::write(dir.path().join("file.txt"), "ours\n").unwrap();
    git(dir.path(), &["commit", "-qam", "ours"]);

    assert!(repo.merge("other").is_err());
    assert_eq!(repo.state(), Some(heritree_core::RepoState::Merging));
    assert_eq!(status_of(&repo, "file.txt").0, Some(ChangeKind::Conflicted));
    repo.abort_operation().unwrap();
    assert_eq!(repo.state(), None);
}

#[test]
fn stash_push_and_drop_undo() {
    let (dir, repo) = repo();
    fs::write(dir.path().join("file.txt"), "stashed\n").unwrap();
    repo.stash_push(Some("wip"), false).unwrap();
    let stashes = repo.stashes().unwrap();
    assert_eq!(stashes.len(), 1);
    assert!(stashes[0].message.contains("wip"));

    let undo = repo.stash_drop(0).unwrap();
    assert!(repo.stashes().unwrap().is_empty());
    repo.undo(&undo.action).unwrap();
    assert_eq!(repo.stashes().unwrap().len(), 1);

    repo.stash_apply(0, true).unwrap();
    assert_eq!(
        fs::read_to_string(dir.path().join("file.txt")).unwrap(),
        "stashed\n"
    );
}

#[test]
fn commit_details_list_changed_files() {
    let (dir, repo) = repo();
    git(dir.path(), &["mv", "file.txt", "renamed.txt"]);
    fs::write(dir.path().join("extra.txt"), "x").unwrap();
    git(dir.path(), &["add", "."]);
    git(dir.path(), &["commit", "-qm", "Move things\n\nBody text."]);
    let head = git(dir.path(), &["rev-parse", "HEAD"]);

    let details = repo.commit_details(&head).unwrap();
    assert_eq!(details.message, "Move things\n\nBody text.");
    assert_eq!(details.files.len(), 2);
    let renamed = details
        .files
        .iter()
        .find(|f| f.path == "renamed.txt")
        .unwrap();
    assert_eq!(renamed.kind, ChangeKind::Renamed);
    assert_eq!(renamed.original_path.as_deref(), Some("file.txt"));

    let diff = repo.commit_file_diff(&head, "extra.txt", None).unwrap();
    assert_eq!(diff.hunks[0].lines[0].text, "x");
    // The root commit diffs against the empty tree.
    let root = git(dir.path(), &["rev-list", "--max-parents=0", "HEAD"]);
    assert_eq!(repo.commit_details(&root).unwrap().files.len(), 1);
}

#[test]
fn lineage_uses_recorded_reflog_and_naming_signals() {
    let (dir, repo) = repo();
    let path = dir.path();
    // Recorded: created through the library.
    repo.create_branch("release/2.4", None, true).unwrap();
    fs::write(path.join("r.txt"), "r").unwrap();
    git(path, &["add", "."]);
    git(path, &["commit", "-qm", "release work"]);
    // Reflog: created with plain git from a named branch.
    git(path, &["branch", "hotfix/2.4.1", "release/2.4"]);
    // Convention: created from HEAD, so the reflog says nothing useful.
    git(path, &["checkout", "-q", "main"]);
    git(path, &["checkout", "-q", "-b", "feature/x"]);

    let lineage = repo.lineage().unwrap();
    let find = |name: &str| lineage.iter().find(|b| b.name == name).unwrap();
    assert_eq!(find("main").parent, None);
    let release = find("release/2.4");
    assert_eq!(release.parent.as_deref(), Some("main"));
    assert_eq!(release.source, Some(LineageSource::Recorded));
    assert_eq!(release.ahead, 1);
    let hotfix = find("hotfix/2.4.1");
    assert_eq!(hotfix.parent.as_deref(), Some("release/2.4"));
    assert_eq!(hotfix.source, Some(LineageSource::Reflog));
    let feature = find("feature/x");
    assert_eq!(feature.parent.as_deref(), Some("main"));
    assert_eq!(feature.source, Some(LineageSource::Convention));
}

#[test]
fn file_history_and_blame() {
    let (dir, repo) = repo();
    fs::write(dir.path().join("file.txt"), "one\nTWO\nthree\nfour\nfive\n").unwrap();
    git(dir.path(), &["commit", "-qam", "Shout two"]);
    let history = repo.file_history("file.txt", 10).unwrap();
    assert_eq!(history.len(), 2);
    assert_eq!(history[0].summary, "Shout two");
    let blame = repo.blame("file.txt").unwrap();
    assert_eq!(blame.len(), 5);
    assert_eq!(blame[1].summary, "Shout two");
    assert_eq!(blame[0].summary, "first");
}

#[test]
fn every_write_lands_in_the_command_log() {
    let (dir, repo) = repo();
    fs::write(dir.path().join("logged.txt"), "x").unwrap();
    repo.stage(&["logged.txt".to_owned()]).unwrap();
    let log = heritree_core::command_log();
    assert!(
        log.iter()
            .any(|r| r.command == "git add -A -- logged.txt" && r.ok)
    );
}
