//! The sample history shared by graph settings previews.

use crate::git::Commit;
use crate::graph::{self, Graph};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum RefKind {
    /// The branch checked out.
    Head,
    Local,
    Remote,
    Tag,
}

pub(super) struct Mock {
    pub(super) hash: &'static str,
    pub(super) parents: &'static [&'static str],
    pub(super) summary: &'static str,
    pub(super) author: &'static str,
    pub(super) hash7: &'static str,
    pub(super) when: &'static str,
    pub(super) date: &'static str,
    pub(super) refs: &'static [(&'static str, RefKind)],
}

/// A short history: a feature branch, a merged fix, a release tag.
pub(super) const HISTORY: [Mock; 7] = [
    Mock {
        hash: "a",
        parents: &["c"],
        summary: "interaction-preview-mock-summary-1",
        author: "Ada Lovelace",
        hash7: "3f9a2c1",
        when: "interaction-preview-mock-when-1",
        date: "2026-11-06 14:02",
        refs: &[("feature/settings-previews", RefKind::Head)],
    },
    Mock {
        hash: "b",
        parents: &["d", "e"],
        summary: "interaction-preview-mock-summary-2",
        author: "Grace Hopper",
        hash7: "b81e07d",
        when: "interaction-preview-mock-when-2",
        date: "2026-11-06 12:20",
        refs: &[("main", RefKind::Local), ("origin/main", RefKind::Remote)],
    },
    Mock {
        hash: "c",
        parents: &["d"],
        summary: "interaction-preview-mock-summary-3",
        author: "Ada Lovelace",
        hash7: "77c4e90",
        when: "interaction-preview-mock-when-3",
        date: "2026-11-05 18:44",
        refs: &[],
    },
    Mock {
        hash: "e",
        parents: &["d"],
        summary: "interaction-preview-mock-summary-4",
        author: "Linus Torvalds",
        hash7: "0d5be33",
        when: "interaction-preview-mock-when-4",
        date: "2026-11-04 09:13",
        refs: &[(
            "origin/fix/tab-strip-overflows-in-small-windows",
            RefKind::Remote,
        )],
    },
    Mock {
        hash: "d",
        parents: &["f"],
        summary: "interaction-preview-mock-summary-5",
        author: "Grace Hopper",
        hash7: "e2a9f41",
        when: "interaction-preview-mock-when-5",
        date: "2026-10-30 16:00",
        refs: &[("v0.9.0", RefKind::Tag)],
    },
    Mock {
        hash: "f",
        parents: &["g"],
        summary: "interaction-preview-mock-summary-6",
        author: "Linus Torvalds",
        hash7: "5c0d8b2",
        when: "interaction-preview-mock-when-6",
        date: "2026-10-22 11:31",
        refs: &[],
    },
    Mock {
        hash: "g",
        parents: &[],
        summary: "interaction-preview-mock-summary-7",
        author: "Ada Lovelace",
        hash7: "9b7f3aa",
        when: "interaction-preview-mock-when-7",
        date: "2026-10-01 08:00",
        refs: &[],
    },
];

pub(super) fn history_graph() -> Graph {
    let commits: Vec<Commit> = HISTORY
        .iter()
        .map(|m| Commit {
            hash: m.hash.to_owned(),
            parents: m.parents.iter().map(|p| (*p).to_owned()).collect(),
            ..Commit::default()
        })
        .collect();
    graph::layout(&commits)
}
