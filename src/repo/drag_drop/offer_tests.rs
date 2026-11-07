//! Which operations a drop offers for which item and target.

use std::collections::HashSet;
use std::sync::Arc;

use super::offers::{Ancestry, Line, Scene, offers};
use super::reorder::Place;
use super::{DragBranch, DragCommit, DragItem, DropTarget, Offer};
use crate::git::Commit;

/// Ancestry given as the pairs (ancestor, descendant) that hold; `unknown` commits aren't
/// loaded.
#[derive(Default)]
struct Known {
    pairs: HashSet<(String, String)>,
    unknown: HashSet<String>,
}

impl Known {
    fn with(pairs: &[(&str, &str)]) -> Self {
        Self {
            pairs: pairs
                .iter()
                .map(|(a, b)| ((*a).to_owned(), (*b).to_owned()))
                .collect(),
            unknown: HashSet::new(),
        }
    }
}

impl Ancestry for Known {
    fn is_ancestor(&self, a: &str, b: &str) -> Option<bool> {
        if self.unknown.contains(a) || self.unknown.contains(b) {
            return None;
        }
        Some(a == b || self.pairs.contains(&(a.to_owned(), b.to_owned())))
    }
}

/// A repository as the drop sees it.
struct World {
    current: Option<&'static str>,
    head: &'static str,
    line: Line,
    elsewhere: Vec<String>,
    known: Known,
}

impl World {
    /// `main` checked out at `m`, with no line of commits.
    fn on_main(known: Known) -> Self {
        Self {
            current: Some("main"),
            head: "m",
            line: Line::default(),
            elsewhere: Vec::new(),
            known,
        }
    }

    fn labels(&self, item: &DragItem, target: &DropTarget) -> Vec<String> {
        self.offers(item, target).iter().map(Offer::label).collect()
    }

    fn offers(&self, item: &DragItem, target: &DropTarget) -> Vec<Offer> {
        let scene = Scene {
            current: self.current,
            head: self.head,
            line: &self.line,
            elsewhere: &self.elsewhere,
            ancestry: &self.known,
        };
        offers(item, target, &scene)
    }
}

fn branch(name: &str, tip: &str) -> DragBranch {
    DragBranch {
        name: name.to_owned(),
        remote: false,
        tip: tip.to_owned(),
    }
}

fn remote(name: &str, tip: &str) -> DragBranch {
    DragBranch {
        remote: true,
        ..branch(name, tip)
    }
}

fn commit(hash: &str) -> DragCommit {
    DragCommit {
        hash: hash.to_owned(),
        summary: format!("commit {hash}"),
        merge: false,
    }
}

fn drag(b: &DragBranch) -> DragItem {
    DragItem::Branch(b.clone())
}

fn on(b: &DragBranch) -> DropTarget {
    DropTarget::Branch(b.clone())
}

fn strings(list: &[&str]) -> Vec<String> {
    list.iter().map(|s| (*s).to_owned()).collect()
}

#[test]
fn a_branch_dropped_on_the_checked_out_one_merges_or_rebases_it() {
    let w = World::on_main(Known::default());
    let main = branch("main", "m");
    let feature = branch("feature", "f");
    assert_eq!(
        w.labels(&drag(&feature), &on(&main)),
        strings(&[
            "Merge feature into main",
            "Rebase main onto feature",
            "Interactive rebase main onto feature…",
        ])
    );
    // Dropped on HEAD's row, it is the same.
    let head_row = DropTarget::Commit(commit("m"));
    assert_eq!(
        w.offers(&drag(&feature), &head_row),
        w.offers(&drag(&feature), &on(&main))
    );
}

#[test]
fn only_a_fast_forward_when_the_checked_out_branch_is_behind() {
    let w = World::on_main(Known::with(&[("m", "f")]));
    assert_eq!(
        w.labels(&drag(&branch("feature", "f")), &on(&branch("main", "m"))),
        strings(&["Fast-forward main to feature"])
    );
}

#[test]
fn a_branch_already_merged_offers_only_the_interactive_rebase() {
    let w = World::on_main(Known::with(&[("f", "m")]));
    assert_eq!(
        w.labels(&drag(&branch("feature", "f")), &on(&branch("main", "m"))),
        strings(&["Interactive rebase main onto feature…"])
    );
}

#[test]
fn ancestry_not_known_offers_no_fast_forward() {
    let mut known = Known::default();
    known.unknown.insert("f".to_owned());
    let w = World::on_main(known);
    let labels = w.labels(&drag(&branch("feature", "f")), &on(&branch("main", "m")));
    assert_eq!(labels.len(), 3);
    assert!(!labels.iter().any(|l| l.starts_with("Fast-forward")));
}

