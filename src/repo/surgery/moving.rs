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

impl RepoTab {
    /// Reads commit `hash` and opens the editor to move some of its changes elsewhere.
    pub(in crate::repo) fn open_move(&mut self, hash: String, ctx: &egui::Context) {
        if let Some(why) = self.surgery_refusal(&hash) {
            self.status = Some(crate::repo::Notice::plain((
                format!("Move changes: {why}"),
                true,
            )));
            return;
        }
        let line = quick_edit::first_parents(&self.commits, &self.index, &self.refs.head);
        let Some(depth) = line.iter().position(|c| c.hash == hash) else {
            return;
        };
        let targets = targets(&line, depth);
        // Neither a merge nor the first commit ended the line: its history isn't all loaded.
        let truncated = line.iter().all(|c| c.parents.len() == 1);
        self.status = Some(crate::repo::Notice::plain((
            format!("Move changes: reading {}…", short(&hash)),
            false,
        )));
        let path = self.path.clone();
        self.then(
            ctx,
            move || super::split::read(&path, &hash),
            move |tab, result: Result<Read, String>, _| match result {
                Ok(_) if tab.rebase.is_some() || tab.conflict.is_some() => tab.status = None,
                Ok(read) => {
                    tab.status = None;
                    tab.diff = None;
                    tab.file_view = None;
                    let mut editor = SplitEditor::moving(read, targets, depth);
                    if let Mode::Move(m) = &mut editor.mode {
                        m.truncated = truncated;
                    }
                    tab.split = Some(Box::new(editor));
                }
                Err(e) => {
                    tab.status = Some(crate::repo::Notice::plain((
                        format!("Move changes: {e}"),
                        true,
                    )))
                }
            },
        );
    }

    /// Makes the trees of what stays and what moves, and shows both.
    pub(in crate::repo) fn check_move(&mut self, ctx: &egui::Context) {
        let Some(editor) = self.split.as_mut().filter(|e| !e.busy) else {
            return;
        };
        if !matches!(editor.mode, Mode::Move(_)) || editor.problem().is_some() {
            return;
        }
        let stay = patchset::patch_of(&editor.files, |f, u| editor.group_of(f, u) == 0);
        let moved = patchset::patch_of(&editor.files, |f, u| editor.group_of(f, u) == 1);
        let (id, parent) = (editor.id, editor.parent.clone());
        editor.busy = true;
        editor.error = None;
        let path = self.path.clone();
        self.then(
            ctx,
            move || prepare(&path, &parent, &stay, &moved),
            move |tab, result, _| {
                let Some(editor) = tab.split.as_mut().filter(|e| e.id == id) else {
                    return;
                };
                editor.busy = false;
                match result {
                    Ok(checked) => editor.checked = Some(checked),
                    Err(e) => editor.error = Some(e),
                }
            },
        );
    }

    /// Makes the commits of what stays and what moves and rewrites the branch from the older
    /// of the two commits; to the working changes, applies what moved once that is done.
    pub(in crate::repo) fn run_move(&mut self, ctx: &egui::Context) {
        let Some(editor) = self.split.as_mut().filter(|e| !e.busy) else {
            return;
        };
        let (Mode::Move(m), Some(checked)) = (&editor.mode, editor.checked.clone()) else {
            return;
        };
        let target = m.target().cloned();
        // From below the older of the two.
        let onto = match &target {
            Some(t) if t.depth > m.depth => format!("{}^", t.hash),
            _ => editor.parent.clone(),
        };
        let (id, hash, parent, message) = (
            editor.id,
            editor.hash.clone(),
            editor.parent.clone(),
            editor.message.clone(),
        );
        let label = match &target {
            Some(t) => format!("Move changes from {} to {}", short(&hash), short(&t.hash)),
            None => format!("Move changes from {} to the working changes", short(&hash)),
        };
        let (after, undo) = if target.is_some() {
            (Vec::new(), Undo::HeadMove { mode: "--keep" })
        } else {
            (
                // Picked last by the rebase, which went through: out of the commits, staged.
                vec![["reset", "--soft", "HEAD^"].map(str::to_owned).to_vec()],
                // Back with the moved changes staged again in their old commit's place:
                // HEAD's tree before is HEAD's now with them.
                Undo::HeadMove { mode: "--soft" },
            )
        };
        let older = target.as_ref().is_some_and(|t| t.depth > m.depth);
        let onto_for_replay = onto.clone();
        editor.busy = true;
        editor.error = None;
        let path = self.path.clone();
        self.rewrite_with(
            label,
            onto,
            move |entries| {
                let Some(old) = entries.iter().find(|e| e.commit.hash == hash).cloned() else {
                    return Err(
                        crate::i18n::text("repo-workflows-surgery-run-move-97891621").to_owned(),
                    );
                };
                let Some(head) = entries.last().map(|e| e.commit.hash.clone()) else {
                    return Err(
                        crate::i18n::text("repo-workflows-surgery-run-move-145a2a94").to_owned(),
                    );
                };
                let author = git::author_of(&path, &hash)?;
                let [base, stay_tree, moved_tree] = &checked.trees[..] else {
                    return Err(
                        crate::i18n::text("repo-workflows-surgery-run-move-d1994fc1").to_owned(),
                    );
                };
                let whole = git::tree_of(&path, &hash)?;
                let note = format!("fixup! moved from {}", short(&hash));
                let commit = |tree: &str, parent: &str, message: &str| {
                    git::commit_tree(&path, tree, parent, message, Some(&author))
                };
                // Each part made on the other, so that together they are the old commit and
                // neither repeats what the other does (a file both would create…).
                let (stay, moved) = if older {
                    // What moves goes first (into the older commit), what stays on top of it.
                    let moved = commit(moved_tree, &parent, &note)?;
                    let stay = (*moved_tree != whole)
                        .then(|| commit(&whole, &moved, &message))
                        .transpose()?;
                    (stay, moved)
                } else {
                    let stay = (stay_tree != base)
                        .then(|| commit(stay_tree, &parent, &message))
                        .transpose()?;
                    let moved = commit(&whole, stay.as_deref().unwrap_or(&parent), &note)?;
                    (stay, moved)
                };
                let todo = todo(
                    entries,
                    &hash,
                    stay.map(|s| prepared(s, &message, &old.commit)),
                    target.as_ref().map(|t| t.hash.as_str()),
                    prepared(moved, &note, &old.commit),
                )?;
                // Played through first: the branch must end with the files it has now. A commit
                // in between that undoes (or redoes) the moved lines would make them vanish.
                if let Some(end) = super::replay::ends_as(&path, &onto_for_replay, &todo)?
                    && end != git::tree_of(&path, &head)?
                {
                    return Err(
                        crate::i18n::text("repo-workflows-surgery-run-move-9a1d6978").to_owned(),
                    );
                }
                Ok(todo)
            },
            (Vec::new(), after),
            undo,
            move |tab, started| {
                // Cancelled meanwhile: nothing runs.
                let Some(editor) = tab.split.as_mut().filter(|e| e.id == id) else {
                    return false;
                };
                match started {
                    Ok(()) => {
                        tab.split = None;
                        true
                    }
                    Err(e) => {
                        editor.busy = false;
                        editor.error = Some(e);
                        false
                    }
                }
            },
            ctx,
        );
    }
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
            &[("short-editor-hash-1", (short(&editor.hash)).clone().into())],
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
                    ("short-t-hash-1", (short(&t.hash)).clone().into()),
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
mod tests {
    use super::*;
    use crate::git::RebaseCommit;

