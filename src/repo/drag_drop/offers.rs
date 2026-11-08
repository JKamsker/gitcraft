//! What a drop offers: for what is dragged and where it is dropped, the operations that make
//! sense there, each the action of the menu entry of the same name.

use std::collections::HashMap;
use std::sync::Arc;

use super::reorder::{CommitDrop, Place};
use crate::git::Commit;
use crate::repo::short;

/// A branch, as dragged or dropped on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DragBranch {
    /// `main`, or `origin/main` for a remote-tracking branch.
    pub name: String,
    pub remote: bool,
    /// The commit it points at.
    pub tip: String,
}

/// A commit, as dragged or dropped on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DragCommit {
    pub hash: String,
    pub summary: String,
    pub merge: bool,
}

/// What is being dragged.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum DragItem {
    Branch(DragBranch),
    Commit(DragCommit),
}

impl DragItem {
    /// What the drag ghost and the drop menu call it.
    pub(crate) fn label(&self) -> String {
        match self {
            DragItem::Branch(b) => b.name.clone(),
            DragItem::Commit(c) => format!("{} {}", short(&c.hash), c.summary),
        }
    }
}

/// Where something is dropped.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum DropTarget {
    /// A branch's pill or its row in the sidebar.
    Branch(DragBranch),
    /// The pill of a detached HEAD.
    DetachedHead,
    /// A commit's row, away from its pill.
    Commit(DragCommit),
}

/// Whether one commit is an ancestor of another (or the same one), as far as known.
pub(crate) trait Ancestry {
    /// `None` when it can't be told (one of them isn't loaded).
    fn is_ancestor(&self, ancestor: &str, of: &str) -> Option<bool>;
}

/// The checked-out branch from HEAD down its first parents, newest first, as far as loaded.
#[derive(Default)]
pub(crate) struct Line {
    commits: Vec<Arc<Commit>>,
    /// Where each commit is on it.
    pos: HashMap<String, usize>,
    /// The newest merge on it, if any.
    first_merge: Option<usize>,
}

impl Line {
    pub(crate) fn new(commits: Vec<Arc<Commit>>) -> Self {
        let pos = commits
            .iter()
            .enumerate()
            .map(|(i, c)| (c.hash.clone(), i))
            .collect();
        let first_merge = commits.iter().position(|c| c.parents.len() > 1);
        Self {
            commits,
            pos,
            first_merge,
        }
    }
}

/// What the offers depend on besides the dragged item and the target.
pub(crate) struct Scene<'a> {
    /// The checked-out branch, `None` when HEAD is detached.
    pub current: Option<&'a str>,
    /// HEAD's commit.
    pub head: &'a str,
    /// The checked-out branch's own commits.
    pub line: &'a Line,
    /// Local branches checked out in another worktree, which git won't move from here.
    pub elsewhere: &'a [String],
    pub ancestry: &'a dyn Ancestry,
}

/// A branch or a commit an operation brings in or goes onto.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Rev {
    Branch { name: String, remote: bool },
    Commit { hash: String },
}

impl Rev {
    fn of_branch(b: &DragBranch) -> Self {
        Rev::Branch {
            name: b.name.clone(),
            remote: b.remote,
        }
    }

    /// The name people see: the branch, or the short hash.
    pub(crate) fn label(&self) -> String {
        match self {
            Rev::Branch { name, .. } => name.clone(),
            Rev::Commit { hash } => short(hash).to_owned(),
        }
    }
}

/// One entry of the drop menu.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Offer {
    /// Merge `from` into the checked-out branch `into`.
    Merge { from: Rev, into: String },
    /// Rebase the checked-out `branch` onto `onto`.
    Rebase { branch: String, onto: Rev },
    /// Open the interactive rebase editor for the checked-out `branch` onto `onto`.
    InteractiveRebase { branch: String, onto: Rev },
    /// Move `branch` forward to `to` (at `to_tip`), from `from_tip`: the checked-out branch
    /// with a fast-forward merge, another without checking it out.
    FastForward {
        branch: String,
        to: Rev,
        to_tip: String,
        from_tip: String,
        checked_out: bool,
    },
    /// Cherry-pick `hash` onto what is checked out (`onto`: the branch, or `HEAD`).
    CherryPick { hash: String, onto: String },
    /// Move or squash a commit of the checked-out branch.
    Rewrite(CommitDrop),
}

