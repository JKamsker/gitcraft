//! _Distribute staged fixes…_ (absorb): each run of staged changes goes into the earlier
//! commit of the branch that last touched those lines, as `git-absorb` does. A run whose lines
//! were all last touched by one commit of the branch is a clear match; one touched by several
//! is only suggested (the newest of them); one no commit of the branch touched (a new file, an
//! older line) stays staged. Checking makes the fixups' trees and shows them; absorbing makes
//! a fixup commit for each commit and rebases them into it, what wasn't absorbed staged again.

mod view;

use std::collections::{BTreeMap, HashMap};
use std::path::Path;

use super::patchset::{self, FilePatch};
use super::prepared;
use super::split::Preview;
use crate::git::{self, StackCommit};
use crate::repo::diff::Kind;
use crate::repo::rebase::Step;
use crate::repo::{RepoTab, Undo, short};

/// How a run of staged changes matches the branch's commits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Match {
    /// Every line it touches was last touched by this commit (an index into the targets).
    Clear(usize),
    /// Several commits touched its lines; the newest of the branch's is suggested.
    Several(usize),
    /// No commit of the branch touched them.
    None,
}

/// A run of staged changes: added and removed lines one after the other in a file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Hunk {
    pub file: usize,
    /// Its lines (indices into the file's lines).
    pub lines: Vec<usize>,
    pub found: Match,
    /// The commits of the branch that touched its lines, newest first (indices into targets).
    pub touched: Vec<usize>,
}

/// The runs of added and removed lines of `file`, and for each the lines (at HEAD) whose last
/// change decides where it goes: those it removes, or for a run that only adds, the lines
/// around it.
pub(crate) fn runs_of(file: &FilePatch) -> Vec<(Vec<usize>, Vec<u32>)> {
    let lines = &file.lines;
    let mut runs = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        if !matches!(lines[i].kind, Kind::Add | Kind::Del) {
            i += 1;
            continue;
        }
        let start = i;
        let mut run = Vec::new();
        while i < lines.len() && matches!(lines[i].kind, Kind::Add | Kind::Del | Kind::Meta) {
            if matches!(lines[i].kind, Kind::Add | Kind::Del) {
                run.push(i);
            }
            i += 1;
        }
        let mut old: Vec<u32> = run.iter().filter_map(|&k| lines[k].old).collect();
        if old.is_empty() {
            let before = lines[..start]
                .iter()
                .rev()
                .take_while(|l| l.kind != Kind::Hunk)
                .find_map(|l| (l.kind == Kind::Context).then_some(l.old).flatten());
            let after = lines[i..]
                .iter()
                .take_while(|l| l.kind != Kind::Hunk)
                .find_map(|l| (l.kind == Kind::Context).then_some(l.old).flatten());
            old.extend(before.into_iter().chain(after));
        }
        runs.push((run, old));
    }
    runs
}

/// Where a run goes, from who last touched each of its lines (`None`: someone off the
/// branch's own commits) and where those commits are among `targets` (newest first).
pub(crate) fn classify(blamed: &[Option<usize>]) -> (Match, Vec<usize>) {
    let mut touched: Vec<usize> = blamed.iter().flatten().copied().collect();
    touched.sort_unstable();
    touched.dedup();
    let found = match (touched.first(), blamed.is_empty()) {
        (None, _) | (_, true) => Match::None,
        (Some(&newest), false) if touched.len() == 1 && blamed.iter().all(Option::is_some) => {
            Match::Clear(newest)
        }
        (Some(&newest), false) => Match::Several(newest),
    };
    (found, touched)
}

/// What absorbing reads: the branch's commits, what is staged, and where each run goes.
pub(crate) struct Read {
    pub head: String,
    pub targets: Vec<StackCommit>,
    pub files: Vec<FilePatch>,
    pub hunks: Vec<Hunk>,
    pub git_absorb: bool,
}