#[test]
fn the_checked_out_branch_dropped_on_another_goes_onto_it() {
    let main = branch("main", "m");
    let w = World::on_main(Known::default());
    assert_eq!(
        w.labels(&drag(&main), &on(&branch("feature", "f"))),
        strings(&[
            "Merge feature into main",
            "Rebase main onto feature",
            "Interactive rebase main onto feature…",
        ])
    );
    // `feature` is behind main: it can be brought up to it without checking it out.
    let w = World::on_main(Known::with(&[("f", "m")]));
    assert_eq!(
        w.labels(&drag(&main), &on(&branch("feature", "f"))),
        strings(&[
            "Interactive rebase main onto feature…",
            "Fast-forward feature to main",
        ])
    );
}

#[test]
fn branches_not_checked_out_only_fast_forward() {
    let topic = branch("topic", "t");
    let other = branch("other", "o");
    let behind = World::on_main(Known::with(&[("o", "t")]));
    assert_eq!(
        behind.labels(&drag(&topic), &on(&other)),
        strings(&["Fast-forward other to topic"])
    );
    assert!(matches!(
        behind.offers(&drag(&topic), &on(&other))[..],
        [Offer::FastForward {
            checked_out: false,
            ..
        }]
    ));
    // Diverged, or the other way round: nothing to do.
    let diverged = World::on_main(Known::default());
    assert!(diverged.offers(&drag(&topic), &on(&other)).is_empty());
    assert!(behind.offers(&drag(&other), &on(&topic)).is_empty());
    // Checked out in another worktree: git won't move it from here.
    let mut elsewhere = World::on_main(Known::with(&[("o", "t")]));
    elsewhere.elsewhere = vec!["other".to_owned()];
    assert!(elsewhere.offers(&drag(&topic), &on(&other)).is_empty());
}

#[test]
fn the_same_branch_or_commit_offers_nothing() {
    let w = World::on_main(Known::default());
    let main = branch("main", "m");
    assert!(w.offers(&drag(&main), &on(&main)).is_empty());
    // Another branch at the same commit.
    assert!(w.offers(&drag(&branch("twin", "m")), &on(&main)).is_empty());
    assert!(
        w.offers(&drag(&main), &DropTarget::Commit(commit("m")))
            .is_empty()
    );
}

#[test]
fn remote_branches_are_brought_in_but_never_moved() {
    let main = branch("main", "m");
    let upstream = remote("origin/main", "u");
    let behind = World::on_main(Known::with(&[("m", "u")]));
    assert_eq!(
        behind.labels(&drag(&upstream), &on(&main)),
        strings(&["Fast-forward main to origin/main"])
    );
    // The checked-out branch onto its remote: rebased onto it, the remote one left alone.
    let ahead = World::on_main(Known::with(&[("u", "m")]));
    assert_eq!(
        ahead.labels(&drag(&main), &on(&upstream)),
        strings(&["Interactive rebase main onto origin/main…"])
    );
    // Another branch onto a remote one: nothing.
    let w = World::on_main(Known::with(&[("u", "t")]));
    assert!(
        w.offers(&drag(&branch("topic", "t")), &on(&upstream))
            .is_empty()
    );
}

#[test]
fn a_detached_head_takes_only_cherry_picks() {
    let w = World {
        current: None,
        ..World::on_main(Known::with(&[("o", "t")]))
    };
    let topic = branch("topic", "t");
    assert!(
        w.offers(&drag(&topic), &DropTarget::DetachedHead)
            .is_empty()
    );
    assert!(
        w.offers(&drag(&topic), &DropTarget::Commit(commit("m")))
            .is_empty()
    );
    // Branches that aren't checked out still fast-forward.
    assert_eq!(
        w.labels(&drag(&topic), &on(&branch("other", "o"))),
        strings(&["Fast-forward other to topic"])
    );
    let item = DragItem::Commit(commit("c"));
    assert_eq!(
        w.labels(&item, &DropTarget::DetachedHead),
        strings(&["Cherry-pick c onto HEAD"])
    );
}

