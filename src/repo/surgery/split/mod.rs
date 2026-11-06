//! _Split commit…_: a commit's files, hunks and lines handed out to two or more new commits,
//! each with its own message. Checking prepares the new commits (each patch applied in a
//! scratch index) and shows what each one changes; splitting puts them in place of the old
//! commit and replays the commits after it.

mod view;

use std::collections::HashSet;
use std::path::Path;

use super::patchset::{self, FilePatch};
use super::prepared;
use crate::git;
use crate::repo::diff::Kind;
use crate::repo::{RepoTab, Undo, short};

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
        let (subject, body) = (self.subject.trim(), self.body.trim());
        if body.is_empty() {
            format!("{subject}\n")
        } else {
            format!("{subject}\n\n{body}\n")
        }
    }
}

/// The new commits, made and checked, with what each one changes.
#[derive(Clone, Debug)]
pub(crate) struct Checked {
    pub commits: Vec<String>,
    /// Each commit's patch, and its files.
    pub patches: Vec<(String, Vec<String>)>,
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
    let parent = git::resolve(repo, &format!("{hash}^"))
        .map_err(|_| "the first commit has no parent to split it from".to_owned())?;
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
    let patch = git::patch_between(repo, &parent, hash)?;
    let paths = git::paths_between(repo, &parent, hash)?;
    let files = patchset::files_of(&patch, paths)?;
    Ok(Read {
        hash: hash.to_owned(),
        parent,
        message: message.trim_end().to_owned(),
        files,
    })
}

pub(crate) struct SplitEditor {
    pub hash: String,
    pub parent: String,
    pub summary: String,
    pub files: Vec<FilePatch>,
    /// For each file, the commit (an index into `groups`) each of its units goes to: by line
    /// index for a file picked line by line, at `0` for one that goes whole.
    pub owner: Vec<Vec<usize>>,
    pub groups: Vec<Group>,
    /// The commit a click hands changes to.
    pub active: usize,
    /// Files whose lines are folded away.
    pub folded: HashSet<usize>,
    /// The new commits once checked; any change since drops them.
    pub checked: Option<Checked>,
    /// Being checked or started, in the background.
    pub busy: bool,
    /// What went wrong checking it.
    pub error: Option<String>,
}

impl SplitEditor {
    /// Every change in the last of two commits, the first one active: what is clicked goes
    /// into it. The first keeps the old message, the second starts empty.
    pub fn new(read: Read) -> Self {
        let owner = read
            .files
            .iter()
            .map(|f| vec![1; f.lines.len().max(1)])
            .collect();
        let (subject, body) = read
            .message
            .split_once("\n\n")
            .unwrap_or((&read.message, ""));
        let summary = subject.lines().next().unwrap_or_default().to_owned();
        Self {
            hash: read.hash,
            parent: read.parent,
            summary,
            files: read.files,
            owner,
            groups: vec![
                Group {
                    subject: subject.trim().to_owned(),
                    body: body.trim().to_owned(),
                },
                Group::default(),
            ],
            active: 0,
            folded: HashSet::new(),
            checked: None,
            busy: false,
            error: None,
        }
    }

    /// The commit unit `unit` of file `f` goes to.
    pub fn group_of(&self, f: usize, unit: usize) -> usize {
        self.owner
            .get(f)
            .and_then(|o| o.get(unit))
            .copied()
            .unwrap_or(0)
    }

    fn changed(&mut self) {
        self.checked = None;
        self.error = None;
    }