pub(crate) fn read(repo: &Path, branch: &str) -> Result<Read, String> {
    let head = git::resolve(repo, "HEAD")?;
    let targets = git::absorb_stack(repo, branch)?;
    let (patch, paths) = git::staged_patch(repo)?;
    let files = patchset::files_of(&patch, paths)?;
    let at: HashMap<&str, usize> = targets
        .iter()
        .enumerate()
        .map(|(i, t)| (t.hash.as_str(), i))
        .collect();
    let mut hunks = Vec::new();
    for (f, file) in files.iter().enumerate() {
        // A new file, or one that only goes whole, has no lines of HEAD to follow.
        let new = file
            .raw
            .lines()
            .take(4)
            .any(|l| l.starts_with("new file mode"));
        if file.whole.is_some() || new {
            continue;
        }
        let runs = runs_of(file);
        let wanted: Vec<u32> = runs
            .iter()
            .flat_map(|(_, old)| old.iter().copied())
            .collect();
        let blame = git::blame_lines(repo, &file.path, &wanted)?;
        for (lines, old) in runs {
            let blamed: Vec<Option<usize>> = old
                .iter()
                .map(|l| blame.get(l).and_then(|h| at.get(h.as_str()).copied()))
                .collect();
            let (found, touched) = classify(&blamed);
            hunks.push(Hunk {
                file: f,
                lines,
                found,
                touched,
            });
        }
    }
    Ok(Read {
        head,
        targets,
        files,
        hunks,
        git_absorb: git::has_git_absorb(repo),
    })
}

/// The fixups' trees, made and checked.
#[derive(Clone, Debug)]
pub(crate) struct Checked {
    /// For each commit getting fixes (an index into the targets), the tree of HEAD with them.
    pub fixups: Vec<(usize, String)>,
    /// What is staged now, as a tree: absorbing refuses once that changed.
    pub staged: String,
    /// The patch of what stays staged, where git runs, if anything does.
    pub rest: Option<String>,
    pub preview: Preview,
}

pub(crate) struct AbsorbEditor {
    pub id: u64,
    pub head: String,
    pub targets: Vec<StackCommit>,
    pub files: Vec<FilePatch>,
    pub hunks: Vec<Hunk>,
    /// Where each hunk goes (an index into the targets), `None` staying staged.
    pub choice: Vec<Option<usize>>,
    pub git_absorb: bool,
    pub checked: Option<Checked>,
    pub busy: bool,
    pub error: Option<String>,
    /// The files that stay staged whole (no runs of lines of HEAD to follow).
    pub whole: Vec<String>,
    /// What each hunk's match says, worked out once.
    pub verdicts: Vec<(String, egui::Color32)>,
    /// How tall each hunk's card was last drawn: those out of sight only take their place.
    pub heights: Vec<Option<f32>>,
}

impl AbsorbEditor {
    /// The clear matches taken, the rest staying staged until changed.
    pub fn new(read: Read) -> Self {
        static IDS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        let choice = read
            .hunks
            .iter()
            .map(|h| match h.found {
                Match::Clear(t) => Some(t),
                _ => None,
            })
            .collect();
        let whole = read
            .files
            .iter()
            .enumerate()
            .filter(|(f, _)| !read.hunks.iter().any(|h| h.file == *f))
            .map(|(_, file)| file.path.clone())
            .collect();
        Self {
            whole,
            verdicts: Vec::new(),
            heights: Vec::new(),
            id: IDS.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            head: read.head,
            targets: read.targets,
            files: read.files,
            hunks: read.hunks,
            choice,
            git_absorb: read.git_absorb,
            checked: None,
            busy: false,
            error: None,
        }
    }

    pub fn choose(&mut self, hunk: usize, target: Option<usize>) {
        if self.busy {
            return;
        }
        if let Some(slot) = self.choice.get_mut(hunk) {
            *slot = target.filter(|&t| t < self.targets.len());
            self.checked = None;
            self.error = None;
        }
    }

