//! _Move changes to another commit…_: some files, hunks or lines of a commit taken out of it
//! and put into another commit of the checked-out branch, or back into the working changes
//! (staged). It uses the split editor: commit 1 is what stays, commit 2 what moves.
//!
//! Checking makes two trees on the commit's parent: what stays, and what moves. Moving makes a
//! commit of each, one on the other so that together they are the old commit: into an older
//! commit, what moves first and what stays on it; otherwise the other way round. The rebase
//! picks what stays in place of the old commit (or drops it when nothing stays) and puts what
//! moves right after the destination as a `fixup`, so it melds into it with the destination's
//! message. Moved to the working changes, it is picked last and taken back out of the commits
//! (`reset --soft`) once the rebase is done: stopped on a conflict, it is still a commit there.
//! Before anything runs, the new todo list is played through (`merge-tree`): the branch must
//! end with the files it has now, or the move is refused.

mod operation;

use std::path::Path;

use super::split::{Checked, Mode, Preview, Read, SplitEditor};
use super::{patchset, prepared};
use crate::git::{self, Commit};
use crate::repo::rebase::{Entry, Step};
use crate::repo::{RepoTab, Undo, quick_edit, short};

/// A commit the changes can be moved to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Target {
    pub hash: String,
    pub summary: String,
    /// How far down the branch it is: 0 for HEAD.
    pub depth: usize,
}

/// Where the moved changes go.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Dest {
    /// `targets[0]`.
    Commit(usize),
    /// Staged in the working changes.
    Work,
}

/// A move being set up.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Move {
    /// The other commits of the branch, newest first, down to a merge or the first commit.
    pub targets: Vec<Target>,
    /// How far down the branch the commit moved out of is.
    pub depth: usize,
    pub dest: Option<Dest>,
    /// The branch goes on below the commits loaded: older ones aren't listed yet.
    pub truncated: bool,
}

impl Move {
    /// Why it can't be done as set up, if it can't, and whether that is just a step to take.
    pub fn problem(&self, counts: &[usize]) -> Option<(String, bool)> {
        if counts.get(1).copied().unwrap_or(0) == 0 {
            return Some((
                crate::i18n::text("repo-workflows-surgery-problem-c7982ff3").to_owned(),
                true,
            ));
        }
        if self.dest.is_none() {
            return Some((
                crate::i18n::text("repo-workflows-surgery-problem-5cf54a16").to_owned(),
                true,
            ));
        }
        None
    }

    /// The commit picked, if a commit was.
    pub fn target(&self) -> Option<&Target> {
        match self.dest {
            Some(Dest::Commit(i)) => self.targets.get(i),
            _ => None,
        }
    }
}

/// The commits of the branch the changes of the commit at `depth` (in `line`, HEAD first)
/// can go to: every other one down to the first merge or the first commit, which can't be
/// rewritten here.
pub(crate) fn targets(line: &[&Commit], depth: usize) -> Vec<Target> {
    line.iter()
        .enumerate()
        .take_while(|(_, c)| c.parents.len() == 1)
        .filter(|(d, _)| *d != depth)
        .map(|(d, c)| Target {
            hash: c.hash.clone(),
            summary: c.summary.clone(),
            depth: d,
        })
        .collect()
}

