//! The coverage map built from what was read: the changed files on the left, their hunks in
//! the middle marked by what the report says of their lines, and on the right the tests that
//! ran them (only when the report records tests and matches the revision) and what the test
//! lane said of the commit.

use egui::{Color32, Pos2};

use super::super::graph::{self, Arrange, Edge, Graph, Node, NodeShape};
use crate::git::Verdict;
use crate::insights::Evidence;
use crate::insights::coverage::{Counts, Coverage, HunkCov};
use crate::theme::Tokens;

/// At most so many files and hunks are drawn; the tables list them all.
pub(super) const MAX_FILES: usize = 60;
pub(super) const MAX_HUNKS: usize = 240;

/// What a node stands for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum NodeRef {
    File(usize),
    /// A file's hunk.
    Hunk(usize, usize),
    /// A test the report names.
    Test(usize),
    /// A test lane result.
    Lane(usize),
}

#[derive(Clone, Debug, Default)]
pub(crate) struct Laid {
    pub(crate) graph: Graph,
    pub(crate) positions: Vec<Pos2>,
    pub(crate) nodes: Vec<NodeRef>,
    /// Hunks left out of the picture for room.
    pub(crate) hidden: usize,
}

/// How a hunk is marked: its badge and colour, by the worst of what is known of its lines.
pub(super) fn hunk_mark(t: &Tokens, c: Counts) -> (&'static str, Color32) {
    if c.unknown > 0 {
        ("?", t.text_faint)
    } else if c.uncovered > 0 {
        ("!", t.danger)
    } else if c.covered > 0 {
        ("C", t.success)
    } else {
        ("-", t.text_dim)
    }
}

/// `L12-18`, or where lines were only removed.
pub(super) fn hunk_label(h: &HunkCov) -> String {
    match h.hunk.new_lines() {
        Some((a, b)) if a == b => format!("L{a}"),
        Some((a, b)) => format!("L{a}-{b}"),
        None => format!("L{} (removed)", h.hunk.new_start),
    }
}

pub(super) fn counts_text(c: Counts) -> String {
    format!(
        "{} covered · {} uncovered · {} not instrumented · {} unknown",
        c.covered, c.uncovered, c.not_instrumented, c.unknown
    )
}

/// The trust in the report, as its edges and hunks are drawn.
pub(super) fn evidence(data: &Coverage) -> Evidence {
    data.report
        .as_ref()
        .map_or(Evidence::Unknown, |r| r.freshness.evidence)
}

/// Builds the map of `data`; `test`: the test whose hunks stand out.
pub(crate) fn build(data: &Coverage, test: Option<usize>, t: &Tokens) -> Laid {
    let mut laid = Laid::default();
    let trust = evidence(data);
    let tests: &[String] = data.report.as_ref().map_or(&[], |r| &r.tests);
    let mut test_node: Vec<Option<usize>> = vec![None; tests.len()];
    let mut hunks_drawn = 0;
    for (fi, file) in data.files.iter().enumerate().take(MAX_FILES) {
        let mut node = Node::new(file.path.clone(), t.text_dim);
        node.shape = NodeShape::Pill;
        node.badge = if file.added {
            Some("+".to_owned())
        } else if file.deleted {
            Some("-".to_owned())
        } else {
            None
        };
        let mut total = Counts::default();
        for h in &file.hunks {
            total.add(h.counts());
        }
        node.tip = format!(
            "{} hunk{} · {} · {}",
            file.hunks.len(),
            if file.hunks.len() == 1 { "" } else { "s" },
            match file.in_report {
                None => "no report",
                Some(false) => "not in the report",
                Some(true) if file.by_suffix => "in the report (matched by the end of its name)",
                Some(true) => "in the report",
            },
            counts_text(total)
        );
        let file_node = laid.graph.nodes.len();
        laid.graph.nodes.push(node);
        laid.nodes.push(NodeRef::File(fi));
        for (hi, h) in file.hunks.iter().enumerate() {
            if hunks_drawn == MAX_HUNKS {
                laid.hidden += 1;
                continue;
            }
            hunks_drawn += 1;
            let c = h.counts();
            let (badge, color) = hunk_mark(t, c);
            let mut node = Node::new(hunk_label(h), color);
            node.shape = NodeShape::Square;
            node.column = 1;
            node.badge = Some(badge.to_owned());
            node.evidence = trust;
            node.faded = test.is_some_and(|x| !h.tests.contains(&x));
            let names: Vec<&str> = h
                .tests
                .iter()
                .filter_map(|&i| tests.get(i).map(String::as_str))
                .collect();
            node.tip = format!(
                "{}:{} · {}{}",
                file.path,
                hunk_label(h),
                counts_text(c),
                if names.is_empty() {
                    String::new()
                } else {
                    format!("\nran by {}", names.join(", "))
                }
            );
            let hunk_node = laid.graph.nodes.len();
            laid.graph.nodes.push(node);
            laid.nodes.push(NodeRef::Hunk(fi, hi));
            let mut edge = Edge::new(file_node, hunk_node);
            edge.tip = format!("{} changes {}", file.path, hunk_label(h));
            laid.graph.edges.push(edge);
            for &ti in &h.tests {
                let Some(name) = tests.get(ti) else {
                    continue;
                };
                let at = match test_node[ti] {
                    Some(at) => at,
                    None => {
                        let mut node = Node::new(name.clone(), t.accent);
                        node.column = 2;
                        node.badge = Some("T".to_owned());
                        node.evidence = trust;
                        node.faded = test.is_some_and(|x| x != ti);
                        node.tip = format!("{name}: a test the report names (lcov TN)");
                        test_node[ti] = Some(laid.graph.nodes.len());
                        laid.graph.nodes.push(node);
                        laid.nodes.push(NodeRef::Test(ti));
                        laid.graph.nodes.len() - 1
                    }
                };
                let mut edge = Edge::new(hunk_node, at);
                edge.evidence = trust;
                edge.color = Some(t.accent);
                edge.tip = format!("{name} ran lines of {} (the report says so)", hunk_label(h));
                laid.graph.edges.push(edge);
            }
        }
    }
    laid.hidden += data
        .files
        .iter()
        .skip(MAX_FILES)
        .map(|f| f.hunks.len())
        .sum::<usize>();
    for (i, lane) in data.lane.iter().enumerate() {
        let (badge, color) = match lane.verdict {
            Verdict::Good => ("ok", t.success),
            Verdict::Bad => ("x", t.danger),
        };
        let mut node = Node::new(format!("lane: {}", lane.name), color);
        node.shape = NodeShape::Diamond;
        node.column = 2;
        node.badge = Some(badge.to_owned());
        node.tip = format!(
            "The test lane's {} test said {} of {}{}",
            lane.name,
            lane.verdict.word(),
            crate::repo::short(&data.sha),
            if data.ask == crate::insights::coverage::Ask::WorkTree {
                " (HEAD, not the uncommitted edits)"
            } else {
                ""
            }
        );
        laid.graph.nodes.push(node);
        laid.nodes.push(NodeRef::Lane(i));
    }
    laid.positions = graph::arrange(&laid.graph, Arrange::Columns);
    laid
}
