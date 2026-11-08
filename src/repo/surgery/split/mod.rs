//! _Split commit…_: a commit's files, hunks and lines handed out to two or more new commits,
//! each with its own message. Checking makes the new commits' trees (each patch applied in a
//! scratch index) and shows what each one changes; splitting makes the commits, with the
//! messages as they are then, puts them in place of the old commit and replays the commits
//! after it.

mod preview;
mod view;

use std::collections::HashSet;
use std::path::Path;

use super::patchset::{self, FilePatch};
use super::prepared;
use crate::git;
use crate::repo::diff::Kind;
use crate::repo::{RepoTab, Undo, short};

pub(crate) use preview::{Look, Preview};

use super::moving::Move;

/// Most commits a commit is split into.
pub(crate) const MAX_GROUPS: usize = 9;

/// One of the new commits.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Group {
    pub subject: String,
    pub body: String,
}

impl Group {
    fn message(&self) -> String {
        let (subject, body) = (self.subject.trim(), self.body.trim_matches('\n').trim_end());
        if body.is_empty() {
            format!("{subject}\n")
        } else {
            format!("{subject}\n\n{body}\n")
        }
    }
}

/// The new commits' trees, made and checked, with what each one changes.
#[derive(Clone, Debug)]
pub(crate) struct Checked {
    /// The parent's tree, then each new commit's.
    pub trees: Vec<String>,
    pub preview: Preview,
}

/// The commit being split, read in the background.
pub(crate) struct Read {
    pub hash: String,
    pub parent: String,
    pub message: String,
    pub files: Vec<FilePatch>,
}

/// Reads commit `hash` for splitting: its parent, message and files.
pub(crate) fn read(repo: &Path, hash: &str) -> Result<Read, String> {
    let parents = git::run(
        Some(repo),
        &["rev-list", "--parents", "-n", "1", "--end-of-options", hash],
    )?;
    let parent = match parents.split_whitespace().collect::<Vec<_>>()[..] {
        [_] => return Err(crate::i18n::text("repo-workflows-surgery-read-c35ba44e").to_owned()),
        [_, parent] => parent.to_owned(),
        _ => return Err(crate::i18n::text("repo-workflows-surgery-read-6a75c5d8").to_owned()),
    };
    let message = git::run(
        Some(repo),
        &[
            "log",
            "-1",
            "--no-show-signature",
            "--encoding=UTF-8",
            "--format=%B",
            "--end-of-options",
            hash,
        ],
    )?;
    let patch = git::patch_bytes_between(repo, &parent, hash)?;
    let paths = git::paths_between(repo, &parent, hash)?;
    let files = patchset::files_of(&patch, paths)?;
    Ok(Read {
        hash: hash.to_owned(),
        parent,
        message: message.trim_end().to_owned(),
        files,
    })
}

/// A row of the list of the old commit's changes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Row {
    File(usize),
    /// Line `1` of file `0`.
    Line(usize, usize),
}

pub(crate) struct SplitEditor {
    /// Which editor this is: what was checked for one closed since never lands in another.
    pub id: u64,
    pub hash: String,
    pub parent: String,
    pub summary: String,
    pub files: Vec<FilePatch>,
    /// Each file's units (see [`FilePatch::units`]), worked out once.
    pub units: Vec<Vec<usize>>,
    /// For each file, the commit (an index into `groups`) each of its units goes to: by line
    /// index for a file picked line by line, at `0` for one that goes whole.
    pub owner: Vec<Vec<usize>>,
    pub groups: Vec<Group>,
    /// How many units each commit has, kept up to date.
    pub counts: Vec<usize>,
    /// The commit a click hands changes to.
    pub active: usize,
    /// Files whose lines are folded away.
    pub folded: HashSet<usize>,
    /// The rows of the list, kept up to date with `folded`.
    pub rows: Vec<Row>,
    /// The new commits' trees once checked; a change of what goes where drops them.
    pub checked: Option<Checked>,
    /// Being checked or started, in the background.
    pub busy: bool,
    /// What went wrong checking or starting it.
    pub error: Option<String>,
    /// The old commit's whole message.
    pub message: String,
    /// Splitting the commit, or moving some of its changes elsewhere.
    pub mode: Mode,
}

/// What the editor does with the commit.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Mode {
    Split,
    /// Commit `0` is what stays, commit `1` what moves.
    Move(Move),
}

