//! Runs heritree-core against real repositories built with the `git` CLI.

use std::path::Path;
use std::process::Command;

use heritree_core::{ChangeKind, RefKind, Repository, graph};

/// Runs git with a fixed identity and no user or system config.
fn git_at(dir: &Path, args: &[&str], date: Option<&str>) {
    let mut cmd = Command::new("git");
    cmd.arg("-C")
        .arg(dir)
        .args([
            "-c",
            "user.name=Ada",
            "-c",
            "user.email=ada@example.com",
            "-c",
            "commit.gpgsign=false",
        ])
        .args(args)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env(
            "GIT_CONFIG_GLOBAL",
            if cfg!(windows) { "NUL" } else { "/dev/null" },
        );
    if let Some(date) = date {
        let date = format!("{date} +0000");
        cmd.env("GIT_AUTHOR_DATE", &date)
            .env("GIT_COMMITTER_DATE", &date);
    }
    let status = cmd
        .status()
        .expect("git must be installed to run these tests");
    assert!(status.success(), "git {args:?} failed");
}

fn git(dir: &Path, args: &[&str]) {
    git_at(dir, args, None);
}

fn commit(dir: &Path, file: &str, message: &str, date: &str) {
    std::fs::write(dir.join(file), message).unwrap();
    git(dir, &["add", file]);
    git_at(dir, &["commit", "-q", "-m", message], Some(date));
}

/// main: a -- b ------- m
///             \       /
/// release/1.0  r1 -- r2   (tag v1.0 on r2)
fn sample_repo() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path();
    git(path, &["init", "-q", "-b", "main"]);
    commit(path, "a.txt", "a", "2026-01-01T10:00:00");
    commit(path, "b.txt", "b", "2026-01-02T10:00:00");
    git(path, &["checkout", "-q", "-b", "release/1.0"]);
    commit(path, "r1.txt", "r1", "2026-01-03T10:00:00");
    commit(path, "r2.txt", "r2", "2026-01-04T10:00:00");
    git_at(
        path,
        &["tag", "-a", "v1.0", "-m", "1.0"],
        Some("2026-01-04T11:00:00"),
    );
    git(path, &["checkout", "-q", "main"]);
    git_at(
        path,
        &["merge", "-q", "--no-ff", "-m", "m", "release/1.0"],
        Some("2026-01-05T10:00:00"),
    );
    dir
}

#[test]
fn reads_info_refs_and_log() {
    let dir = sample_repo();
    let repo = Repository::open(dir.path()).unwrap();

    let info = repo.info().unwrap();
    assert_eq!(info.head.branch.as_deref(), Some("main"));
    assert!(!info.head.detached && !info.head.unborn);
    assert!(info.workdir.is_some());
    assert!(!info.name.is_empty());

    let refs = repo.refs().unwrap();
    let names: Vec<(&str, RefKind)> = refs
        .iter()
        .map(|r| (r.short_name.as_str(), r.kind))
        .collect();
    assert_eq!(
        names,
        vec![
            ("main", RefKind::LocalBranch),
            ("release/1.0", RefKind::LocalBranch),
            ("v1.0", RefKind::Tag)
        ]
    );

    let log = repo.log(100).unwrap();
    let summaries: Vec<&str> = log.iter().map(|c| c.summary.as_str()).collect();
    assert_eq!(summaries, vec!["m", "r2", "r1", "b", "a"]);
    assert_eq!(log[0].parents.len(), 2);
    assert_eq!(log[0].author_name, "Ada");
    assert_eq!(log[0].short_id.len(), 7);

    // The annotated tag is peeled to the commit it marks.
    let tag = refs.iter().find(|r| r.kind == RefKind::Tag).unwrap();
    assert_eq!(tag.target, log[1].id);

    let rows = graph::layout(&log);
    assert_eq!(rows.len(), 5);
    assert!(rows.iter().all(|r| r.width <= 2));
}

#[test]
fn log_respects_the_limit() {
    let dir = sample_repo();
    let repo = Repository::open(dir.path()).unwrap();
    assert_eq!(repo.log(2).unwrap().len(), 2);
}

#[test]
fn opens_from_a_subfolder() {
    let dir = sample_repo();
    let sub = dir.path().join("nested");
    std::fs::create_dir(&sub).unwrap();
    assert!(Repository::open(&sub).is_ok());
}

#[test]
fn reports_working_tree_status() {
    let dir = sample_repo();
    let path = dir.path();
    std::fs::write(path.join("a.txt"), "changed").unwrap();
    std::fs::write(path.join("new.txt"), "new").unwrap();
    std::fs::write(path.join("staged.txt"), "staged").unwrap();
    git(path, &["add", "staged.txt"]);

    let repo = Repository::open(path).unwrap();
    let mut status = repo.status().unwrap();
    status.sort_by(|a, b| a.path.cmp(&b.path));
    let summary: Vec<(&str, Option<ChangeKind>, Option<ChangeKind>)> = status
        .iter()
        .map(|f| (f.path.as_str(), f.staged, f.unstaged))
        .collect();
    assert_eq!(
        summary,
        vec![
            ("a.txt", None, Some(ChangeKind::Modified)),
            ("new.txt", None, Some(ChangeKind::Untracked)),
            ("staged.txt", Some(ChangeKind::Added), None),
        ]
    );
}

#[test]
fn empty_repository_has_an_unborn_head() {
    let dir = tempfile::tempdir().unwrap();
    git(dir.path(), &["init", "-q", "-b", "main"]);
    let repo = Repository::open(dir.path()).unwrap();
    let info = repo.info().unwrap();
    assert!(info.head.unborn);
    assert_eq!(info.head.id, None);
    assert!(repo.log(10).unwrap().is_empty());
    assert!(repo.refs().unwrap().is_empty());
}

#[test]
fn not_a_repository() {
    let dir = tempfile::tempdir().unwrap();
    assert!(Repository::open(dir.path()).is_err());
}