    fn entry(hash: &str) -> Entry {
        Entry {
            commit: RebaseCommit {
                hash: hash.to_owned(),
                summary: hash.to_owned(),
                ..RebaseCommit::default()
            },
            step: Step::Pick,
            message: hash.to_owned(),
        }
    }

    fn line(entries: &[Entry]) -> Vec<(String, Step)> {
        entries
            .iter()
            .map(|e| (e.commit.hash.clone(), e.step))
            .collect()
    }

    #[test]
    fn the_moved_changes_meld_into_their_commit_older_or_newer() {
        let all = || ["b", "x", "a", "y"].map(entry).to_vec();
        let pick = |h: &str| (h.to_owned(), Step::Pick);
        let fix = |h: &str| (h.to_owned(), Step::Fixup);
        // Into an older one: right after it, the old commit replaced.
        let older = todo(all(), "a", Some(entry("a2")), Some("b"), entry("m"));
        assert_eq!(
            older.map(|t| line(&t)),
            Ok(vec![pick("b"), fix("m"), pick("x"), pick("a2"), pick("y")])
        );
        // Into a newer one, nothing left of the old one.
        let newer = todo(all(), "b", None, Some("y"), entry("m"));
        assert_eq!(
            newer.map(|t| line(&t)),
            Ok(vec![pick("x"), pick("a"), pick("y"), fix("m")])
        );
        // To the working changes: picked last.
        let work = todo(all(), "x", Some(entry("x2")), None, entry("m"));
        assert_eq!(
            work.map(|t| line(&t)),
            Ok(vec![pick("b"), pick("x2"), pick("a"), pick("y"), pick("m")])
        );
        assert!(todo(all(), "gone", None, None, entry("m")).is_err());
        assert!(todo(all(), "a", None, Some("gone"), entry("m")).is_err());
    }

    #[test]
    fn targets_stop_at_a_merge_and_leave_the_commit_out() {
        let commit = |h: &str, parents: usize| Commit {
            hash: h.to_owned(),
            summary: format!("s{h}"),
            parents: (0..parents).map(|p| format!("p{p}")).collect(),
            ..Commit::default()
        };
        let commits = [
            commit("h", 1),
            commit("a", 1),
            commit("b", 1),
            commit("m", 2),
            commit("o", 1),
        ];
        let line: Vec<&Commit> = commits.iter().collect();
        let found: Vec<(String, usize)> = targets(&line, 1)
            .into_iter()
            .map(|t| (t.hash, t.depth))
            .collect();
        assert_eq!(found, [("h".to_owned(), 0), ("b".to_owned(), 2)]);
        let root = [commit("h", 1), commit("r", 0)];
        let line: Vec<&Commit> = root.iter().collect();
        assert!(targets(&line, 0).is_empty());
    }

    #[test]
    fn a_move_needs_something_to_move_and_a_place() {
        let mut m = Move {
            targets: vec![],
            depth: 0,
            dest: None,
            truncated: false,
        };
        assert!(
            m.problem(&[3, 0])
                .is_some_and(|(p, hint)| p.contains("Click") && hint)
        );
        assert!(m.problem(&[3, 1]).is_some_and(|(p, _)| p.contains("where")));
        m.dest = Some(Dest::Work);
        assert_eq!(m.problem(&[0, 1]), None);
    }
}