    /// For each file, the commit each of its lines goes into, if any: worked out once, for
    /// the patches to be made in the background.
    pub fn table(&self) -> Vec<Vec<Option<usize>>> {
        let mut table: Vec<Vec<Option<usize>>> = self
            .files
            .iter()
            .map(|f| vec![None; f.lines.len()])
            .collect();
        for (h, c) in self.hunks.iter().zip(&self.choice) {
            let Some(c) = c else { continue };
            if let Some(row) = table.get_mut(h.file) {
                for &i in &h.lines {
                    if let Some(slot) = row.get_mut(i) {
                        *slot = Some(*c);
                    }
                }
            }
        }
        table
    }

    /// The commits getting fixes, oldest first.
    pub fn chosen_targets(&self) -> Vec<usize> {
        let mut chosen: Vec<usize> = self.choice.iter().flatten().copied().collect();
        chosen.sort_unstable_by(|a, b| b.cmp(a));
        chosen.dedup();
        chosen
    }

    pub fn problem(&self) -> Option<String> {
        if self.hunks.is_empty() {
            return Some(
                "Nothing staged changes lines an earlier commit has: new files and whole-file changes stay staged"
                    .to_owned(),
            );
        }
        if self.targets.is_empty() {
            return Some(
                "The branch has no commits of its own to put fixes in (every one is on another branch or a remote)"
                    .to_owned(),
            );
        }
        if self.choice.iter().all(Option::is_none) {
            return Some("Pick a commit for at least one change".to_owned());
        }
        None
    }
}

/// For each commit getting fixes (`targets`), the patch of them; then of every fix, and of
/// everything staged. `table`: see [`AbsorbEditor::table`].
pub(crate) fn patches(
    files: &[FilePatch],
    table: &[Vec<Option<usize>>],
    targets: &[usize],
) -> (Vec<(usize, String)>, String, String) {
    let at = |f: usize, u: usize| table.get(f).and_then(|r| r.get(u)).copied().flatten();
    let each = targets
        .iter()
        .map(|&t| (t, patchset::patch_of(files, |f, u| at(f, u) == Some(t))))
        .collect();
    let all = patchset::patch_of(files, |f, u| at(f, u).is_some());
    let staged = patchset::patch_of(files, |_, _| true);
    (each, all, staged)
}

/// Makes the trees of the fixups on `head`, of what is staged, and the patch of what stays.
pub(crate) fn prepare(
    repo: &Path,
    head: &str,
    each: &[(usize, String)],
    all: &str,
    staged: &str,
    names: &[String],
) -> Result<Checked, String> {
    let base = git::tree_of(repo, head)?;
    let tree = |patch: &str| -> Result<String, String> {
        if patch.is_empty() {
            return Ok(base.clone());
        }
        let file = git::save_patch_for(repo, patch)?;
        git::tree_after(repo, head, &[file])
    };
    let mut fixups = Vec::new();
    let mut sections = Vec::new();
    for (t, patch) in each {
        let fixed = tree(patch)?;
        let name = names.get(*t).cloned().unwrap_or_default();
        let files = git::paths_between(repo, &base, &fixed)?;
        sections.push((
            git::patch_between(repo, &base, &fixed)?,
            files.into_iter().map(|f| format!("{f} → {name}")).collect(),
        ));
        fixups.push((*t, fixed));
    }
    let absorbed = tree(all)?;
    let staged_tree = tree(staged)?;
    if git::run(
        Some(repo),
        &["diff", "--cached", "--quiet", &staged_tree, "--"],
    )
    .is_err()
    {
        return Err(
            "what is staged changed since this was opened: close it and open it again".to_owned(),
        );
    }
    let rest_patch = git::patch_between(repo, &absorbed, &staged_tree)?;
    let rest = if rest_patch.is_empty() {
        None
    } else {
        let files = git::paths_between(repo, &absorbed, &staged_tree)?;
        sections.push((rest_patch.clone(), files));
        Some(git::save_patch_for(repo, &rest_patch)?)
    };
    Ok(Checked {
        fixups,
        staged: staged_tree,
        rest,
        preview: Preview::new(&sections),
    })
}

