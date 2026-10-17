//! Working-tree status, parsed from `git status --porcelain=v2 -z`.
//!
//! The v2 format is documented in `git help status` under "Porcelain Format
//! Version 2". With `-z`, entries are NUL-terminated and paths are not quoted.

use crate::{ChangeKind, Error, FileStatus, Result};

pub(crate) const ARGS: &[&str] = &["status", "--porcelain=v2", "-z", "--untracked-files=all"];

pub fn parse_porcelain_v2(output: &[u8]) -> Result<Vec<FileStatus>> {
    let text = String::from_utf8_lossy(output);
    let mut entries = text.split('\0').filter(|e| !e.is_empty());
    let mut files = Vec::new();

    while let Some(entry) = entries.next() {
        let bad = || Error::Parse(format!("unexpected status entry: {entry:?}"));
        let (tag, rest) = entry.split_once(' ').ok_or_else(bad)?;
        match tag {
            // 1 <XY> <sub> <mH> <mI> <mW> <hH> <hI> <path>
            "1" => {
                let fields: Vec<&str> = rest.splitn(8, ' ').collect();
                let [xy, .., path] = fields.as_slice() else {
                    return Err(bad());
                };
                if fields.len() != 8 {
                    return Err(bad());
                }
                let (staged, unstaged) = parse_xy(xy).ok_or_else(bad)?;
                files.push(FileStatus {
                    path: path.to_string(),
                    original_path: None,
                    staged,
                    unstaged,
                });
            }
            // 2 <XY> <sub> <mH> <mI> <mW> <hH> <hI> <X><score> <path>, then <origPath> as its own entry
            "2" => {
                let fields: Vec<&str> = rest.splitn(9, ' ').collect();
                let [xy, .., path] = fields.as_slice() else {
                    return Err(bad());
                };
                if fields.len() != 9 {
                    return Err(bad());
                }
                let (staged, unstaged) = parse_xy(xy).ok_or_else(bad)?;
                let original = entries.next().ok_or_else(bad)?;
                files.push(FileStatus {
                    path: path.to_string(),
                    original_path: Some(original.to_owned()),
                    staged,
                    unstaged,
                });
            }
            // u <XY> <sub> <m1> <m2> <m3> <mW> <h1> <h2> <h3> <path>
            "u" => {
                let fields: Vec<&str> = rest.splitn(10, ' ').collect();
                if fields.len() != 10 {
                    return Err(bad());
                }
                files.push(FileStatus {
                    path: fields[9].to_owned(),
                    original_path: None,
                    staged: Some(ChangeKind::Conflicted),
                    unstaged: Some(ChangeKind::Conflicted),
                });
            }
            "?" => files.push(FileStatus {
                path: rest.to_owned(),
                original_path: None,
                staged: None,
                unstaged: Some(ChangeKind::Untracked),
            }),
            // Ignored files and header lines.
            "!" | "#" => {}
            _ => return Err(bad()),
        }
    }
    Ok(files)
}

fn parse_xy(xy: &str) -> Option<(Option<ChangeKind>, Option<ChangeKind>)> {
    let mut chars = xy.chars();
    let (x, y) = (chars.next()?, chars.next()?);
    Some((change(x)?, change(y)?))
}

/// `None` (outer) means an unknown code; `Some(None)` means unchanged (`.`).
fn change(code: char) -> Option<Option<ChangeKind>> {
    Some(match code {
        '.' => None,
        'M' => Some(ChangeKind::Modified),
        'T' => Some(ChangeKind::TypeChanged),
        'A' => Some(ChangeKind::Added),
        'D' => Some(ChangeKind::Deleted),
        'R' => Some(ChangeKind::Renamed),
        'C' => Some(ChangeKind::Copied),
        'U' => Some(ChangeKind::Conflicted),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use ChangeKind::*;

    #[test]
    fn parses_every_entry_kind() {
        let out = b"1 .M N... 100644 100644 100644 aaa aaa src/main.rs\0\
1 A. N... 000000 100644 100644 000 bbb new file.txt\0\
2 R. N... 100644 100644 100644 ccc ccc R100 docs/new.md\0docs/old.md\0\
u UU N... 100644 100644 100644 100644 d1 d2 d3 conflict.rs\0\
? notes/todo.txt\0\
! target/\0";
        let files = parse_porcelain_v2(out).unwrap();
        assert_eq!(
            files,
            vec![
                FileStatus {
                    path: "src/main.rs".into(),
                    original_path: None,
                    staged: None,
                    unstaged: Some(Modified)
                },
                FileStatus {
                    path: "new file.txt".into(),
                    original_path: None,
                    staged: Some(Added),
                    unstaged: None
                },
                FileStatus {
                    path: "docs/new.md".into(),
                    original_path: Some("docs/old.md".into()),
                    staged: Some(Renamed),
                    unstaged: None
                },
                FileStatus {
                    path: "conflict.rs".into(),
                    original_path: None,
                    staged: Some(Conflicted),
                    unstaged: Some(Conflicted)
                },
                FileStatus {
                    path: "notes/todo.txt".into(),
                    original_path: None,
                    staged: None,
                    unstaged: Some(Untracked)
                },
            ]
        );
    }

    #[test]
    fn empty_output_means_clean() {
        assert!(parse_porcelain_v2(b"").unwrap().is_empty());
    }

    #[test]
    fn rejects_garbage() {
        assert!(parse_porcelain_v2(b"1 ZZ broken\0").is_err());
    }
}
