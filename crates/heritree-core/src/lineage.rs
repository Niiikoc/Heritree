//! Branch lineage: which branch each branch was created from.
//!
//! Git does not record this, so Heritree works it out from these signals, most
//! reliable first:
//!
//! 1. `branch.<name>.heritree-parent`, written when Heritree creates a branch.
//! 2. The branch's reflog, whose first entry often reads `branch: Created from X`.
//! 3. The branch's upstream (`feature/x` tracking `origin/develop`), then
//!    naming conventions (`hotfix/*` comes from `release/*` or `main`).

use std::collections::{HashMap, HashSet};

use serde::Serialize;

use crate::{RefKind, Repository, Result};

const CONFIG_KEY: &str = "heritree-parent";

/// How a parent was found, from most to least certain.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum LineageSource {
    Recorded,
    Reflog,
    Upstream,
    Convention,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BranchLineage {
    pub name: String,
    /// `None` for root branches such as `main`, or when nothing points to a parent.
    pub parent: Option<String>,
    pub source: Option<LineageSource>,
    /// Where the branch left its parent (their merge base).
    pub fork_point: Option<String>,
    pub fork_time: Option<i64>,
    /// Commits on this branch that the parent does not have.
    pub ahead: u32,
    /// Commits on the parent since the fork.
    pub behind: u32,
}

const ROOTS: &[&str] = &["main", "master", "trunk"];

impl Repository {
    pub fn lineage(&self) -> Result<Vec<BranchLineage>> {
        let branches: Vec<String> = self
            .refs()?
            .into_iter()
            .filter(|r| r.kind == RefKind::LocalBranch)
            .map(|r| r.short_name)
            .collect();
        let local: HashSet<&str> = branches.iter().map(String::as_str).collect();
        let config = self.branch_config();

        let mut parents: HashMap<&str, (String, LineageSource)> = HashMap::new();
        for name in &branches {
            let found = config
                .get(&(name.clone(), CONFIG_KEY.to_owned()))
                .filter(|p| p.as_str() != name)
                .map(|p| (p.clone(), LineageSource::Recorded))
                .or_else(|| {
                    self.reflog_origin(name)
                        .and_then(|from| resolve(&from, name, &local))
                        .map(|p| (p, LineageSource::Reflog))
                })
                .or_else(|| {
                    let merge = config.get(&(name.clone(), "merge".to_owned()))?;
                    let upstream = merge.strip_prefix("refs/heads/")?;
                    (upstream != name && local.contains(upstream))
                        .then(|| (upstream.to_owned(), LineageSource::Upstream))
                })
                .or_else(|| {
                    self.by_convention(name, &branches)
                        .map(|p| (p, LineageSource::Convention))
                });
            if let Some(found) = found {
                parents.insert(name, found);
            }
        }
        break_cycles(&mut parents);

        let mut result = Vec::new();
        for name in &branches {
            let mut entry = BranchLineage {
                name: name.clone(),
                parent: None,
                source: None,
                fork_point: None,
                fork_time: None,
                ahead: 0,
                behind: 0,
            };
            if let Some((parent, source)) = parents.get(name.as_str()) {
                entry.parent = Some(parent.clone());
                entry.source = Some(*source);
                if let Some((id, time)) = self.merge_base(parent, name) {
                    entry.fork_point = Some(id);
                    entry.fork_time = Some(time);
                }
                let range = format!("{parent}...{name}");
                if let Ok(counts) = self.git(&["rev-list", "--left-right", "--count", &range]) {
                    let mut n = counts.split_whitespace().map(|c| c.parse().unwrap_or(0));
                    entry.behind = n.next().unwrap_or(0);
                    entry.ahead = n.next().unwrap_or(0);
                }
            }
            result.push(entry);
        }
        Ok(result)
    }

    /// `branch.<name>.<key>` values, keyed by (name, key).
    fn branch_config(&self) -> HashMap<(String, String), String> {
        let out = self
            .git(&["config", "--get-regexp", r"^branch\."])
            .unwrap_or_default();
        out.lines()
            .filter_map(|line| {
                let (key, value) = line.split_once(' ')?;
                let rest = key.strip_prefix("branch.")?;
                // Branch names may contain dots; the key never does.
                let (name, key) = rest.rsplit_once('.')?;
                Some(((name.to_owned(), key.to_owned()), value.to_owned()))
            })
            .collect()
    }