impl Offer {
    /// The menu entry.
    pub(crate) fn label(&self) -> String {
        match self {
            Offer::Merge { from, into } => crate::i18n::message(
                "repo-core-2422",
                &[
                    ("hash", from.label().to_string().into()),
                    ("into", into.to_string().into()),
                ],
            ),
            Offer::Rebase { branch, onto } => crate::i18n::message(
                "repo-core-2423",
                &[
                    ("branch", branch.to_string().into()),
                    ("hash", onto.label().to_string().into()),
                ],
            ),
            Offer::InteractiveRebase { branch, onto } => crate::i18n::message(
                "repo-core-2424",
                &[
                    ("branch", branch.to_string().into()),
                    ("label", onto.label().to_string().into()),
                ],
            ),
            Offer::FastForward { branch, to, .. } => crate::i18n::message(
                "repo-core-2425",
                &[
                    ("branch", branch.to_string().into()),
                    ("label", to.label().to_string().into()),
                ],
            ),
            Offer::CherryPick { hash, onto } => crate::i18n::message(
                "repo-core-2426",
                &[
                    ("hash", short(hash).to_string().into()),
                    ("onto", onto.to_string().into()),
                ],
            ),
            Offer::Rewrite(drop) => drop.label(),
        }
    }

    /// What the entry does, naming the branch it changes, for its tooltip.
    pub(crate) fn explain(&self) -> String {
        match self {
            Offer::Merge { from, into } => crate::i18n::message(
                "repo-core-2427",
                &[
                    ("into", into.to_string().into()),
                    ("label", from.label().to_string().into()),
                ],
            ),
            Offer::Rebase { branch, onto } => crate::i18n::message(
                "repo-core-2428",
                &[
                    ("branch", branch.to_string().into()),
                    ("label", onto.label().to_string().into()),
                ],
            ),
            Offer::InteractiveRebase { branch, .. } => {
                crate::i18n::message("repo-core-2429", &[("branch", branch.to_string().into())])
            }
            Offer::FastForward {
                branch,
                to,
                checked_out: false,
                ..
            } => crate::i18n::message(
                "repo-core-2430",
                &[
                    ("branch", branch.to_string().into()),
                    ("label", to.label().to_string().into()),
                ],
            ),
            Offer::FastForward { branch, to, .. } => crate::i18n::message(
                "repo-core-2431",
                &[
                    ("branch", branch.to_string().into()),
                    ("label", to.label().to_string().into()),
                ],
            ),
            Offer::CherryPick { onto, .. } => {
                crate::i18n::message("repo-core-2432", &[("onto", onto.to_string().into())])
            }
            Offer::Rewrite(drop) => drop.place.explain().to_owned(),
        }
    }
}

/// What dropping `item` on `target` offers; empty where nothing can be done (the target
/// then doesn't light up).
pub(crate) fn offers(item: &DragItem, target: &DropTarget, scene: &Scene<'_>) -> Vec<Offer> {
    match item {
        DragItem::Branch(branch) => branch_offers(branch, target, scene),
        DragItem::Commit(commit) => commit_offers(commit, target, scene),
    }
}

/// Whether `target` stands for what is checked out: its branch's pill or row, the detached
/// HEAD's pill, or HEAD's commit.
fn is_checked_out(target: &DropTarget, scene: &Scene<'_>) -> bool {
    match target {
        DropTarget::Branch(b) => !b.remote && scene.current == Some(b.name.as_str()),
        DropTarget::DetachedHead => scene.current.is_none(),
        DropTarget::Commit(c) => c.hash == scene.head,
    }
}

/// A branch dropped on a branch or a commit: what changes is the checked-out one (or, with a
/// fast-forward, a local branch that isn't).
fn branch_offers(src: &DragBranch, target: &DropTarget, scene: &Scene<'_>) -> Vec<Offer> {
    let (target_rev, target_tip) = match target {
        DropTarget::Branch(b) => (Rev::of_branch(b), b.tip.as_str()),
        DropTarget::Commit(c) => (
            Rev::Commit {
                hash: c.hash.clone(),
            },
            c.hash.as_str(),
        ),
        DropTarget::DetachedHead => return Vec::new(),
    };
    // The same branch, or another at the same commit: nothing to bring in.
    if src.tip == target_tip {
        return Vec::new();
    }
    let Some(current) = scene.current else {
        // Detached: only branches that aren't checked out can be moved.
        return fast_forward_other(target, Rev::of_branch(src), &src.tip, scene)
            .into_iter()
            .collect();
    };
    let src_is_current = !src.remote && src.name == current;
    if is_checked_out(target, scene) {
        return onto_current(current, scene, Rev::of_branch(src), &src.tip);
    }
    if src_is_current {
        let mut offers = onto_current(current, scene, target_rev, target_tip);
        offers.extend(fast_forward_other(
            target,
            Rev::of_branch(src),
            &src.tip,
            scene,
        ));
        return offers;
    }
    fast_forward_other(target, Rev::of_branch(src), &src.tip, scene)
        .into_iter()
        .collect()
}