impl RepoTab {
    /// Reads what is staged and the branch's commits, and opens the absorb editor.
    pub(in crate::repo) fn open_absorb(&mut self, ctx: &egui::Context) {
        let Some(branch) = self.refs.current.clone() else {
            self.status = Some((
                "Distribute staged fixes: HEAD is detached, check out a branch first".to_owned(),
                true,
            ));
            return;
        };
        if let Some(state) = self.work.in_progress {
            self.status = Some((
                format!(
                    "Distribute staged fixes: a {} is in progress",
                    state.label().to_lowercase()
                ),
                true,
            ));
            return;
        }
        if self.work.staged.is_empty() {
            self.status = Some((
                "Distribute staged fixes: nothing is staged, stage the fixes first".to_owned(),
                true,
            ));
            return;
        }
        if self.split.is_some() || self.rebase.is_some() {
            self.status = Some((
                "Distribute staged fixes: close the editor that is open first".to_owned(),
                true,
            ));
            return;
        }
        self.status = Some(("Distribute staged fixes: reading…".to_owned(), false));
        let path = self.path.clone();
        self.then(
            ctx,
            move || read(&path, &branch),
            |tab, result, _| match result {
                Ok(_) if tab.rebase.is_some() || tab.conflict.is_some() || tab.split.is_some() => {
                    tab.status = Some((
                        "Distribute staged fixes: another editor opened meanwhile".to_owned(),
                        true,
                    ));
                }
                Ok(read) => {
                    tab.status = None;
                    tab.diff = None;
                    tab.file_view = None;
                    tab.absorb = Some(Box::new(AbsorbEditor::new(read)));
                }
                Err(e) => tab.status = Some((format!("Distribute staged fixes: {e}"), true)),
            },
        );
    }

