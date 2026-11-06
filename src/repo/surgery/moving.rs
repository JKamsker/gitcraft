//! _Move changes to another commit…_: some files, hunks or lines of a commit taken out of it
//! and put into another commit of the checked-out branch, or back into the working changes
//! (staged). It uses the split editor: commit 1 is what stays, commit 2 what moves.
//!
//! Checking makes two trees on the commit's parent: what stays, and what moves. Moving makes a
//! commit of each; the rebase picks the first in place of the old commit (or drops it when
//! nothing stays) and puts the second right after the destination as a `fixup`, so it melds
//! into it with the destination's message. Moved to the working changes, the second is applied
//! (`apply --3way`) once the rebase is done; a conflict there is the usual conflicted file.

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
}

impl Move {
    /// Why it can't be done as set up, if it can't, and whether that is just a step to take.
    pub fn problem(&self, counts: &[usize]) -> Option<(String, bool)> {
        if counts.get(1).copied().unwrap_or(0) == 0 {
            return Some((
                "Click the files, hunks or lines to move: they get the chip 2".to_owned(),
                true,
            ));
        }
        if self.dest.is_none() {
            return Some(("Pick on the left where they go".to_owned(), true));
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
    leaving: &str,
    arriving: &str,
) -> Result<Checked, String> {
    let base = git::tree_of(repo, parent)?;
    let stay_tree = if stay.is_empty() {
        base.clone()
    } else {
        let file = git::save_patch_for(repo, stay)?;
        git::tree_after(repo, parent, &[file]).map_err(|e| format!("what stays: {e}"))?
    };
    let moved_file = git::save_patch_for(repo, moved)?;
    let moved_tree = git::tree_after(repo, parent, std::slice::from_ref(&moved_file))
        .map_err(|e| format!("what moves: {e}"))?;
    let leaving_patch = git::patch_between(repo, &base, &stay_tree)?;
    let mut leaving_files = git::paths_between(repo, &base, &stay_tree)?;
    if leaving_files.is_empty() {
        leaving_files.push(format!("nothing: {leaving} is dropped"));
    }
    let arriving_files = git::paths_between(repo, &base, &moved_tree)?;
    let preview = Preview::new(&[
        (leaving_patch, leaving_files),
        (
            git::patch_between(repo, &base, &moved_tree)?,
            arriving_files
                .iter()
                .map(|f| format!("{f} → {arriving}"))
                .collect(),
        ),
    ]);
    Ok(Checked {
        trees: vec![base, stay_tree, moved_tree],
        preview,
        moved: Some(moved_file),
    })
}

/// Why the moved changes can't land in the working changes: one of their files has changes
/// there already.
fn work_refusal(repo: &Path, paths: &[String]) -> Result<(), String> {
    let mut args = vec![
        "--literal-pathspecs",
        "status",
        "--porcelain=v1",
        "-z",
        "--untracked-files=all",
        "--",
    ];
    args.extend(paths.iter().map(String::as_str));
    let out = git::run(Some(repo), &args)?;
    match out.split('\0').find(|e| e.len() > 3) {
        Some(entry) => Err(format!(
            "{} has uncommitted changes, where the moved ones would land: commit or stash them first",
            &entry[3..]
        )),
        None => Ok(()),
    }
}

/// The todo list of a move out of `from` (`stay`: what stays of it, `None` when nothing does)
/// into `to` (`moved`, melded into it), or out of the branch when `to` is `None`.
fn todo(
    entries: Vec<Entry>,
    from: &str,
    stay: Option<Entry>,
    to: Option<&str>,
    moved: Entry,
) -> Result<Vec<Entry>, String> {
    if !entries.iter().any(|e| e.commit.hash == from) {
        return Err("the commit is no longer on the checked-out branch".to_owned());
    }
    if let Some(to) = to
        && !entries.iter().any(|e| e.commit.hash == to)
    {
        return Err("the commit to move them to is no longer on the branch".to_owned());
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
    Ok(out)
}

impl RepoTab {
    /// Reads commit `hash` and opens the editor to move some of its changes elsewhere.
    pub(in crate::repo) fn open_move(&mut self, hash: String, ctx: &egui::Context) {
        if let Some(why) = self.surgery_refusal(&hash) {
            self.status = Some((format!("Move changes: {why}"), true));
            return;
        }
        let line = quick_edit::first_parents(&self.commits, &self.index, &self.refs.head);
        let Some(depth) = line.iter().position(|c| c.hash == hash) else {
            return;
        };
        let targets = targets(&line, depth);
        self.status = Some((format!("Move changes: reading {}…", short(&hash)), false));
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
                    tab.split = Some(Box::new(SplitEditor::moving(read, targets, depth)));
                }
                Err(e) => tab.status = Some((format!("Move changes: {e}"), true)),
            },
        );
    }

