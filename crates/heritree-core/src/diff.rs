//! Unified diffs: parsing `git diff` output into hunks and lines, and building
//! the partial patches behind "stage these lines" and "discard this hunk".

use serde::{Deserialize, Serialize};

use crate::{Error, Result};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileDiff {
    pub path: String,
    /// Where the file came from, for renames and copies.
    pub old_path: Option<String>,
    pub is_new: bool,
    pub is_deleted: bool,
    /// Git found binary content; there are no hunks to show.
    pub binary: bool,
    pub hunks: Vec<Hunk>,
    /// The raw `diff --git` … `+++` lines, reused when building patches.
    #[serde(skip)]
    pub(crate) header: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Hunk {
    /// The `@@ -1,4 +1,5 @@ fn main()` line.
    pub header: String,
    pub old_start: u32,
    pub old_lines: u32,
    pub new_start: u32,
    pub new_lines: u32,
    pub lines: Vec<DiffLine>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum LineKind {
    Context,
    Added,
    Removed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffLine {
    pub kind: LineKind,
    pub old_line: Option<u32>,
    pub new_line: Option<u32>,
    /// The line without its `+`/`-`/` ` prefix or newline. A `\r` stays, so
    /// patches built from it apply to files with Windows line endings.
    pub text: String,
    /// Followed by `\ No newline at end of file`.
    pub no_newline: bool,
}

/// A line picked in the diff viewer: `line` indexes into `hunks[hunk].lines`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize, Serialize)]
pub struct LineRef {
    pub hunk: usize,
    pub line: usize,
}

/// Parses the output of `git diff` (any number of files).
pub fn parse(output: &str) -> Result<Vec<FileDiff>> {
    let mut files: Vec<FileDiff> = Vec::new();
    let mut in_header = false;

    for line in output.split('\n') {
        if let Some(rest) = line.strip_prefix("diff --git ") {
            let path = header_paths(rest).1;
            files.push(FileDiff {
                path: path.clone(),
                old_path: None,
                is_new: false,
                is_deleted: false,
                binary: false,
                hunks: Vec::new(),
                header: vec![line.to_owned()],
            });
            in_header = true;
            continue;
        }
        let Some(file) = files.last_mut() else {
            continue;
        };

        if line.starts_with("@@") {
            in_header = false;
            file.hunks.push(parse_hunk_header(line)?);
            continue;
        }
        if in_header {
            file.header.push(line.to_owned());
            if line.starts_with("new file mode") {
                file.is_new = true;
            } else if line.starts_with("deleted file mode") {
                file.is_deleted = true;
            } else if let Some(from) = line
                .strip_prefix("rename from ")
                .or_else(|| line.strip_prefix("copy from "))
            {
                file.old_path = Some(unquote(from));
            } else if let Some(to) = line
                .strip_prefix("rename to ")
                .or_else(|| line.strip_prefix("copy to "))
            {
                file.path = unquote(to);
            } else if let Some(to) = line.strip_prefix("+++ ") {
                if to != "/dev/null" {
                    file.path = strip_prefix_dir(&unquote(to));
                }
            } else if line.starts_with("Binary files ") || line == "GIT binary patch" {
                file.binary = true;
            }
            continue;
        }

        let Some(hunk) = file.hunks.last_mut() else {
            continue;
        };
        if line.starts_with('\\') {
            if let Some(last) = hunk.lines.last_mut() {
                last.no_newline = true;
            }
            continue;
        }
        let (kind, text) = match line.as_bytes().first() {
            Some(b'+') => (LineKind::Added, &line[1..]),
            Some(b'-') => (LineKind::Removed, &line[1..]),
            Some(b' ') => (LineKind::Context, &line[1..]),
            // The empty string after the final newline.
            _ => continue,
        };
        let (old_seen, new_seen) = hunk.lines.iter().fold((0, 0), |(o, n), l| match l.kind {
            LineKind::Context => (o + 1, n + 1),
            LineKind::Removed => (o + 1, n),
            LineKind::Added => (o, n + 1),
        });
        if old_seen >= hunk.old_lines && new_seen >= hunk.new_lines {
            continue; // stray text after the hunk
        }
        hunk.lines.push(DiffLine {
            kind,
            old_line: (kind != LineKind::Added).then_some(hunk.old_start + old_seen),
            new_line: (kind != LineKind::Removed).then_some(hunk.new_start + new_seen),
            text: text.to_owned(),
            no_newline: false,
        });
    }
    Ok(files)
}

/// A diff that adds every line of `content`, for files git does not track yet.
pub fn untracked(path: &str, content: &[u8]) -> FileDiff {
    let probe = &content[..content.len().min(8000)];
    let binary = probe.contains(&0);
    let mut hunks = Vec::new();
    if !binary && !content.is_empty() {
        let text = String::from_utf8_lossy(content);
        let no_newline = !text.ends_with('\n');
        let mut lines: Vec<DiffLine> = text
            .strip_suffix('\n')
            .unwrap_or(&text)
            .split('\n')
            .enumerate()
            .map(|(i, line)| DiffLine {
                kind: LineKind::Added,
                old_line: None,
                new_line: Some(i as u32 + 1),
                text: line.to_owned(),
                no_newline: false,
            })
            .collect();
        if let Some(last) = lines.last_mut() {
            last.no_newline = no_newline;
        }
        let count = lines.len() as u32;
        hunks.push(Hunk {
            header: format!("@@ -0,0 +1,{count} @@"),
            old_start: 0,
            old_lines: 0,
            new_start: 1,
            new_lines: count,
            lines,
        });
    }
    FileDiff {
        path: path.to_owned(),
        old_path: None,
        is_new: true,
        is_deleted: false,
        binary,
        hunks,
        header: Vec::new(),
    }
}

/// Which way a partial patch will be applied.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PatchMode {
    /// `git apply --cached`: add the picked changes to the index.
    Forward,
    /// `git apply --reverse` (with or without `--cached`): take the picked
    /// changes back out of the index or the working tree.
    Reverse,
}

/// Builds a patch containing only the picked `+`/`-` lines of `file`.
/// Returns `None` when the selection holds no changed lines.
pub fn partial_patch(file: &FileDiff, picked: &[LineRef], mode: PatchMode) -> Option<String> {
    let is_picked = |hunk: usize, line: usize| picked.contains(&LineRef { hunk, line });
    let mut body = String::new();
    let mut delta: i64 = 0;
    let mut all_changes_picked = true;
    let mut any = false;

    for (h, hunk) in file.hunks.iter().enumerate() {
        let mut lines = String::new();
        let (mut old_count, mut new_count) = (0u32, 0u32);
        let mut changed = false;
        for (l, line) in hunk.lines.iter().enumerate() {
            let picked = is_picked(h, l);
            let prefix = match (line.kind, picked, mode) {
                (LineKind::Context, ..) => ' ',
                (LineKind::Added, true, _) => '+',
                (LineKind::Removed, true, _) => '-',
                // Forward: an unpicked addition is not in the index yet, an
                // unpicked removal is still there.
                (LineKind::Added, false, PatchMode::Forward) => continue,
                (LineKind::Removed, false, PatchMode::Forward) => ' ',
                // Reverse: the target still has unpicked additions and still
                // lacks unpicked removals.
                (LineKind::Added, false, PatchMode::Reverse) => ' ',
                (LineKind::Removed, false, PatchMode::Reverse) => continue,
            };
            if line.kind != LineKind::Context && !picked {
                all_changes_picked = false;
            }
            match prefix {
                '+' => new_count += 1,
                '-' => old_count += 1,
                _ => {
                    old_count += 1;
                    new_count += 1;
                }
            }
            changed |= prefix != ' ';
            lines.push(prefix);
            lines.push_str(&line.text);
            lines.push('\n');
            if line.no_newline {
                lines.push_str("\\ No newline at end of file\n");
            }
        }
        if !changed {
            continue;
        }
        any = true;
        let new_start = (i64::from(hunk.old_start) + delta).max(0);
        body.push_str(&format!(
            "@@ -{},{} +{},{} @@\n",
            hunk.old_start, old_count, new_start, new_count
        ));
        body.push_str(&lines);
        delta += i64::from(new_count) - i64::from(old_count);
    }
    if !any {
        return None;
    }

    let mut patch = String::new();
    for line in &file.header {
        // A partly staged deletion leaves the file in place.
        if file.is_deleted && !all_changes_picked {
            if line.starts_with("deleted file mode") {
                continue;
            }
            if line == "+++ /dev/null" {
                patch.push_str(&format!("+++ b/{}\n", file.path));
                continue;
            }
        }
        patch.push_str(line);
        patch.push('\n');
    }
    patch.push_str(&body);
    Some(patch)
}

/// The text a new file gets when only the `picked` lines of `file` (an
/// untracked file's diff) are staged.
pub fn picked_content(file: &FileDiff, picked: &[LineRef]) -> Vec<u8> {
    let mut out = String::new();
    for (h, hunk) in file.hunks.iter().enumerate() {
        for (l, line) in hunk.lines.iter().enumerate() {
            if picked.contains(&LineRef { hunk: h, line: l }) {
                out.push_str(&line.text);
                if !line.no_newline {
                    out.push('\n');
                }
            }
        }
    }
    out.into_bytes()
}

fn parse_hunk_header(line: &str) -> Result<Hunk> {
    let bad = || Error::Parse(format!("bad hunk header: {line}"));
    let inner = line.strip_prefix("@@ ").ok_or_else(bad)?;
    let (ranges, _) = inner.split_once(" @@").ok_or_else(bad)?;
    let (old, new) = ranges.split_once(' ').ok_or_else(bad)?;
    let range = |r: &str, sign: char| -> Option<(u32, u32)> {
        let r = r.strip_prefix(sign)?;
        Some(match r.split_once(',') {
            Some((start, count)) => (start.parse().ok()?, count.parse().ok()?),
            None => (r.parse().ok()?, 1),
        })
    };
    let (old_start, old_lines) = range(old, '-').ok_or_else(bad)?;
    let (new_start, new_lines) = range(new, '+').ok_or_else(bad)?;
    Ok(Hunk {
        header: line.to_owned(),
        old_start,
        old_lines,
        new_start,
        new_lines,
        lines: Vec::new(),
    })
}

/// `a/x b/x` → (`x`, `x`). Only used as a fallback until `+++` or `rename to`.
fn header_paths(rest: &str) -> (String, String) {
    let rest = rest.trim();
    if let Some(mid) = rest.find(" b/") {
        let a = strip_prefix_dir(&unquote(&rest[..mid]));
        let b = strip_prefix_dir(&unquote(&rest[mid + 1..]));
        return (a, b);
    }
    (rest.to_owned(), rest.to_owned())
}

fn strip_prefix_dir(path: &str) -> String {
    path.strip_prefix("a/")
        .or_else(|| path.strip_prefix("b/"))
        .unwrap_or(path)
        .to_owned()
}

/// Undoes git's C-style quoting of unusual paths (`"caf\303\251.txt"`).
fn unquote(path: &str) -> String {
    let Some(inner) = path.strip_prefix('"').and_then(|p| p.strip_suffix('"')) else {
        return path.to_owned();
    };
    let mut bytes = Vec::new();
    let mut chars = inner.bytes().peekable();
    while let Some(b) = chars.next() {
        if b != b'\\' {
            bytes.push(b);
            continue;
        }
        match chars.next() {
            Some(b'n') => bytes.push(b'\n'),
            Some(b't') => bytes.push(b'\t'),
            Some(d @ b'0'..=b'7') => {
                let mut value = u32::from(d - b'0');
                for _ in 0..2 {
                    if let Some(&n @ b'0'..=b'7') = chars.peek() {
                        value = value * 8 + u32::from(n - b'0');
                        chars.next();
                    }
                }
                bytes.push(value as u8);
            }
            Some(other) => bytes.push(other),
            None => {}
        }
    }
    String::from_utf8_lossy(&bytes).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    const DIFF: &str = "diff --git a/src/app.rs b/src/app.rs
index 1111111..2222222 100644
--- a/src/app.rs
+++ b/src/app.rs
@@ -1,4 +1,5 @@ fn main() {
 one
-two
+TWO
+two and a half
 three
 four
@@ -10,2 +11,2 @@
 ten
-eleven
+ELEVEN
\\ No newline at end of file
";

    #[test]
    fn parses_hunks_and_line_numbers() {
        let files = parse(DIFF).unwrap();
        assert_eq!(files.len(), 1);
        let file = &files[0];
        assert_eq!(file.path, "src/app.rs");
        assert_eq!(file.hunks.len(), 2);
        let first = &file.hunks[0];
        assert_eq!(
            (
                first.old_start,
                first.old_lines,
                first.new_start,
                first.new_lines
            ),
            (1, 4, 1, 5)
        );
        let kinds: Vec<_> = first.lines.iter().map(|l| l.kind).collect();
        use LineKind::*;
        assert_eq!(kinds, [Context, Removed, Added, Added, Context, Context]);
        assert_eq!(first.lines[1].old_line, Some(2));
        assert_eq!(first.lines[1].new_line, None);
        assert_eq!(first.lines[3].new_line, Some(3));
        assert_eq!(first.lines[4].old_line, Some(3));
        assert_eq!(first.lines[4].new_line, Some(4));
        assert!(file.hunks[1].lines[2].no_newline);
    }

    #[test]
    fn forward_patch_keeps_only_picked_lines() {
        let file = &parse(DIFF).unwrap()[0];
        // Stage "+TWO" only: "-two" stays as context, the other addition is dropped.
        let patch =
            partial_patch(file, &[LineRef { hunk: 0, line: 2 }], PatchMode::Forward).unwrap();
        assert!(
            patch.ends_with("@@ -1,4 +1,5 @@\n one\n two\n+TWO\n three\n four\n"),
            "{patch}"
        );
    }

    #[test]
    fn reverse_patch_turns_unpicked_additions_into_context() {
        let file = &parse(DIFF).unwrap()[0];
        let patch =
            partial_patch(file, &[LineRef { hunk: 1, line: 2 }], PatchMode::Reverse).unwrap();
        assert!(
            patch.ends_with("@@ -10,1 +10,2 @@\n ten\n+ELEVEN\n\\ No newline at end of file\n"),
            "{patch}"
        );
    }

    #[test]
    fn nothing_picked_means_no_patch() {
        let file = &parse(DIFF).unwrap()[0];
        assert!(partial_patch(file, &[LineRef { hunk: 0, line: 0 }], PatchMode::Forward).is_none());
    }

    #[test]
    fn untracked_files_become_one_added_hunk() {
        let diff = untracked("notes.txt", b"a\nb");
        assert_eq!(diff.hunks[0].lines.len(), 2);
        assert!(diff.hunks[0].lines[1].no_newline);
        assert_eq!(picked_content(&diff, &[LineRef { hunk: 0, line: 1 }]), b"b");
        assert!(untracked("logo.png", b"\x89PNG\0\0").binary);
    }

    #[test]
    fn unquotes_octal_paths() {
        assert_eq!(unquote("\"caf\\303\\251.txt\""), "café.txt");
    }
}