    /// What the first reflog entry says the branch was created from.
    fn reflog_origin(&self, name: &str) -> Option<String> {
        let path = self.inner.common_dir().join("logs/refs/heads").join(name);
        let log = std::fs::read_to_string(path).ok()?;
        let first = log.lines().next()?;
        let (_, message) = first.split_once('\t')?;
        message
            .strip_prefix("branch: Created from ")
            .map(|from| from.trim().to_owned())
    }

    /// Picks the conventional parent with the most recent common commit.
    fn by_convention(&self, name: &str, branches: &[String]) -> Option<String> {
        if ROOTS.contains(&name) {
            return None;
        }
        let named = |names: &[&str]| -> Vec<String> {
            names
                .iter()
                .filter(|n| branches.iter().any(|b| b == *n))
                .map(|n| (*n).to_owned())
                .collect()
        };
        let releases: Vec<String> = branches
            .iter()
            .filter(|b| b.starts_with("release/") || b.starts_with("releases/"))
            .cloned()
            .collect();
        let candidates: Vec<String> = if name.starts_with("hotfix/") {
            releases.into_iter().chain(named(ROOTS)).collect()
        } else if name.starts_with("release/") || name.starts_with("releases/") {
            named(&["develop", "main", "master", "trunk"])
        } else if matches!(name, "develop" | "dev" | "development") {
            named(ROOTS)
        } else {
            named(&["develop", "dev", "main", "master", "trunk"])
        };
        candidates
            .into_iter()
            .filter(|c| c != name)
            .filter_map(|c| self.merge_base(&c, name).map(|(_, time)| (c, time)))
            // Latest fork wins; on a tie, the earlier (more specific) candidate.
            .fold(None, |best: Option<(String, i64)>, (c, time)| match best {
                Some((_, t)) if t >= time => best,
                _ => Some((c, time)),
            })
            .map(|(c, _)| c)
    }

    fn merge_base(&self, a: &str, b: &str) -> Option<(String, i64)> {
        let id = self.git(&["merge-base", a, b]).ok()?.trim().to_owned();
        let time = self
            .git(&["show", "-s", "--format=%ct", &id])
            .ok()?
            .trim()
            .parse()
            .ok()?;
        Some((id, time))
    }
}

/// Turns a reflog `Created from X` into a local branch name, if X is one.
fn resolve(from: &str, name: &str, local: &HashSet<&str>) -> Option<String> {
    let from = from
        .strip_prefix("refs/heads/")
        .or_else(|| from.strip_prefix("refs/remotes/"))
        .unwrap_or(from);
    if from != name && local.contains(from) {
        return Some(from.to_owned());
    }
    // `origin/develop` → `develop`.
    let (_, rest) = from.split_once('/')?;
    (rest != name && local.contains(rest)).then(|| rest.to_owned())
}

/// Drops the parent link that closes a loop (`a → b → a`), keeping the rest.
fn break_cycles(parents: &mut HashMap<&str, (String, LineageSource)>) {
    let names: Vec<&str> = parents.keys().copied().collect();
    for start in names {
        let mut seen = HashSet::new();
        let mut current = start.to_owned();
        while let Some((parent, _)) = parents.get(current.as_str()) {
            if !seen.insert(current.clone()) {
                break;
            }
            if parent == start {
                parents.remove(current.as_str());
                break;
            }
            current = parent.clone();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_reflog_names() {
        let local: HashSet<&str> = ["main", "develop", "release/2.4"].into();
        assert_eq!(
            resolve("release/2.4", "hotfix/1", &local).as_deref(),
            Some("release/2.4")
        );
        assert_eq!(
            resolve("refs/heads/main", "x", &local).as_deref(),
            Some("main")
        );
        assert_eq!(
            resolve("origin/develop", "x", &local).as_deref(),
            Some("develop")
        );
        assert_eq!(resolve("HEAD", "x", &local), None);
        assert_eq!(resolve("main", "main", &local), None);
    }

    #[test]
    fn breaks_loops() {
        let mut parents = HashMap::new();
        parents.insert("a", ("b".to_owned(), LineageSource::Convention));
        parents.insert("b", ("a".to_owned(), LineageSource::Convention));
        parents.insert("c", ("a".to_owned(), LineageSource::Reflog));
        break_cycles(&mut parents);
        assert_eq!(parents.len(), 2);
        assert!(parents.contains_key("c"));
    }
}