    /// Makes the trees of what stays and what moves, and shows both.
    pub(in crate::repo) fn check_move(&mut self, ctx: &egui::Context) {
        let Some(editor) = self.split.as_mut().filter(|e| !e.busy) else {
            return;
        };
        let Mode::Move(m) = &editor.mode else {
            return;
        };
        if editor.problem().is_some() {
            return;
        }
        let arriving = match m.target() {
            Some(t) => short(&t.hash).to_owned(),
            None => "working changes".to_owned(),
        };
        let stay = patchset::patch_of(&editor.files, |f, u| editor.group_of(f, u) == 0);
        let moved = patchset::patch_of(&editor.files, |f, u| editor.group_of(f, u) == 1);
        let (id, parent, leaving) = (
            editor.id,
            editor.parent.clone(),
            short(&editor.hash).to_owned(),
        );
        editor.busy = true;
        editor.error = None;
        let path = self.path.clone();
        self.then(
            ctx,
            move || prepare(&path, &parent, &stay, &moved, &leaving, &arriving),
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
        let Some(moved_file) = checked.moved.clone() else {
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
        let moved_paths: Vec<String> = editor
            .files
            .iter()
            .enumerate()
            .filter(|(f, _)| {
                editor
                    .units
                    .get(*f)
                    .is_some_and(|us| us.iter().any(|&u| editor.group_of(*f, u) == 1))
            })
            .map(|(_, file)| file.path.clone())
            .collect();
        let label = match &target {
            Some(t) => format!("Move changes from {} to {}", short(&hash), short(&t.hash)),
            None => format!("Move changes from {} to the working changes", short(&hash)),
        };
        let (after, undo) = if target.is_some() {
            (Vec::new(), Undo::HeadMove { mode: "--keep" })
        } else {
            (
                vec![
                    ["apply", "--3way", "--whitespace=nowarn", &moved_file]
                        .map(str::to_owned)
                        .to_vec(),
                ],
                // Back with the moved changes staged again in their old commit's place:
                // HEAD's tree before is HEAD's now with them.
                Undo::HeadMove { mode: "--soft" },
            )
        };
        editor.busy = true;
        editor.error = None;
        let path = self.path.clone();
        self.rewrite_with(
            label,
            onto,
            move |entries| {
                let Some(old) = entries.iter().find(|e| e.commit.hash == hash).cloned() else {
                    return Err("the commit is no longer on the checked-out branch".to_owned());
                };
                if target.is_none() {
                    work_refusal(&path, &moved_paths)?;
                }
                let author = git::author_of(&path, &hash)?;
                let [_, stay_tree, moved_tree] = &checked.trees[..] else {
                    return Err("the move was not checked".to_owned());
                };
                let base = git::tree_of(&path, &parent)?;
                let stay = if *stay_tree == base {
                    None
                } else {
                    let made =
                        git::commit_tree(&path, stay_tree, &parent, &message, Some(&author))?;
                    Some(prepared(made, &message, &old.commit))
                };
                let note = format!("fixup! moved from {}", short(&hash));
                let made = git::commit_tree(&path, moved_tree, &parent, &note, Some(&author))?;
                let moved = prepared(made, &note, &old.commit);
                todo(
                    entries,
                    &hash,
                    stay,
                    target.as_ref().map(|t| t.hash.as_str()),
                    moved,
                )
            },
            after,
            undo,
            move |tab, started| {
                let Some(editor) = tab.split.as_mut().filter(|e| e.id == id) else {
                    return;
                };
                match started {
                    Ok(()) => tab.split = None,
                    Err(e) => {
                        editor.busy = false;
                        editor.error = Some(e);
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
        editor.groups[0].subject = format!("Stays in {}", short(&editor.hash));
        editor.groups[0].body.clear();
        editor.groups[1].subject = "Moves".to_owned();
        editor.active = 1;
        editor.mode = Mode::Move(Move {
            targets,
            depth,
            dest: None,
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
            Some(t) => format!("Moves to {} {}", short(&t.hash), t.summary),
            None => "Moves to the working changes (staged)".to_owned(),
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
        // To the working changes: out of the todo list.
        let work = todo(all(), "x", Some(entry("x2")), None, entry("m"));
        assert_eq!(
            work.map(|t| line(&t)),
            Ok(vec![pick("b"), pick("x2"), pick("a"), pick("y")])
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
