//! `heritree`: a thin command-line front end to heritree-core.
//!
//! It exists to exercise the engine without the desktop app: handy when
//! debugging the graph layout on a real repository, and in tests.

use std::collections::HashMap;
use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use heritree_core::{EdgeKind, GraphRow, RefInfo, Repository, graph};

#[derive(Parser)]
#[command(
    name = "heritree",
    version,
    about = "Inspect a repository the way the Heritree app sees it"
)]
struct Cli {
    /// Repository path (any folder inside it works).
    #[arg(short = 'C', long = "repo", default_value = ".", global = true)]
    repo: PathBuf,

    /// Print JSON instead of text, exactly what the app receives.
    #[arg(long, global = true)]
    json: bool,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Show the repository name, paths and HEAD.
    Info,
    /// List branches, remote branches and tags.
    Refs,
    /// Show the commit graph.
    Log {
        /// How many commits to load.
        #[arg(short = 'n', long, default_value_t = 50)]
        limit: usize,
    },
    /// Show staged and unstaged changes.
    Status,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("error: {err}");
            ExitCode::FAILURE
        }
    }
}

fn run(cli: Cli) -> Result<(), Box<dyn std::error::Error>> {
    let repo = Repository::open(&cli.repo)?;
    match cli.command {
        Command::Info => {
            let info = repo.info()?;
            if cli.json {
                println!("{}", serde_json::to_string_pretty(&info)?);
            } else {
                println!("name:    {}", info.name);
                println!("git dir: {}", info.git_dir);
                println!("workdir: {}", info.workdir.as_deref().unwrap_or("(bare)"));
                let head = match (&info.head.branch, &info.head.id) {
                    (Some(branch), _) if info.head.unborn => format!("{branch} (no commits yet)"),
                    (Some(branch), Some(id)) => format!("{branch} at {}", &id[..7]),
                    (None, Some(id)) => format!("detached at {}", &id[..7]),
                    _ => "unknown".to_owned(),
                };
                println!("head:    {head}");
            }
        }
        Command::Refs => {
            let refs = repo.refs()?;
            if cli.json {
                println!("{}", serde_json::to_string_pretty(&refs)?);
            } else {
                for r in refs {
                    println!(
                        "{:<14} {}  {}",
                        format!("{:?}", r.kind),
                        &r.target[..7],
                        r.short_name
                    );
                }
            }
        }
        Command::Log { limit } => {
            let commits = repo.log(limit)?;
            let rows = graph::layout(&commits);
            if cli.json {
                let rows: Vec<_> = commits
                    .iter()
                    .zip(&rows)
                    .map(|(commit, graph)| serde_json::json!({ "commit": commit, "graph": graph }))
                    .collect();
                println!("{}", serde_json::to_string_pretty(&rows)?);
            } else {
                let refs = repo.refs()?;
                let labels = ref_labels(&refs);
                let width = rows.iter().map(|r| r.width).max().unwrap_or(1) * 2;
                for (commit, row) in commits.iter().zip(&rows) {
                    let (above, node, below) = draw(row);
                    if let Some(line) = above {
                        println!("{line:<width$}");
                    }
                    let refs = labels
                        .get(commit.id.as_str())
                        .map(|l| format!("({}) ", l.join(", ")))
                        .unwrap_or_default();
                    println!(
                        "{node:<width$} {} {} {refs}{}  [{}]",
                        commit.short_id,
                        date(commit.author_time),
                        commit.summary,
                        commit.author_name
                    );
                    if let Some(line) = below {
                        println!("{line:<width$}");
                    }
                }
            }
        }
        Command::Status => {
            let files = repo.status()?;
            if cli.json {
                println!("{}", serde_json::to_string_pretty(&files)?);
            } else if files.is_empty() {
                println!("nothing to commit, working tree clean");
            } else {
                for f in files {
                    let code = |k: Option<heritree_core::ChangeKind>| {
                        k.map(|k| format!("{k:?}")).unwrap_or_else(|| "-".into())
                    };
                    let from = f
                        .original_path
                        .map(|p| format!(" (from {p})"))
                        .unwrap_or_default();
                    println!(
                        "{:<12} {:<12} {}{from}",
                        code(f.staged),
                        code(f.unstaged),
                        f.path
                    );
                }
            }
        }
    }
    Ok(())
}

fn ref_labels(refs: &[RefInfo]) -> HashMap<&str, Vec<&str>> {
    let mut labels: HashMap<&str, Vec<&str>> = HashMap::new();
    for r in refs {
        labels
            .entry(r.target.as_str())
            .or_default()
            .push(r.short_name.as_str());
    }
    labels
}

/// Draws one graph row as up to three lines of ASCII: the lines converging
/// into the commit, the commit itself, and the lines leaving it sideways.
fn draw(row: &GraphRow) -> (Option<String>, String, Option<String>) {
    let width = row.width * 2;
    let blank = || vec![' '; width];
    let mut node = blank();
    let mut above = blank();
    let mut below = blank();
    let (mut has_above, mut has_below) = (false, false);

    for edge in &row.edges {
        match edge.kind {
            EdgeKind::Through => {
                for line in [&mut above, &mut node, &mut below] {
                    line[edge.from * 2] = '|';
                }
                if edge.from != edge.to {
                    has_below = true;
                    put_diagonal(&mut below, edge.from, edge.to);
                }
            }
            EdgeKind::In if edge.from == edge.to => above[edge.to * 2] = '|',
            EdgeKind::In => {
                has_above = true;
                // Coming from the top-right goes down-left, and vice versa.
                put_diagonal(&mut above, edge.from, edge.to);
            }
            EdgeKind::Out if edge.from == edge.to => below[edge.from * 2] = '|',
            EdgeKind::Out => {
                has_below = true;
                put_diagonal(&mut below, edge.from, edge.to);
            }
        }
    }
    node[row.lane * 2] = '*';

    let text = |chars: Vec<char>| chars.into_iter().collect::<String>().trim_end().to_owned();
    (
        has_above.then(|| text(above)),
        text(node),
        has_below.then(|| text(below)),
    )
}

/// A line from lane `from` (top) to lane `to` (bottom).
fn put_diagonal(line: &mut [char], from: usize, to: usize) {
    let (low, high) = (from.min(to), from.max(to));
    line[low * 2 + 1..high * 2].fill('_');
    line[low * 2 + 1] = if from > to { '/' } else { '\\' };
}

/// Formats Unix seconds as YYYY-MM-DD (UTC).
fn date(seconds: i64) -> String {
    // Howard Hinnant's days-to-civil algorithm.
    let days = seconds.div_euclid(86_400);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_dates() {
        assert_eq!(date(0), "1970-01-01");
        assert_eq!(date(1_767_225_600), "2026-01-01");
        assert_eq!(date(951_782_400), "2000-02-29");
    }
}