    pub(in crate::repo) fn check_absorb(&mut self, ctx: &egui::Context) {
        let Some(editor) = self.absorb.as_mut().filter(|e| !e.busy) else {
            return;
        };
        if editor.problem().is_some() {
            return;
        }
        let (files, table, targets) = (
            editor.files.clone(),
            editor.table(),
            editor.chosen_targets(),
        );
        let names: Vec<String> = editor
            .targets
            .iter()
            .map(|t| short(&t.hash).to_owned())
            .collect();
        let (id, head) = (editor.id, editor.head.clone());
        editor.busy = true;
        editor.error = None;
        let path = self.path.clone();
        self.then(
            ctx,
            move || {
                let (each, all, staged) = patches(&files, &table, &targets);
                prepare(&path, &head, &each, &all, &staged, &names)
            },
            move |tab, result, _| {
                let Some(editor) = tab.absorb.as_mut().filter(|e| e.id == id) else {
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

    /// Makes a fixup commit for each commit getting fixes and rebases each into its commit;
    /// what wasn't absorbed is staged again after.
    pub(in crate::repo) fn run_absorb(&mut self, ctx: &egui::Context) {
        if self.busy_op.is_some() {
            return;
        }
        let Some(editor) = self.absorb.as_mut().filter(|e| !e.busy) else {
            return;
        };
        let Some(checked) = editor.checked.clone() else {
            return;
        };
        let oldest = checked
            .fixups
            .iter()
            .map(|(t, _)| *t)
            .max()
            .and_then(|t| editor.targets.get(t));
        let Some(oldest) = oldest else {
            return;
        };
        let onto = format!("{}^", oldest.hash);
        let targets: Vec<StackCommit> = editor.targets.clone();
        let (id, head) = (editor.id, editor.head.clone());
        let label = format!(
            "Distribute staged fixes into {} commit{}",
            checked.fixups.len(),
            if checked.fixups.len() == 1 { "" } else { "s" }
        );
        // What is staged must still be what was checked; then it is unstaged (it all is in the
        // working tree, which the rebase stashes and puts back), and what stays is staged again.
        let before = vec![
            ["diff", "--cached", "--quiet", &checked.staged, "--"]
                .map(str::to_owned)
                .to_vec(),
            vec!["reset".to_owned(), "-q".to_owned()],
        ];
        let after = checked
            .rest
            .iter()
            .map(|rest| {
                ["apply", "--cached", "--whitespace=nowarn", rest]
                    .map(str::to_owned)
                    .to_vec()
            })
            .collect();
        editor.busy = true;
        editor.error = None;
        let path = self.path.clone();
        let fixups = checked.fixups.clone();
        self.rewrite_with(
            label,
            onto,
            move |entries| {
                let mut made: BTreeMap<String, crate::repo::rebase::Entry> = BTreeMap::new();
                for (t, tree) in &fixups {
                    let Some(target) = targets.get(*t) else {
                        continue;
                    };
                    let Some(of) = entries.iter().find(|e| e.commit.hash == target.hash) else {
                        return Err(format!(
                            "{} is no longer on the checked-out branch",
                            short(&target.hash)
                        ));
                    };
                    let note = format!("fixup! {}", target.summary);
                    let commit = git::commit_tree(&path, tree, &head, &note, None)?;
                    let mut entry = prepared(commit, &note, &of.commit);
                    entry.step = Step::Fixup;
                    made.insert(target.hash.clone(), entry);
                }
                let mut todo = Vec::new();
                for e in entries {
                    let fix = made.remove(&e.commit.hash);
                    todo.push(e);
                    todo.extend(fix);
                }
                Ok(todo)
            },
            (before, after),
            Undo::HeadMove { mode: "--soft" },
            move |tab, started| {
                // Cancelled meanwhile: nothing runs.
                let Some(editor) = tab.absorb.as_mut().filter(|e| e.id == id) else {
                    return false;
                };
                match started {
                    Ok(()) => {
                        tab.absorb = None;
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

    /// Runs the installed `git-absorb` instead: it makes the fixup commits and rebases them in.
    pub(in crate::repo) fn run_git_absorb(&mut self, ctx: &egui::Context) {
        let Some(editor) = self.absorb.as_ref().filter(|e| !e.busy) else {
            return;
        };
        let steps = [
            self.same_branch(),
            super::super::operations::expect_ref_at("HEAD", &editor.head),
            // Its rebase takes the todo list as it is, whatever editor the user's config sets.
            ["-c", "sequence.editor=true", "absorb", "--and-rebase"]
                .map(str::to_owned)
                .to_vec(),
        ]
        .to_vec();
        self.absorb = None;
        self.run_undoable(
            "git absorb",
            steps,
            crate::repo::Refresh::All,
            Undo::HeadMove { mode: "--soft" },
            ctx,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn where_a_run_goes_from_who_touched_its_lines() {
        // All by the second commit of the branch: clear.
        assert_eq!(classify(&[Some(1), Some(1)]), (Match::Clear(1), vec![1]));
        // Two of the branch's commits: the newer suggested.
        assert_eq!(
            classify(&[Some(2), Some(0)]),
            (Match::Several(0), vec![0, 2])
        );
        // One of the branch's, one older: suggested, not clear.
        assert_eq!(classify(&[Some(3), None]), (Match::Several(3), vec![3]));
        // None of the branch's.
        assert_eq!(classify(&[None, None]), (Match::None, vec![]));
        assert_eq!(classify(&[]), (Match::None, vec![]));
    }

    fn file(patch: &str) -> FilePatch {
        patchset::files_of(patch.as_bytes(), vec!["f".to_owned()])
            .unwrap_or_default()
            .remove(0)
    }

    #[test]
    fn runs_and_the_lines_that_decide_them() {
        let f = file(
            "diff --git a/f b/f\nindex 1111111111111111111111111111111111111111..2222222222222222222222222222222222222222 100644\n--- a/f\n+++ b/f\n@@ -1,6 +1,7 @@\n one\n-two\n+zwei\n three\n+new\n four\n-five\n six\n",
        );
        let runs = runs_of(&f);
        // "two"→"zwei" decided by line 2; "new" by the lines around it (3 and 4); "five" by 5.
        let decided: Vec<Vec<u32>> = runs.iter().map(|(_, old)| old.clone()).collect();
        assert_eq!(decided, [vec![2], vec![3, 4], vec![5]]);
        assert_eq!(runs[0].0.len(), 2);
    }
}