impl SplitEditor {
    /// Every change in the last of two commits, the first one active: what is clicked goes
    /// into it. The first keeps the old message, the second starts empty.
    pub fn new(read: Read) -> Self {
        static IDS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        let units: Vec<Vec<usize>> = read.files.iter().map(FilePatch::units).collect();
        let owner = read
            .files
            .iter()
            .map(|f| vec![1; f.lines.len().max(1)])
            .collect();
        let (subject, body) = read
            .message
            .split_once("\n\n")
            .unwrap_or((&read.message, ""));
        // A first paragraph over several lines is one summary.
        let subject = subject.split_whitespace().collect::<Vec<_>>().join(" ");
        let mut editor = Self {
            id: IDS.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            hash: read.hash,
            parent: read.parent,
            summary: subject.clone(),
            files: read.files,
            units,
            owner,
            groups: vec![
                Group {
                    subject,
                    body: body.trim_matches('\n').trim_end().to_owned(),
                },
                Group::default(),
            ],
            counts: Vec::new(),
            active: 0,
            folded: HashSet::new(),
            rows: Vec::new(),
            checked: None,
            busy: false,
            error: None,
            message: read.message,
            mode: Mode::Split,
        };
        editor.recount();
        editor.relist();
        editor
    }

    /// The commit unit `unit` of file `f` goes to.
    pub fn group_of(&self, f: usize, unit: usize) -> usize {
        self.owner
            .get(f)
            .and_then(|o| o.get(unit))
            .copied()
            .unwrap_or(0)
    }

    pub(crate) fn recount(&mut self) {
        let mut counts = vec![0; self.groups.len()];
        for (f, units) in self.units.iter().enumerate() {
            for &unit in units {
                if let Some(c) = counts.get_mut(self.group_of(f, unit)) {
                    *c += 1;
                }
            }
        }
        self.counts = counts;
    }

    fn relist(&mut self) {
        let mut rows = Vec::new();
        for (f, file) in self.files.iter().enumerate() {
            rows.push(Row::File(f));
            if !self.folded.contains(&f) {
                rows.extend((0..file.lines.len()).map(|i| Row::Line(f, i)));
            }
        }
        self.rows = rows;
    }

    /// Folds file `f`'s lines away, or shows them again.
    pub fn toggle_fold(&mut self, f: usize) {
        if !self.folded.remove(&f) {
            self.folded.insert(f);
        }
        self.relist();
    }

    fn changed(&mut self) {
        self.checked = None;
        self.error = None;
        self.recount();
    }

    /// Hands the units of file `f` that `which` keeps to commit `g`.
    fn assign(&mut self, f: usize, g: usize, which: impl Fn(usize) -> bool) {
        if self.busy || g >= self.groups.len() {
            return;
        }
        let units = self.units.get(f).cloned().unwrap_or_default();
        if let Some(owner) = self.owner.get_mut(f) {
            for unit in units.into_iter().filter(|&u| which(u)) {
                if let Some(slot) = owner.get_mut(unit) {
                    *slot = g;
                }
            }
        }
        self.changed();
    }

    pub fn assign_file(&mut self, f: usize, g: usize) {
        self.assign(f, g, |_| true);
    }

    /// The lines of the hunk whose header is line `h` of file `f`.
    pub fn hunk_range(&self, f: usize, h: usize) -> std::ops::Range<usize> {
        let end = self.files.get(f).map_or(h, |file| {
            file.lines
                .get(h + 1..)
                .and_then(|rest| rest.iter().position(|l| l.kind == Kind::Hunk))
                .map_or(file.lines.len(), |n| h + 1 + n)
        });
        h + 1..end
    }

    pub fn assign_hunk(&mut self, f: usize, h: usize, g: usize) {
        let range = self.hunk_range(f, h);
        self.assign(f, g, |u| range.contains(&u));
    }

    pub fn assign_line(&mut self, f: usize, i: usize, g: usize) {
        self.assign(f, g, |u| u == i);
    }

    pub fn add_group(&mut self) {
        if self.groups.len() < MAX_GROUPS && !self.busy {
            self.groups.push(Group::default());
            self.active = self.groups.len() - 1;
            self.changed();
        }
    }

    /// Where the changes of commit `g` go when it is taken out: the one before it, or for the
    /// first the one after it (which becomes the first).
    pub fn heir(g: usize) -> usize {
        g.saturating_sub(1)
    }