#[test]
fn a_commit_dropped_on_the_checked_out_branch_is_cherry_picked() {
    let w = World::on_main(Known::default());
    let main = branch("main", "m");
    let item = DragItem::Commit(commit("c"));
    assert_eq!(
        w.labels(&item, &on(&main)),
        strings(&["Cherry-pick c onto main"])
    );
    assert_eq!(
        w.labels(&item, &DropTarget::Commit(commit("m"))),
        strings(&["Cherry-pick c onto main"])
    );
    // Not onto another branch, a commit elsewhere, nor a merge commit, nor one main has.
    assert!(w.offers(&item, &on(&branch("feature", "f"))).is_empty());
    assert!(w.offers(&item, &DropTarget::Commit(commit("x"))).is_empty());
    let merge = DragItem::Commit(DragCommit {
        merge: true,
        ..commit("c")
    });
    assert!(w.offers(&merge, &on(&main)).is_empty());
    let has_it = World::on_main(Known::with(&[("c", "m")]));
    assert!(has_it.offers(&item, &on(&main)).is_empty());
}

/// Commits newest first: `(hash, parents)`.
fn line(list: &[(&str, &[&str])]) -> Line {
    Line::new(
        list.iter()
            .map(|(h, p)| {
                Arc::new(Commit {
                    hash: (*h).to_owned(),
                    parents: p.iter().map(|p| (*p).to_owned()).collect(),
                    summary: format!("commit {h}"),
                    ..Commit::default()
                })
            })
            .collect(),
    )
}

/// The places offered for dropping `hash` on `target`, and what they replay onto.
fn places(w: &World, hash: &str, target: &DropTarget) -> Vec<(Place, String)> {
    w.offers(&DragItem::Commit(commit(hash)), target)
        .into_iter()
        .filter_map(|o| match o {
            Offer::Rewrite(d) => Some((d.place, d.base)),
            _ => None,
        })
        .collect()
}

#[test]
fn commits_of_the_branch_move_or_squash_onto_each_other() {
    // main: m, c, b, then the root a; all of them on main already.
    let known = Known::with(&[("c", "m"), ("b", "m"), ("a", "m")]);
    let w = World {
        line: line(&[("m", &["c"]), ("c", &["b"]), ("b", &["a"]), ("a", &[])]),
        ..World::on_main(known)
    };
    let all = |base: &str| {
        [Place::Above, Place::Below, Place::Squash, Place::Fixup]
            .map(|p| (p, base.to_owned()))
            .to_vec()
    };
    // m onto b: replayed from below b.
    assert_eq!(places(&w, "m", &DropTarget::Commit(commit("b"))), all("a"));
    // b onto m (or main's pill): the same base, b being the older.
    assert_eq!(places(&w, "b", &DropTarget::Commit(commit("m"))), all("a"));
    assert_eq!(
        places(&w, "b", &on(&branch("main", "m"))),
        all("a"),
        "main's pill stands for its latest commit"
    );
    // Right above its neighbour below already: it can only go below it, or into it.
    assert_eq!(
        places(&w, "c", &DropTarget::Commit(commit("b"))),
        vec![
            (Place::Below, "a".to_owned()),
            (Place::Squash, "a".to_owned()),
            (Place::Fixup, "a".to_owned()),
        ]
    );
    // The root has nothing below it to replay onto; nor anything onto itself.
    assert!(places(&w, "a", &DropTarget::Commit(commit("c"))).is_empty());
    assert!(places(&w, "c", &DropTarget::Commit(commit("c"))).is_empty());
    // Not a commit of the branch.
    assert!(places(&w, "x", &DropTarget::Commit(commit("c"))).is_empty());
}

#[test]
fn commits_behind_a_merge_are_not_rewritten() {
    // m merges x into c, and every rewrite below it would replay (and flatten) m.
    let known = Known::with(&[("c", "m"), ("b", "m")]);
    let w = World {
        line: line(&[("m", &["c", "x"]), ("c", &["b"]), ("b", &["a"])]),
        ..World::on_main(known)
    };
    assert!(places(&w, "b", &DropTarget::Commit(commit("c"))).is_empty());
    assert!(places(&w, "c", &DropTarget::Commit(commit("m"))).is_empty());
    assert!(places(&w, "b", &DropTarget::Commit(commit("m"))).is_empty());
}

#[test]
fn nothing_is_offered_on_a_detached_head_line() {
    let w = World {
        current: None,
        line: line(&[("m", &["c"]), ("c", &["b"])]),
        ..World::on_main(Known::with(&[("c", "m")]))
    };
    assert!(places(&w, "c", &DropTarget::Commit(commit("m"))).is_empty());
}

#[test]
fn unknown_ancestry_is_not_proof_that_a_commit_needs_cherry_picking() {
    let mut known = Known::default();
    known.unknown.insert("c".to_owned());
    let w = World::on_main(known);
    assert!(
        w.offers(
            &DragItem::Commit(commit("c")),
            &DropTarget::Commit(commit("m"))
        )
        .is_empty()
    );
}
