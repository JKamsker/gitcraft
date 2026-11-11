//! _Distribute staged fixes…_ (absorb): each run of staged changes goes into the earlier
//! commit of the branch that last touched those lines, as `git-absorb` does. A run whose lines
//! were all last touched by one commit of the branch is a clear match; one touched by several
//! is only suggested (the newest of them); one no commit of the branch touched (a new file, an
//! older line) stays staged. Checking makes the new commits with the fixes in them (see
//! [`history`]) and shows the fixes and what stays staged; absorbing moves the branch there,
//! leaving the index and the working tree exactly as they are.

mod operation;

mod history;
mod view;

use std::collections::HashMap;
use std::path::Path;

use super::patchset::{self, FilePatch};
use super::split::Preview;
use crate::git::{self, StackCommit};
use crate::repo::diff::Kind;
use crate::repo::operations::expect_ref_at;
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
    /// Something stays staged.
    pub rest: bool,
    /// The branch with the fixes absorbed, made already: absorbing moves the branch there.
    pub new_head: String,
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
            return Some(crate::i18n::text("repo-workflows-surgery-problem-2d960e1c").to_owned());
        }
        if self.targets.is_empty() {
            return Some(crate::i18n::text("repo-workflows-surgery-problem-e836ee78").to_owned());
        }
        if self.choice.iter().all(Option::is_none) {
            return Some(crate::i18n::text("repo-workflows-surgery-problem-ed9bc2e2").to_owned());
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
    targets: &[String],
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
        let name = targets
            .get(*t)
            .map(|h| short(h).to_owned())
            .unwrap_or_default();
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
        return Err(crate::i18n::text("repo-workflows-surgery-prepare-9e68dba0").to_owned());
    }
    let rest_patch = git::patch_between(repo, &absorbed, &staged_tree)?;
    let rest = !rest_patch.is_empty();
    if rest {
        sections.push((
            rest_patch,
            git::paths_between(repo, &absorbed, &staged_tree)?,
        ));
    }
    let fixes: HashMap<String, String> = fixups
        .iter()
        .filter_map(|(t, tree)| Some((targets.get(*t)?.clone(), tree.clone())))
        .collect();
    let oldest = fixups
        .iter()
        .map(|(t, _)| *t)
        .max()
        .and_then(|t| targets.get(t))
        .ok_or(crate::i18n::text("repo-workflows-surgery-prepare-6f254528"))?;
    let new_head = history::absorbed(repo, head, oldest, &fixes, &absorbed)?;
    Ok(Checked {
        fixups,
        staged: staged_tree,
        rest,
        new_head,
        preview: Preview::new(&sections),
    })
}

#[cfg(test)]
mod tests;

/// Stored status identity; presentation labels never supply this raw operation state.
fn in_progress_status(state: git::InProgress) -> String {
    let identity = match state {
        git::InProgress::Merge => "merge",
        git::InProgress::CherryPick => "cherry-pick",
        git::InProgress::Revert => "revert",
        git::InProgress::Rebase => "rebase",
        git::InProgress::Am => "patch",
    };
    format!("Distribute staged fixes: a {identity} is in progress")
}

#[cfg(test)]
mod fluent_raw_operation_tests;