    /// Hands the units of file `f` that `which` keeps to commit `g`.
    fn assign(&mut self, f: usize, g: usize, which: impl Fn(usize) -> bool) {
        if self.busy || g >= self.groups.len() {
            return;
        }
        let units = self.files.get(f).map(FilePatch::units).unwrap_or_default();
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

    /// The hunk whose header is line `h` of file `f`.
    pub fn assign_hunk(&mut self, f: usize, h: usize, g: usize) {
        let end = self.files.get(f).map_or(h, |file| {
            file.lines[h + 1..]
                .iter()
                .position(|l| l.kind == Kind::Hunk)
                .map_or(file.lines.len(), |n| h + 1 + n)
        });
        self.assign(f, g, |u| u > h && u < end);
    }

    pub fn assign_line(&mut self, f: usize, i: usize, g: usize) {
        self.assign(f, g, |u| u == i);
    }

    /// How many units each commit has.
    pub fn counts(&self) -> Vec<usize> {
        let mut counts = vec![0; self.groups.len()];
        for (f, file) in self.files.iter().enumerate() {
            for unit in file.units() {
                if let Some(c) = counts.get_mut(self.group_of(f, unit)) {
                    *c += 1;
                }
            }
        }
        counts
    }

    pub fn add_group(&mut self) {
        if self.groups.len() < MAX_GROUPS && !self.busy {
            self.groups.push(Group::default());
            self.active = self.groups.len() - 1;
            self.changed();
        }
    }

    /// Takes commit `g` out; its changes go to the one before it (the first: after it).
    pub fn remove_group(&mut self, g: usize) {
        if self.groups.len() <= 2 || g >= self.groups.len() || self.busy {
            return;
        }
        let into = g.saturating_sub(1);
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

    /// Why it can't be split as set up, if it can't.
    pub fn problem(&self) -> Option<String> {
        let units: usize = self.files.iter().map(|f| f.units().len()).sum();
        if units < 2 {
            return Some("This commit has only one change: there is nothing to split".to_owned());
        }
        let counts = self.counts();
        if let Some(g) = counts.iter().position(|&c| c == 0) {
            return Some(format!(
                "Commit {} has no changes: click files, hunks or lines to give it some",
                g + 1
            ));
        }
        if let Some(g) = self.groups.iter().position(|g| g.subject.trim().is_empty()) {
            return Some(format!("Commit {} needs a summary", g + 1));
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

/// Makes the new commits of a split of `hash` (on `parent`): each but the last the parent with
/// its `patches` applied, the last the old commit's tree, each by the old commit's author with
/// its message. Fails, saying which, when a patch doesn't apply or leaves a commit empty.
pub(crate) fn prepare(
    repo: &Path,
    hash: &str,
    parent: &str,
    patches: &[String],
    messages: &[String],
) -> Result<Checked, String> {
    let author = git::author_of(repo, hash)?;
    let mut trees = vec![git::tree_of(repo, parent)?];
    for (g, patch) in patches.iter().enumerate() {
        let file = git::save_patch_for(repo, patch)?;
        let tree =
            git::tree_after(repo, parent, &[file]).map_err(|e| format!("commit {}: {e}", g + 1))?;
        trees.push(tree);
    }
    trees.push(git::tree_of(repo, hash)?);
    let mut commits = Vec::new();
    let mut patches_out = Vec::new();
    let mut base = parent.to_owned();
    for (g, pair) in trees.windows(2).enumerate() {
        let [before, after] = pair else { continue };
        if before == after {
            return Err(format!("commit {} would change nothing", g + 1));
        }
        let message = messages.get(g).cloned().unwrap_or_default();
        let made = git::commit_tree(repo, after, &base, &message, Some(&author))?;
        patches_out.push((
            git::patch_between(repo, before, after)?,
            git::paths_between(repo, before, after)?,
        ));
        commits.push(made.clone());
        base = made;
    }
    Ok(Checked {
        commits,
        patches: patches_out,
    })
}

impl RepoTab {
    /// Reads commit `hash` and opens the split editor on it.
    pub(in crate::repo) fn open_split(&mut self, hash: String, ctx: &egui::Context) {
        if let Some(why) = self.surgery_refusal(&hash) {
            self.status = Some((format!("Split commit: {why}"), true));
            return;
        }
        let path = self.path.clone();
        self.then(
            ctx,
            move || read(&path, &hash),
            |tab, result, _| match result {
                Ok(read) => {
                    tab.diff = None;
                    tab.rebase = None;
                    tab.file_view = None;
                    tab.split = Some(Box::new(SplitEditor::new(read)));
                }
                Err(e) => tab.status = Some((format!("Split commit: {e}"), true)),
            },
        );
    }

    /// Prepares the new commits of the split being edited and shows what each changes.
    pub(in crate::repo) fn check_split(&mut self, ctx: &egui::Context) {
        let Some(editor) = self.split.as_mut().filter(|e| !e.busy) else {
            return;
        };
        if editor.problem().is_some() {
            return;
        }
        editor.busy = true;
        editor.error = None;
        let (hash, parent) = (editor.hash.clone(), editor.parent.clone());
        let (patches, messages) = (editor.cumulative_patches(), editor.messages());
        let path = self.path.clone();
        self.then(
            ctx,
            move || prepare(&path, &hash, &parent, &patches, &messages).map(|c| (hash, c)),
            |tab, result, _| {
                let Some(editor) = tab.split.as_mut() else {
                    return;
                };
                editor.busy = false;
                match result {
                    Ok((hash, checked)) if hash == editor.hash => editor.checked = Some(checked),
                    Ok(_) => {}
                    Err(e) => editor.error = Some(e),
                }
            },
        );
    }

    /// Puts the checked commits in place of the one split, replaying those after it.
    pub(in crate::repo) fn run_split(&mut self, ctx: &egui::Context) {
        if self.busy_op.is_some() {
            return;
        }
        let Some(editor) = self.split.as_ref() else {
            return;
        };
        let Some(checked) = editor.checked.clone() else {
            return;
        };
        let (hash, parent, messages) = (
            editor.hash.clone(),
            editor.parent.clone(),
            editor.messages(),
        );
        let label = format!("Split {} into {}", short(&hash), checked.commits.len());
        self.split = None;
        self.rewrite_with(
            label,
            parent,
            move |entries| {
                let Some((first, rest)) = entries.split_first() else {
                    return Err("the branch no longer has the commit".to_owned());
                };
                if first.commit.hash != hash {
                    return Err(
                        "the commit is no longer on the checked-out branch after its parent"
                            .to_owned(),
                    );
                }
                let mut todo: Vec<_> = checked
                    .commits
                    .into_iter()
                    .zip(&messages)
                    .map(|(made, message)| prepared(made, message, &first.commit))
                    .collect();
                todo.extend(rest.iter().cloned());
                Ok(todo)
            },
            Vec::new(),
            Undo::HeadMove { mode: "--keep" },
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
        let files =
            patchset::files_of(patch, vec!["a.txt".into(), "b.bin".into()]).unwrap_or_default();
        SplitEditor::new(Read {
            hash: "c".into(),
            parent: "p".into(),
            message: "Subject\n\nBody line".into(),
            files,
        })
    }

    #[test]
    fn everything_starts_in_the_last_commit_with_the_message_in_the_first() {
        let ed = editor();
        assert_eq!(ed.counts(), [0, 5]);
        assert_eq!(ed.groups[0].subject, "Subject");
        assert_eq!(ed.groups[0].body, "Body line");
        assert_eq!(ed.summary, "Subject");
        assert!(
            ed.problem()
                .is_some_and(|p| p.contains("Commit 1 has no changes"))
        );
    }

    #[test]
    fn files_hunks_and_lines_are_handed_out() {
        let mut ed = editor();
        // The first hunk's two lines.
        ed.assign_hunk(0, 0, 0);
        assert_eq!(ed.counts(), [2, 3]);
        ed.assign_line(0, 1, 1);
        assert_eq!(ed.counts(), [1, 4]);
        ed.assign_file(1, 0);
        assert_eq!(ed.counts(), [2, 3]);
        assert!(ed.problem().is_some_and(|p| p.contains("summary")));
        ed.groups[1].subject = "Second".into();
        assert_eq!(ed.problem(), None);
        let patches = ed.cumulative_patches();
        assert_eq!(patches.len(), 1);
        // "uno" without the removal of "one", and the binary file.
        assert!(patches[0].contains("+uno") && patches[0].contains(" one"));
        assert!(!patches[0].contains("diez") && patches[0].contains("Binary files"));
        assert_eq!(ed.messages(), ["Subject\n\nBody line\n", "Second\n"]);
    }

    #[test]
    fn a_third_commit_and_taking_one_out() {
        let mut ed = editor();
        ed.add_group();
        assert_eq!((ed.groups.len(), ed.active), (3, 2));
        ed.assign_file(1, 2);
        ed.assign_hunk(0, 0, 0);
        assert_eq!(ed.counts(), [2, 2, 1]);
        let patches = ed.cumulative_patches();
        assert_eq!(patches.len(), 2);
        // The second holds what the first does, and more.
        assert!(patches[1].contains("+uno") && patches[1].contains("+diez"));
        ed.remove_group(1);
        assert_eq!(ed.counts(), [4, 1]);
        assert_eq!(ed.active, 1);
        ed.remove_group(1);
        assert_eq!(ed.groups.len(), 2, "never fewer than two");
        for _ in 0..20 {
            ed.add_group();
        }
        assert_eq!(ed.groups.len(), MAX_GROUPS);
    }

    #[test]
    fn a_change_drops_what_was_checked() {
        let mut ed = editor();
        ed.checked = Some(Checked {
            commits: vec![],
            patches: vec![],
        });
        ed.assign_line(0, 1, 0);
        assert!(ed.checked.is_none());
        ed.busy = true;
        ed.assign_line(0, 2, 0);
        assert_eq!(ed.counts(), [1, 4], "nothing changes while it is checked");
    }
}