    /// Takes commit `g` out; its changes go to [`Self::heir`].
    pub fn remove_group(&mut self, g: usize) {
        if self.groups.len() <= 2 || g >= self.groups.len() || self.busy {
            return;
        }
        let into = Self::heir(g);
        self.groups.remove(g);
        for slot in self.owner.iter_mut().flatten() {
            if *slot == g {
                *slot = into;
            } else if *slot > g {
                *slot -= 1;
            }
        }
        if self.active >= self.groups.len() || self.active > g {
            self.active = self.active.saturating_sub(1);
        }
        self.changed();
    }

    /// Why it can't be split as set up, if it can't, and whether that is just how it starts
    /// (said as a hint, not an error).
    pub fn problem(&self) -> Option<(String, bool)> {
        if let Mode::Move(m) = &self.mode {
            return m.problem(&self.counts);
        }
        let units: usize = self.units.iter().map(Vec::len).sum();
        if units < 2 {
            return Some((
                crate::i18n::text("repo-workflows-surgery-problem-45b607f4").to_owned(),
                false,
            ));
        }
        if let Some(g) = self.counts.iter().position(|&c| c == 0) {
            let start = self.counts.iter().filter(|&&c| c > 0).count() == 1;
            return Some((
                crate::i18n::message(
                    "repo-workflows-surgery-problem-552d4c58",
                    &[("g-1-1", (g + 1).clone().into())],
                ),
                start,
            ));
        }
        if let Some(g) = self.groups.iter().position(|g| g.subject.trim().is_empty()) {
            return Some((
                crate::i18n::message(
                    "repo-workflows-surgery-problem-cd79fde0",
                    &[("g-1-1", (g + 1).clone().into())],
                ),
                true,
            ));
        }
        None
    }

    /// For each new commit but the last, the patch from the parent to it: its changes and those
    /// of the commits before it. The last is the old commit's tree, whatever is left.
    pub fn cumulative_patches(&self) -> Vec<String> {
        (0..self.groups.len().saturating_sub(1))
            .map(|g| patchset::patch_of(&self.files, |f, unit| self.group_of(f, unit) <= g))
            .collect()
    }

    pub fn messages(&self) -> Vec<String> {
        self.groups.iter().map(Group::message).collect()
    }
}

/// The trees of a split of `hash` (on `parent`): the parent's, then each new commit's but the
/// last the parent with its `patches` applied, the last the old commit's tree. Fails, saying
/// which, when a patch doesn't apply or leaves a commit with no change.
pub(crate) fn prepare(
    repo: &Path,
    hash: &str,
    parent: &str,
    patches: &[String],
) -> Result<Checked, String> {
    let mut trees = vec![git::tree_of(repo, parent)?];
    for (g, patch) in patches.iter().enumerate() {
        let file = git::save_patch_for(repo, patch)?;
        let tree = git::tree_after(repo, parent, &[file]).map_err(|e| {
            crate::i18n::message(
                "repo-workflows-surgery-prepare-1a3632ec",
                &[
                    ("g-1-1", (g + 1).clone().into()),
                    ("e-2", (e).clone().into()),
                ],
            )
        })?;
        trees.push(tree);
    }
    trees.push(git::tree_of(repo, hash)?);
    let mut patches_out = Vec::new();
    for (g, pair) in trees.windows(2).enumerate() {
        let [before, after] = pair else { continue };
        if before == after {
            return Err(crate::i18n::message(
                "repo-workflows-surgery-prepare-9ee5bdbf",
                &[("g-1-1", (g + 1).clone().into())],
            ));
        }
        patches_out.push((
            git::patch_between(repo, before, after)?,
            git::paths_between(repo, before, after)?,
        ));
    }
    Ok(Checked {
        trees,
        preview: Preview::new(&patches_out),
    })
}

/// Makes the new commits of `trees` (the parent's first) on `parent`, each by the old commit
/// `hash`'s author with its message.
pub(crate) fn make_commits(
    repo: &Path,
    hash: &str,
    parent: &str,
    trees: &[String],
    messages: &[String],
) -> Result<Vec<String>, String> {
    let author = git::author_of(repo, hash)?;
    let mut base = parent.to_owned();
    let mut commits = Vec::new();
    for (tree, message) in trees.iter().skip(1).zip(messages) {
        base = git::commit_tree(repo, tree, &base, message, Some(&author))?;
        commits.push(base.clone());
    }
    Ok(commits)
}

