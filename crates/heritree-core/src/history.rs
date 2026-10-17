//! Reading history beyond the log: what a commit changed, a file's history
//! and blame.

use crate::diff::{self, FileDiff};
use crate::{
    BlameLine, ChangeKind, ChangedFile, CommitDetails, CommitInfo, Error, Repository, Result,
};

/// The tree of a commit with no files, used as the "parent" of root commits.
const EMPTY_TREE: &str = "4b825dc642cb6eb9a060e54bf8d69288fbee4904";

impl Repository {
    /// Full message, committer and changed files of one commit.
    pub fn commit_details(&self, id: &str) -> Result<CommitDetails> {
        let out = self.git(&["show", "-s", "--format=%P%x1f%cn%x1f%ce%x1f%ct%x1f%B", id])?;
        let mut parts = out.splitn(5, '\u{1f}');
        let bad = || Error::Parse(format!("unexpected commit format for {id}"));
        let parents = parts.next().ok_or_else(bad)?.to_owned();
        let committer_name = parts.next().ok_or_else(bad)?.to_owned();
        let committer_email = parts.next().ok_or_else(bad)?.to_owned();
        let commit_time = parts.next().ok_or_else(bad)?.parse().unwrap_or_default();
        let message = parts.next().ok_or_else(bad)?.trim_end().to_owned();

        let base = parents.split_whitespace().next().unwrap_or(EMPTY_TREE);
        let names = self.git(&[
            "-c",
            "core.quotepath=false",
            "diff-tree",
            "-r",
            "-z",
            "-M",
            "--name-status",
            "--no-commit-id",
            base,
            id,
        ])?;
        Ok(CommitDetails {
            id: id.to_owned(),
            message,
            committer_name,
            committer_email,
            commit_time,
            files: parse_name_status(&names),
        })
    }

    /// How one file changed in a commit, against its first parent.
    pub fn commit_file_diff(
        &self,
        id: &str,
        path: &str,
        old_path: Option<&str>,
    ) -> Result<FileDiff> {
        let parents = self.git(&["show", "-s", "--format=%P", id])?;
        let base = parents.split_whitespace().next().unwrap_or(EMPTY_TREE);
        let mut args = vec![
            "-c",
            "core.quotepath=false",
            "diff",
            "--no-color",
            "--no-ext-diff",
            "--src-prefix=a/",
            "--dst-prefix=b/",
            "-M",
            base,
            id,
            "--",
        ];
        if let Some(old) = old_path {
            args.push(old);
        }
        args.push(path);
        let files = diff::parse(&self.git(&args)?)?;
        files
            .into_iter()
            .find(|f| f.path == path)
            .ok_or_else(|| Error::Git(format!("{path} did not change in {id}")))
    }

    /// Commits that touched `path`, following renames, newest first.
    pub fn file_history(&self, path: &str, limit: usize) -> Result<Vec<CommitInfo>> {
        let limit = limit.to_string();
        let out = self.git(&[
            "log",
            "--follow",
            "-n",
            &limit,
            "--format=%H%x1f%P%x1f%an%x1f%ae%x1f%at%x1f%ct%x1f%s",
            "--",
            path,
        ])?;
        Ok(out
            .lines()
            .filter_map(|line| {
                let mut p = line.splitn(7, '\u{1f}');
                let id = p.next()?.to_owned();
                Some(CommitInfo {
                    short_id: id[..7.min(id.len())].to_owned(),
                    parents: p.next()?.split_whitespace().map(str::to_owned).collect(),
                    author_name: p.next()?.to_owned(),
                    author_email: p.next()?.to_owned(),
                    author_time: p.next()?.parse().unwrap_or_default(),
                    commit_time: p.next()?.parse().unwrap_or_default(),
                    summary: p.next()?.to_owned(),
                    id,
                })
            })
            .collect())
    }

    /// Who last changed each line of `path` in the working tree.
    pub fn blame(&self, path: &str) -> Result<Vec<BlameLine>> {
        let out = self.git(&["blame", "--line-porcelain", "--", path])?;
        Ok(parse_blame(&out))
    }
}

fn parse_name_status(output: &str) -> Vec<ChangedFile> {
    let mut fields = output.split('\0').filter(|f| !f.is_empty());
    let mut files = Vec::new();
    while let Some(status) = fields.next() {
        let kind = match status.as_bytes().first() {
            Some(b'A') => ChangeKind::Added,
            Some(b'D') => ChangeKind::Deleted,
            Some(b'R') => ChangeKind::Renamed,
            Some(b'C') => ChangeKind::Copied,
            Some(b'T') => ChangeKind::TypeChanged,
            Some(b'U') => ChangeKind::Conflicted,
            _ => ChangeKind::Modified,
        };
        let Some(first) = fields.next() else { break };
        if matches!(kind, ChangeKind::Renamed | ChangeKind::Copied) {
            let Some(second) = fields.next() else { break };
            files.push(ChangedFile {
                path: second.to_owned(),
                original_path: Some(first.to_owned()),
                kind,
            });
        } else {
            files.push(ChangedFile {
                path: first.to_owned(),
                original_path: None,
                kind,
            });
        }
    }
    files
}

fn parse_blame(output: &str) -> Vec<BlameLine> {
    let mut lines = Vec::new();
    let mut current: Option<BlameLine> = None;
    for line in output.lines() {
        if let Some(text) = line.strip_prefix('\t') {
            if let Some(mut entry) = current.take() {
                entry.text = text.to_owned();
                lines.push(entry);
            }
        } else if let Some(name) = line.strip_prefix("author ") {
            if let Some(entry) = current.as_mut() {
                entry.author_name = name.to_owned();
            }
        } else if let Some(time) = line.strip_prefix("author-time ") {
            if let Some(entry) = current.as_mut() {
                entry.author_time = time.parse().unwrap_or_default();
            }
        } else if let Some(summary) = line.strip_prefix("summary ") {
            if let Some(entry) = current.as_mut() {
                entry.summary = summary.to_owned();
            }
        } else {
            // "<sha> <orig line> <final line> [<group size>]"
            let mut parts = line.split(' ');
            let (Some(sha), Some(_), Some(final_line)) = (parts.next(), parts.next(), parts.next())
            else {
                continue;
            };
            if sha.len() >= 40 && sha.bytes().all(|b| b.is_ascii_hexdigit()) {
                current = Some(BlameLine {
                    line: final_line.parse().unwrap_or_default(),
                    commit: sha.to_owned(),
                    author_name: String::new(),
                    author_time: 0,
                    summary: String::new(),
                    text: String::new(),
                });
            }
        }
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_name_status_with_renames() {
        let files = parse_name_status("M\0src/a.rs\0R087\0old.md\0new.md\0A\0b.txt\0");
        assert_eq!(files.len(), 3);
        assert_eq!(files[1].path, "new.md");
        assert_eq!(files[1].original_path.as_deref(), Some("old.md"));
        assert_eq!(files[2].kind, ChangeKind::Added);
    }

    #[test]
    fn parses_line_porcelain_blame() {
        let sha = "a".repeat(40);
        let out = format!(
            "{sha} 1 1 1\nauthor Ada\nauthor-time 1700000000\nsummary First\nfilename x\n\thello\n"
        );
        let lines = parse_blame(&out);
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].author_name, "Ada");
        assert_eq!(lines[0].text, "hello");
        assert_eq!(lines[0].line, 1);
    }
}
