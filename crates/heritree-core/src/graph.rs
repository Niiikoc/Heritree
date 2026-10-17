//! Commit graph layout: assigns every commit a lane (column) and describes the
//! line segments the UI draws in its row.
//!
//! Each row is split at the commit's dot. An [`EdgeKind::In`] segment runs from
//! the top of the row into the dot, [`EdgeKind::Out`] from the dot to the bottom
//! of the row, and [`EdgeKind::Through`] crosses the row without touching it.
//! Drawing every row's segments gives the full graph, so the UI can render any
//! window of rows on its own (virtual scrolling).

use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap};

use serde::Serialize;

use crate::CommitInfo;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum EdgeKind {
    /// From lane `from` at the top of the row into the commit dot at lane `to`.
    In,
    /// From the commit dot at lane `from` to lane `to` at the bottom of the row.
    Out,
    /// Crosses the row from lane `from` at the top to lane `to` at the bottom.
    Through,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Edge {
    pub from: usize,
    pub to: usize,
    pub kind: EdgeKind,
    /// Colour slot; the UI maps it onto its palette (`color % palette.len()`).
    pub color: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphRow {
    /// Lane of this row's commit dot.
    pub lane: usize,
    pub color: usize,
    pub edges: Vec<Edge>,
    /// Number of lanes this row needs to draw all of its edges.
    pub width: usize,
}

/// Orders commits so every child comes before its parents, newest first among
/// the commits that are ready. This is `git log --date-order`, and it keeps
/// the layout correct even when commit timestamps are skewed.
pub fn topo_order(commits: Vec<CommitInfo>) -> Vec<CommitInfo> {
    let index: HashMap<&str, usize> = commits
        .iter()
        .enumerate()
        .map(|(i, c)| (c.id.as_str(), i))
        .collect();

    let mut pending_children = vec![0usize; commits.len()];
    for commit in &commits {
        for parent in &commit.parents {
            if let Some(&p) = index.get(parent.as_str()) {
                pending_children[p] += 1;
            }
        }
    }

    // Max-heap on commit time; ties keep the input order.
    let mut ready: BinaryHeap<(i64, Reverse<usize>)> = pending_children
        .iter()
        .enumerate()
        .filter(|(_, n)| **n == 0)
        .map(|(i, _)| (commits[i].commit_time, Reverse(i)))
        .collect();

    let mut order = Vec::with_capacity(commits.len());
    while let Some((_, Reverse(i))) = ready.pop() {
        order.push(i);
        for parent in &commits[i].parents {
            if let Some(&p) = index.get(parent.as_str()) {
                pending_children[p] -= 1;
                if pending_children[p] == 0 {
                    ready.push((commits[p].commit_time, Reverse(p)));
                }
            }
        }
    }
    drop(index);

    let mut slots: Vec<Option<CommitInfo>> = commits.into_iter().map(Some).collect();
    order.into_iter().filter_map(|i| slots[i].take()).collect()
}

/// Lays out commits that are already in [`topo_order`].
///
/// A parent outside the given commits keeps its lane open to the bottom of the
/// graph, which is what you want when only the newest N commits are loaded.
pub fn layout(commits: &[CommitInfo]) -> Vec<GraphRow> {
    // lanes[i] = the commit lane i is waiting for.
    let mut lanes: Vec<Option<&str>> = Vec::new();
    let mut colors: Vec<usize> = Vec::new();
    let mut next_color = 0;
    let mut rows = Vec::with_capacity(commits.len());

    for commit in commits {
        let width_before = lanes.len();
        let waiting: Vec<usize> = lanes
            .iter()
            .enumerate()
            .filter(|(_, l)| **l == Some(commit.id.as_str()))
            .map(|(i, _)| i)
            .collect();
        let passing: Vec<usize> = lanes
            .iter()
            .enumerate()
            .filter(|(i, l)| l.is_some() && !waiting.contains(i))
            .map(|(i, _)| i)
            .collect();

        let lane = match waiting.first() {
            Some(&lane) => lane,
            // A branch tip nobody pointed at yet: open a new lane for it.
            None => {
                let lane = free_lane(&mut lanes, &mut colors);
                colors[lane] = next_color;
                next_color += 1;
                lane
            }
        };
        let color = colors[lane];

        let mut edges = Vec::new();
        for &i in &waiting {
            edges.push(Edge {
                from: i,
                to: lane,
                kind: EdgeKind::In,
                color: colors[i],
            });
            lanes[i] = None;
        }

        for (n, parent) in commit.parents.iter().enumerate() {
            let expected_at = lanes.iter().position(|l| *l == Some(parent.as_str()));
            if n == 0 {
                match expected_at {
                    // A lane to our left already leads to this parent: join it.
                    Some(k) if k < lane => edges.push(Edge {
                        from: lane,
                        to: k,
                        kind: EdgeKind::Out,
                        color,
                    }),
                    // Otherwise keep going straight down, so a long-lived branch
                    // like `main` keeps its column; the lines meet at the parent.
                    _ => {
                        lanes[lane] = Some(parent.as_str());
                        edges.push(Edge {
                            from: lane,
                            to: lane,
                            kind: EdgeKind::Out,
                            color,
                        });
                    }
                }
            } else if let Some(k) = expected_at {
                edges.push(Edge {
                    from: lane,
                    to: k,
                    kind: EdgeKind::Out,
                    color: colors[k],
                });
            } else {
                // Merge from a branch we have not drawn yet: give it a lane.
                let k = free_lane(&mut lanes, &mut colors);
                lanes[k] = Some(parent.as_str());
                colors[k] = next_color;
                next_color += 1;
                edges.push(Edge {
                    from: lane,
                    to: k,
                    kind: EdgeKind::Out,
                    color: colors[k],
                });
            }
        }

        for i in passing {
            edges.push(Edge {
                from: i,
                to: i,
                kind: EdgeKind::Through,
                color: colors[i],
            });
        }

        let width = width_before.max(lanes.len()).max(lane + 1);
        while lanes.last() == Some(&None) {
            lanes.pop();
        }
        colors.truncate(lanes.len().max(1));

        rows.push(GraphRow {
            lane,
            color,
            edges,
            width,
        });
    }
    rows
}

fn free_lane(lanes: &mut Vec<Option<&str>>, colors: &mut Vec<usize>) -> usize {
    match lanes.iter().position(Option::is_none) {
        Some(i) => i,
        None => {
            lanes.push(None);
            colors.resize(lanes.len(), 0);
            lanes.len() - 1
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use EdgeKind::*;

    fn commit(id: &str, parents: &[&str], time: i64) -> CommitInfo {
        CommitInfo {
            id: id.into(),
            short_id: id.into(),
            parents: parents.iter().map(|p| p.to_string()).collect(),
            summary: String::new(),
            author_name: String::new(),
            author_email: String::new(),
            author_time: time,
            commit_time: time,
        }
    }

    fn ids(commits: &[CommitInfo]) -> Vec<&str> {
        commits.iter().map(|c| c.id.as_str()).collect()
    }

    #[test]
    fn linear_history_stays_in_one_lane() {
        let commits = vec![
            commit("c", &["b"], 3),
            commit("b", &["a"], 2),
            commit("a", &[], 1),
        ];
        let rows = layout(&commits);
        assert!(rows.iter().all(|r| r.lane == 0 && r.width == 1));
        assert_eq!(
            rows[0].edges,
            vec![Edge {
                from: 0,
                to: 0,
                kind: Out,
                color: 0
            }]
        );
        assert_eq!(
            rows[1].edges,
            vec![
                Edge {
                    from: 0,
                    to: 0,
                    kind: In,
                    color: 0
                },
                Edge {
                    from: 0,
                    to: 0,
                    kind: Out,
                    color: 0
                }
            ]
        );
        assert_eq!(
            rows[2].edges,
            vec![Edge {
                from: 0,
                to: 0,
                kind: In,
                color: 0
            }]
        );
    }

    #[test]
    fn branch_and_merge() {
        //   m        merge of main (b) and feature (f)
        //   |\
        //   | f
        //   b |
        //   |/
        //   a
        let commits = vec![
            commit("m", &["b", "f"], 5),
            commit("f", &["a"], 4),
            commit("b", &["a"], 3),
            commit("a", &[], 1),
        ];
        let rows = layout(&commits);

        assert_eq!(rows[0].lane, 0);
        assert_eq!(
            rows[0].edges,
            vec![
                Edge {
                    from: 0,
                    to: 0,
                    kind: Out,
                    color: 0
                },
                Edge {
                    from: 0,
                    to: 1,
                    kind: Out,
                    color: 1
                }
            ]
        );
        // The feature commit sits in its own lane, main passes beside it.
        assert_eq!(rows[1].lane, 1);
        assert!(rows[1].edges.contains(&Edge {
            from: 0,
            to: 0,
            kind: Through,
            color: 0
        }));
        // f's parent `a` is not expected yet, so f keeps lane 1 for it.
        assert!(rows[1].edges.contains(&Edge {
            from: 1,
            to: 1,
            kind: Out,
            color: 1
        }));
        // b also leads to `a` but stays in main's column...
        assert_eq!(rows[2].lane, 0);
        assert!(rows[2].edges.contains(&Edge {
            from: 0,
            to: 0,
            kind: Out,
            color: 0
        }));
        // ...and both lines meet at `a`, which lands in the leftmost lane.
        assert_eq!(rows[3].lane, 0);
        assert_eq!(
            rows[3].edges,
            vec![
                Edge {
                    from: 0,
                    to: 0,
                    kind: In,
                    color: 0
                },
                Edge {
                    from: 1,
                    to: 0,
                    kind: In,
                    color: 1
                }
            ]
        );
        assert!(rows.iter().all(|r| r.width <= 2));
    }

    #[test]
    fn two_tips_get_two_colors() {
        let commits = vec![
            commit("x", &["a"], 3),
            commit("y", &["a"], 2),
            commit("a", &[], 1),
        ];
        let rows = layout(&commits);
        assert_eq!((rows[0].lane, rows[0].color), (0, 0));
        assert_eq!((rows[1].lane, rows[1].color), (1, 1));
        // y joins the lane that already waits for `a`.
        assert!(rows[1].edges.contains(&Edge {
            from: 1,
            to: 0,
            kind: Out,
            color: 1
        }));
        assert_eq!(rows[2].lane, 0);
    }

    #[test]
    fn parents_outside_the_window_keep_their_lane_open() {
        let commits = vec![commit("b", &["a"], 2)];
        let rows = layout(&commits);
        assert_eq!(
            rows[0].edges,
            vec![Edge {
                from: 0,
                to: 0,
                kind: Out,
                color: 0
            }]
        );
    }

    #[test]
    fn topo_order_puts_children_first_despite_clock_skew() {
        // `b` claims to be older than its parent `a`.
        let commits = vec![
            commit("a", &[], 10),
            commit("b", &["a"], 5),
            commit("c", &["b"], 20),
        ];
        assert_eq!(ids(&topo_order(commits)), vec!["c", "b", "a"]);
    }

    #[test]
    fn topo_order_interleaves_branches_by_date() {
        let commits = vec![
            commit("a", &[], 1),
            commit("f1", &["a"], 2),
            commit("m1", &["a"], 3),
            commit("f2", &["f1"], 4),
            commit("m2", &["m1"], 5),
        ];
        assert_eq!(ids(&topo_order(commits)), vec!["m2", "f2", "m1", "f1", "a"]);
    }
}