impl RepoTab {
    /// Reads commit `hash` and opens the split editor on it.
    pub(in crate::repo) fn open_split(&mut self, hash: String, ctx: &egui::Context) {
        if let Some(why) = self.surgery_refusal(&hash) {
            self.status = Some(crate::repo::Notice::plain((
                format!("Split commit: {why}"),
                true,
            )));
            return;
        }
        if self.absorb.is_some() {
            self.status = Some(crate::repo::Notice::plain((
                "Split commit: close the staged fixes editor first".to_owned(),
                true,
            )));
            return;
        }
        self.status = Some(crate::repo::Notice::plain((
            format!("Split commit: reading {}…", short(&hash)),
            false,
        )));
        let path = self.path.clone();
        self.then(
            ctx,
            move || read(&path, &hash),
            |tab, result, _| match result {
                // Another editor opened meanwhile is left alone.
                Ok(_) if tab.rebase.is_some() || tab.conflict.is_some() => tab.status = None,
                Ok(read) => {
                    tab.status = None;
                    tab.diff = None;
                    tab.file_view = None;
                    tab.split = Some(Box::new(SplitEditor::new(read)));
                }
                Err(e) => {
                    tab.status = Some(crate::repo::Notice::plain((
                        format!("Split commit: {e}"),
                        true,
                    )))
                }
            },
        );
    }