/// The trees of a move of some changes out of `hash` (on `parent`): the parent's, what stays
/// (`stay`, a patch of the parent) and what moves (`moved`). The moved patch is kept where git
/// runs for the working changes.
pub(crate) fn prepare(
    repo: &Path,
    parent: &str,
    stay: &str,
    moved: &str,
) -> Result<Checked, String> {
    let base = git::tree_of(repo, parent)?;
    let stay_tree = if stay.is_empty() {
        base.clone()
    } else {
        let file = git::save_patch_for(repo, stay)?;
        git::tree_after(repo, parent, &[file]).map_err(|e| {
            crate::i18n::message(
                "repo-workflows-surgery-prepare-d129ae6d",
                &[("e", (e).clone().into())],
            )
        })?
    };
    let moved_file = git::save_patch_for(repo, moved)?;
    let moved_tree = git::tree_after(repo, parent, &[moved_file]).map_err(|e| {
        crate::i18n::message(
            "repo-workflows-surgery-prepare-17c56a59",
            &[("e", (e).clone().into())],
        )
    })?;
    let preview = Preview::new(&[
        (
            git::patch_between(repo, &base, &stay_tree)?,
            git::paths_between(repo, &base, &stay_tree)?,
        ),
        (
            git::patch_between(repo, &base, &moved_tree)?,
            git::paths_between(repo, &base, &moved_tree)?,
        ),
    ]);
    Ok(Checked {
        trees: vec![base, stay_tree, moved_tree],
        preview,
    })
}

/// The todo list of a move out of `from` (`stay`: what stays of it, `None` when nothing does)
/// into `to` (`moved`, melded into it), or, when `to` is `None`, `moved` picked last, to be
/// taken out of the commits into the working changes once the rebase is done.
fn todo(
    entries: Vec<Entry>,
    from: &str,
    stay: Option<Entry>,
    to: Option<&str>,
    moved: Entry,
) -> Result<Vec<Entry>, String> {
    if !entries.iter().any(|e| e.commit.hash == from) {
        return Err(crate::i18n::text("repo-workflows-surgery-todo-97891621").to_owned());
    }
    if let Some(to) = to
        && !entries.iter().any(|e| e.commit.hash == to)
    {
        return Err(crate::i18n::text("repo-workflows-surgery-todo-41cdda9e").to_owned());
    }
    let mut out = Vec::new();
    let mut moved = Some(moved);
    for e in entries {
        if e.commit.hash == from {
            out.extend(stay.clone());
            continue;
        }
        let here = to == Some(e.commit.hash.as_str());
        out.push(e);
        if here && let Some(mut m) = moved.take() {
            m.step = Step::Fixup;
            out.push(m);
        }
    }
    // To the working changes: picked last, then taken out of the commits (`reset --soft`).
    out.extend(moved);
    Ok(out)
}

impl SplitEditor {
    /// The editor for moving some changes out of the commit `read`, at `depth` down the
    /// branch, to one of `targets` or the working changes: everything stays at first.
    pub fn moving(read: Read, targets: Vec<Target>, depth: usize) -> Self {
        let mut editor = SplitEditor::new(read);
        for slot in editor.owner.iter_mut().flatten() {
            *slot = 0;
        }
        editor.groups[0].subject = crate::i18n::message(
            "repo-workflows-surgery-moving-4c6a80bb",
            &[("short-editor-hash-1", (short(&editor.hash)).into())],
        );
        editor.groups[0].body.clear();
        editor.groups[1].subject =
            crate::i18n::text("repo-workflows-surgery-moving-26c84ac3").to_owned();
        editor.active = 1;
        editor.mode = Mode::Move(Move {
            targets,
            depth,
            dest: None,
            truncated: false,
        });
        editor.recount();
        editor
    }

    /// Sends what moves to `dest`.
    pub fn set_dest(&mut self, dest: Dest) {
        if self.busy {
            return;
        }
        let Mode::Move(m) = &mut self.mode else {
            return;
        };
        m.dest = Some(dest);
        self.groups[1].subject = match m.target() {
            Some(t) => crate::i18n::message(
                "repo-workflows-surgery-set-dest-556c2e6e",
                &[
                    ("short-t-hash-1", (short(&t.hash)).into()),
                    ("t-summary-2", (t.summary).clone().into()),
                ],
            ),
            None => crate::i18n::text("repo-workflows-surgery-set-dest-0c40c6af").to_owned(),
        };
        self.checked = None;
        self.error = None;
    }
}

#[cfg(test)]
mod tests;