/// What `other` (at `other_tip`) offers the checked-out branch `current`: a fast-forward when
/// it is only behind, an interactive rebase when it already has `other`, else merge and rebase.
fn onto_current(current: &str, scene: &Scene<'_>, other: Rev, other_tip: &str) -> Vec<Offer> {
    let behind = scene.ancestry.is_ancestor(scene.head, other_tip);
    let contains = scene.ancestry.is_ancestor(other_tip, scene.head);
    let interactive = Offer::InteractiveRebase {
        branch: current.to_owned(),
        onto: other.clone(),
    };
    if behind == Some(true) {
        return vec![Offer::FastForward {
            branch: current.to_owned(),
            to: other,
            to_tip: other_tip.to_owned(),
            from_tip: scene.head.to_owned(),
            checked_out: true,
        }];
    }
    // Merging or rebasing would do nothing; its commits after `other` can still be edited.
    if contains == Some(true) {
        return vec![interactive];
    }
    vec![
        Offer::Merge {
            from: other.clone(),
            into: current.to_owned(),
        },
        Offer::Rebase {
            branch: current.to_owned(),
            onto: other,
        },
        interactive,
    ]
}

/// Moving the local branch `target` (not checked out, here or in another worktree) forward to
/// `to`, when it is known to be behind it.
fn fast_forward_other(
    target: &DropTarget,
    to: Rev,
    to_tip: &str,
    scene: &Scene<'_>,
) -> Option<Offer> {
    let DropTarget::Branch(b) = target else {
        return None;
    };
    let checked_out = scene.current == Some(b.name.as_str());
    if b.remote || checked_out || scene.elsewhere.contains(&b.name) || b.tip == to_tip {
        return None;
    }
    (scene.ancestry.is_ancestor(&b.tip, to_tip) == Some(true)).then(|| Offer::FastForward {
        branch: b.name.clone(),
        to,
        to_tip: to_tip.to_owned(),
        from_tip: b.tip.clone(),
        checked_out: false,
    })
}

/// A commit dropped on what is checked out is cherry-picked onto it; one of the checked-out
/// branch dropped on another of it is moved next to it or squashed into it.
fn commit_offers(src: &DragCommit, target: &DropTarget, scene: &Scene<'_>) -> Vec<Offer> {
    let mut offers = Vec::new();
    let onto_head = is_checked_out(target, scene);
    let in_head = scene.ancestry.is_ancestor(&src.hash, scene.head);
    if onto_head && !src.merge && src.hash != scene.head && in_head != Some(true) {
        offers.push(Offer::CherryPick {
            hash: src.hash.clone(),
            onto: scene.current.unwrap_or("HEAD").to_owned(),
        });
    }
    if scene.current.is_none() {
        return offers;
    }
    let target_hash = match target {
        DropTarget::Commit(c) => Some(c.hash.as_str()),
        DropTarget::Branch(_) if onto_head => Some(scene.head),
        _ => None,
    };
    if let Some(target_hash) = target_hash {
        offers.extend(line_offers(&src.hash, target_hash, scene.line));
    }
    offers
}

/// Moving `hash` next to `target` on the checked-out branch's `line` (newest first), or
/// squashing it into it: both on the line, with no merge in what the rebase replays, and a
/// parent below the older of the two to replay onto.
fn line_offers(hash: &str, target: &str, line: &Line) -> Vec<Offer> {
    let (Some(&from), Some(&to)) = (line.pos.get(hash), line.pos.get(target)) else {
        return Vec::new();
    };
    if from == to {
        return Vec::new();
    }
    let oldest = from.max(to);
    if line.first_merge.is_some_and(|m| m <= oldest) {
        return Vec::new();
    }
    let (Some(oldest), Some(target_commit)) = (line.commits.get(oldest), line.commits.get(to))
    else {
        return Vec::new();
    };
    let Some(base) = oldest.parents.first() else {
        return Vec::new();
    };
    let drop = |place: Place| {
        Offer::Rewrite(CommitDrop {
            place,
            hash: hash.to_owned(),
            target: target.to_owned(),
            target_summary: target_commit.summary.clone(),
            base: base.clone(),
        })
    };
    let mut offers = Vec::new();
    // Rows go newest first: right above the target is one row up from it.
    if from + 1 != to {
        offers.push(drop(Place::Above));
    }
    if from != to + 1 {
        offers.push(drop(Place::Below));
    }
    offers.push(drop(Place::Squash));
    offers.push(drop(Place::Fixup));
    offers
}