    /// Makes the trees of the split being edited and shows what each new commit changes.
    pub(in crate::repo) fn check_split(&mut self, ctx: &egui::Context) {
        let Some(editor) = self.split.as_mut().filter(|e| !e.busy) else {
            return;
        };
        if matches!(editor.mode, Mode::Move(_)) {
            self.check_move(ctx);
            return;
        }
        if editor.problem().is_some() {
            return;
        }
        editor.busy = true;
        editor.error = None;
        let (id, hash, parent) = (editor.id, editor.hash.clone(), editor.parent.clone());
        let patches = editor.cumulative_patches();
        let path = self.path.clone();
        self.then(
            ctx,
            move || prepare(&path, &hash, &parent, &patches),
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

    /// Makes the checked commits, with the messages as they are now, and puts them in place of
    /// the one split, replaying those after it. The editor stays open until that has started.
    pub(in crate::repo) fn run_split(&mut self, ctx: &egui::Context) {
        if self.busy_op.is_some() {
            return;
        }
        let Some(editor) = self.split.as_mut().filter(|e| !e.busy) else {
            return;
        };
        let Some(checked) = editor.checked.clone() else {
            return;
        };
        if editor.problem().is_some() {
            return;
        }
        if matches!(editor.mode, Mode::Move(_)) {
            self.run_move(ctx);
            return;
        }
        editor.busy = true;
        editor.error = None;
        let (id, hash, parent, messages) = (
            editor.id,
            editor.hash.clone(),
            editor.parent.clone(),
            editor.messages(),
        );
        let label = format!("Split {} into {}", short(&hash), messages.len());
        let path = self.path.clone();
        self.rewrite_with(
            label,
            parent.clone(),
            move |entries| {
                let Some((first, rest)) = entries.split_first() else {
                    return Err(
                        crate::i18n::text("repo-workflows-surgery-run-split-609071bf").to_owned(),
                    );
                };
                if first.commit.hash != hash {
                    return Err(
                        crate::i18n::text("repo-workflows-surgery-run-split-6da7c69e").to_owned(),
                    );
                }
                let made = make_commits(&path, &hash, &parent, &checked.trees, &messages)?;
                let mut todo: Vec<_> = made
                    .into_iter()
                    .zip(&messages)
                    .map(|(made, message)| prepared(made, message, &first.commit))
                    .collect();
                todo.extend(rest.iter().cloned());
                Ok(todo)
            },
            (Vec::new(), Vec::new()),
            Undo::HeadMove { mode: "--keep" },
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

#[cfg(test)]
mod tests {
    use super::*;

    fn editor() -> SplitEditor {
        let patch = "\
diff --git a/a.txt b/a.txt
index 1111111111111111111111111111111111111111..2222222222222222222222222222222222222222 100644
--- a/a.txt
+++ b/a.txt
@@ -1,2 +1,2 @@
-one
+uno
 two
@@ -10,2 +10,2 @@
 nine
-ten
+diez
diff --git a/b.bin b/b.bin
index 1111111111111111111111111111111111111111..3333333333333333333333333333333333333333 100644
Binary files a/b.bin and b/b.bin differ
";
        let files = patchset::files_of(patch.as_bytes(), vec!["a.txt".into(), "b.bin".into()])
            .unwrap_or_default();
        SplitEditor::new(Read {
            hash: "c".into(),
            parent: "p".into(),
            message: "Subject over\ntwo lines\n\nBody line".into(),
            files,
        })
    }

    #[test]
    fn everything_starts_in_the_last_commit_with_the_message_in_the_first() {
        let ed = editor();
        assert_eq!(ed.counts, [0, 5]);
        assert_eq!(ed.groups[0].subject, "Subject over two lines");
        assert_eq!(ed.groups[0].body, "Body line");
        assert_eq!(ed.summary, "Subject over two lines");
        // Said as a hint: that is how it starts.
        assert!(
            ed.problem()
                .is_some_and(|(p, hint)| p.contains("Commit 1 has no changes") && hint)
        );
        assert_eq!(ed.rows.len(), 1 + 8 + 1 + 1);
    }

    #[test]
    fn files_hunks_and_lines_are_handed_out() {
        let mut ed = editor();
        // The first hunk's two lines.
        ed.assign_hunk(0, 0, 0);
        assert_eq!(ed.counts, [2, 3]);
        ed.assign_line(0, 1, 1);
        assert_eq!(ed.counts, [1, 4]);
        ed.assign_file(1, 0);
        assert_eq!(ed.counts, [2, 3]);
        assert!(ed.problem().is_some_and(|(p, _)| p.contains("summary")));
        ed.groups[1].subject = "Second".into();
        assert_eq!(ed.problem(), None);
        let patches = ed.cumulative_patches();
        assert_eq!(patches.len(), 1);
        // "uno" without the removal of "one", and the binary file.
        assert!(patches[0].contains("+uno") && patches[0].contains(" one"));
        assert!(!patches[0].contains("diez") && patches[0].contains("Binary files"));
        assert_eq!(
            ed.messages(),
            ["Subject over two lines\n\nBody line\n", "Second\n"]
        );
        // The last hunk runs to the end of the file; past it there is nothing.
        assert_eq!(ed.hunk_range(0, 4), 5..8);
        assert!(ed.hunk_range(9, 0).is_empty());
    }

    #[test]
    fn a_third_commit_and_taking_one_out() {
        let mut ed = editor();
        ed.add_group();
        assert_eq!((ed.groups.len(), ed.active), (3, 2));
        ed.assign_file(1, 2);
        ed.assign_hunk(0, 0, 0);
        assert_eq!(ed.counts, [2, 2, 1]);
        let patches = ed.cumulative_patches();
        assert_eq!(patches.len(), 2);
        // The second holds what the first does, and more.
        assert!(patches[1].contains("+uno") && patches[1].contains("+diez"));
        ed.remove_group(1);
        assert_eq!(ed.counts, [4, 1]);
        assert_eq!(ed.active, 1);
        ed.remove_group(1);
        assert_eq!(ed.groups.len(), 2, "never fewer than two");
        for _ in 0..20 {
            ed.add_group();
        }
        assert_eq!(ed.groups.len(), MAX_GROUPS);
        // The first taken out: its changes go to the one after, now the first.
        let mut ed = editor();
        ed.add_group();
        ed.assign_hunk(0, 0, 0);
        ed.assign_file(1, 2);
        ed.remove_group(0);
        assert_eq!(ed.counts, [4, 1]);
    }

    #[test]
    fn a_change_drops_what_was_checked_and_folding_lists_less() {
        let mut ed = editor();
        ed.checked = Some(Checked {
            trees: vec![],
            preview: Preview::new(&[]),
        });
        ed.assign_line(0, 1, 0);
        assert!(ed.checked.is_none());
        ed.busy = true;
        ed.assign_line(0, 2, 0);
        assert_eq!(ed.counts, [1, 4], "nothing changes while it is checked");
        ed.toggle_fold(0);
        assert_eq!(ed.rows, [Row::File(0), Row::File(1), Row::Line(1, 0)]);
        ed.toggle_fold(0);
        assert_eq!(ed.rows.len(), 11);
    }
}
