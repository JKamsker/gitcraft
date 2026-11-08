//! Stashing some of the changes: the files picked, only what is staged, or a snapshot that
//! saves a stash and leaves every change where it is. Each goes through the undo history: undo
//! pops the stash back (or, for a snapshot, drops it), redo makes it again.

use std::collections::{BTreeSet, HashSet};
use std::path::Path;
use std::time::Instant;

use super::diff::literal_path;
use super::messages::spawn_or;
use super::operations::{
    expect_ref, expect_tree, is_expectation, run_steps_counted, stash_push_args,
};
use super::{Msg, OpKind, Refresh, RepoTab, Undo, ops};
use crate::git::{self, WorkStatus};

/// What gets stashed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum StashMode {
    /// Every change (`git stash push`).
    Everything,
    /// These files only (`git stash push -- <paths>`).
    Files(Vec<String>),
    /// Only what is staged (`git stash push --staged`, git 2.35+).
    Staged,
    /// Every change saved as a stash, the working tree and index left as they are.
    Snapshot,
}

/// A stash asked for in the dialog.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct StashRequest {
    pub(crate) mode: StashMode,
    /// Optional; git names the stash after the last commit without one.
    pub(crate) message: String,
    /// Untracked files go too (for a mode that can take them; picked ones always do).
    pub(crate) untracked: bool,
}

impl StashRequest {
    /// Every change, untracked files included: what the toolbar's Stash did all along.
    #[cfg(test)]
    pub(crate) fn everything(message: &str) -> Self {
        StashRequest {
            mode: StashMode::Everything,
            message: message.to_owned(),
            untracked: true,
        }
    }

    /// The name of the operation, in the status line and the undo history.
    pub(crate) fn label(&self) -> &'static str {
        match self.mode {
            StashMode::Everything => "Stash",
            StashMode::Files(_) => "Stash files",
            StashMode::Staged => "Stash staged",
            StashMode::Snapshot => "Snapshot",
        }
    }
}

/// Whether the repository's git can stash only the staged changes, once asked.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) enum StagedSupport {
    #[default]
    Unknown,
    Checking,
    Known(Result<(), String>),
    /// git couldn't be asked (the machine wasn't reached): asked again when the dialog opens
    /// next.
    Failed(String),
}

/// A tab's part in this: the files picked to stash (Ctrl+click in the changes), and whether
/// its git can stash only what is staged.
#[derive(Default)]
pub(crate) struct StashPicks {
    pub(crate) picked: BTreeSet<String>,
    pub(crate) staged: StagedSupport,
}

impl StashPicks {
    /// Picks or unpicks `path`.
    pub(crate) fn toggle(&mut self, path: &str) {
        if !self.picked.remove(path) {
            self.picked.insert(path.to_owned());
        }
    }

    /// Forgets the picked files that have no changes any more (stashed, committed, discarded).
    pub(crate) fn keep_changed(&mut self, work: &WorkStatus) {
        if self.picked.is_empty() {
            return;
        }
        let changed: HashSet<&str> = all_files(work).map(|f| f.path.as_str()).collect();
        self.picked.retain(|p| changed.contains(p.as_str()));
    }
}

fn all_files(work: &WorkStatus) -> impl Iterator<Item = &git::StatusFile> {
    work.conflicted
        .iter()
        .chain(&work.staged)
        .chain(&work.unstaged)
}

/// Why the file of the row `f` can't be stashed by its name, if it can't: `git stash push --
/// <paths>` refuses a path only HEAD knows (a staged deletion, the old side of a staged rename),
/// and with untracked files it does so after saving the stash, the changes left in place.
/// Messages read after "<operation> failed: ", so they start in lower case.
pub(crate) fn row_problem(f: &git::StatusFile) -> Option<String> {
    let path = &f.path;
    if f.area == git::Area::Conflicted {
        return Some(crate::i18n::message(
            "repo-workflows-partial_stash-row-problem-a2071523",
            &[("path", (path).clone().into())],
        ));
    }
    if let Some(old) = &f.old_path {
        return Some(crate::i18n::message(
            "repo-workflows-partial_stash-row-problem-78f72f91",
            &[
                ("old", (old).clone().into()),
                ("path", (path).clone().into()),
            ],
        ));
    }
    if f.area == git::Area::Staged && f.code == 'D' {
        return Some(crate::i18n::message(
            "repo-workflows-partial_stash-row-problem-140ed58d",
            &[("path", (path).clone().into())],
        ));
    }
    None
}

/// How long the picked paths may be together: they go on one command line, which Windows
/// keeps under 32 767 characters.
const PATHS_BUDGET: usize = 24 * 1024;

/// Why git can't stash just `paths` as they are in `work`, if it can't (see [`row_problem`]).
pub(crate) fn files_problem(paths: &[String], work: &WorkStatus) -> Option<String> {
    if paths.is_empty() {
        return Some(crate::i18n::message(
            "repo-workflows-partial_stash-files-problem-bdb546dd",
            &[(
                "crate-manual-command-1",
                (crate::manual::command()).clone().into(),
            )],
        ));
    }
    let bytes: usize = paths.iter().map(|p| p.len() + 12).sum();
    if bytes > PATHS_BUDGET {
        return Some(crate::i18n::message(
            "repo-workflows-partial_stash-files-problem-0a39f709",
            &[("paths-len-1", (paths.len()).clone().into())],
        ));
    }
    for path in paths {
        let mut entries = all_files(work).filter(|f| &f.path == path).peekable();
        if entries.peek().is_none() {
            return Some(crate::i18n::message(
                "repo-workflows-partial_stash-files-problem-9175a257",
                &[("path", (path).clone().into())],
            ));
        }
        if let Some(problem) = entries.find_map(row_problem) {
            return Some(problem);
        }
    }
    None
}

/// Whether `work` has changes besides untracked files (what a stash without them can take).
pub(crate) fn has_tracked_changes(work: &WorkStatus) -> bool {
    !work.staged.is_empty()
        || !work.conflicted.is_empty()
        || work.unstaged.iter().any(|f| f.area != git::Area::Untracked)
}

/// Why stashing everything (or a snapshot) can't be done now, if it can't.
pub(crate) fn everything_problem(work: &WorkStatus, untracked: bool) -> Option<String> {
    if work.is_clean() {
        Some(crate::i18n::text("repo-workflows-partial_stash-everything-problem-a7f01d16").into())
    } else if !untracked && !has_tracked_changes(work) {
        Some(crate::i18n::text("repo-workflows-partial_stash-everything-problem-63cc5481").into())
    } else {
        None
    }
}

/// Keeps of `files` those that still have changes in `work`.
pub(crate) fn keep_changed_files(files: &mut Vec<String>, work: &WorkStatus) {
    files.retain(|p| all_files(work).any(|f| &f.path == p));
}

/// Whether any of `paths` is untracked (it then needs `--include-untracked`).
pub(crate) fn has_untracked(paths: &[String], work: &WorkStatus) -> bool {
    work.unstaged
        .iter()
        .any(|f| f.area == git::Area::Untracked && paths.contains(&f.path))
}

/// The `git stash push` that makes the stash `request` asks for, or why it can't be made.
pub(crate) fn push_args(request: &StashRequest, work: &WorkStatus) -> Result<Vec<String>, String> {
    let mut args = vec!["stash".to_owned(), "push".to_owned()];
    let paths = match &request.mode {
        StashMode::Snapshot => {
            return Err(crate::i18n::text("repo-workflows-partial_stash-push-args-0a7314b3").into());
        }
        StashMode::Everything => {
            if let Some(problem) = everything_problem(work, request.untracked) {
                return Err(problem);
            }
            if request.untracked {
                args = stash_push_args().map(str::to_owned).to_vec();
            }
            None
        }
        StashMode::Staged => {
            if work.staged.is_empty() {
                return Err(
                    crate::i18n::text("repo-workflows-partial_stash-push-args-e90b2782").into(),
                );
            }
            args.push("--staged".to_owned());
            None
        }
        StashMode::Files(paths) => {
            if let Some(problem) = files_problem(paths, work) {
                return Err(problem);
            }
            if has_untracked(paths, work) {
                args.push("--include-untracked".to_owned());
            }
            Some(paths)
        }
    };
    let message = request.message.trim();
    if !message.is_empty() {
        args.extend(["-m".to_owned(), message.to_owned()]);
    }
    if let Some(paths) = paths {
        args.push("--".to_owned());
        let unique: BTreeSet<&String> = paths.iter().collect();
        args.extend(unique.into_iter().map(|p| literal_path(p)));
    }
    Ok(args)
}

/// Popping the stash back, but only while it is still the newest one.
fn pop_undo() -> Vec<Vec<String>> {
    [expect_ref("refs/stash")]
        .into_iter()
        .chain(ops(&[&["stash", "pop", "--index"]]))
        .collect()
}

/// Makes the undo of a stash of only what is staged put the staged changes back with `git
/// apply` (the working tree first, then the index), then drop the stash: `git stash pop --index`
/// refuses while the files it changes have other changes, which a file staged in part always
/// has. The patch is made as `git stash` made the one it took out with (one line of context), so
/// that it goes back in where that came out. Left as it is (popping) when the patch can't be
/// kept (it isn't text git can read back).
fn reapply_staged_undo(path: &Path, undo: &mut Vec<Vec<String>>) {
    let patch = git::run_bytes(
        Some(path),
        &[
            "diff-tree",
            "-p",
            "--binary",
            "--full-index",
            "--no-color",
            "-U1",
            "stash@{0}^1",
            "stash@{0}^2",
            "--",
        ],
    );
    let Some(patch) = patch.ok().and_then(|p| String::from_utf8(p).ok()) else {
        return;
    };
    let tree = |rev: &str| {
        git::run(
            Some(path),
            &["rev-parse", "--verify", "-q", &format!("{rev}^{{tree}}")],
        )
        .map(|s| s.trim().to_owned())
    };
    let (Ok(base), Ok(staged)) = (tree("stash@{0}^1"), tree("stash@{0}^2")) else {
        return;
    };
    let Ok(file) = git::save_patch_for(path, &patch) else {
        return;
    };
    // The check that the stash is still the newest stays first. Then that it holds what the
    // patch was made from: redone, it is made again from what is staged then, which may be
    // more, and dropping it would lose that.
    undo.retain(|step| is_expectation(step));
    undo.extend([
        expect_tree("stash@{0}^1", &base),
        expect_tree("stash@{0}^2", &staged),
    ]);
    // Both checked before either is applied, so that it isn't left half done.
    undo.extend(ops(&[
        &["apply", "--check", "--whitespace=nowarn", &file],
        &["apply", "--cached", "--check", "--whitespace=nowarn", &file],
        &["apply", "--whitespace=nowarn", &file],
        &["apply", "--cached", "--whitespace=nowarn", &file],
        &["stash", "drop", "-q"],
    ]));
}

/// `git stash store` of a snapshot, as its redo runs it again.
fn store_args(snapshot: &git::Snapshot) -> Vec<String> {
    ops(&[&["stash", "store", "-m", &snapshot.message, &snapshot.commit]]).concat()
}

/// Makes the stash `request` asks for, in the background, and keeps its undo.
pub(crate) fn run_push(
    path: &Path,
    args: Vec<String>,
    head: String,
    roll_back_staged: bool,
) -> (Result<String, String>, OpKind) {
    let undo = Undo::Custom {
        undo: pop_undo(),
        redo: vec![args.clone()],
    };
    let before = git::resolve(path, "refs/stash").ok();
    let (result, mut kind, _) =
        run_steps_counted(path, &[args], OpKind::Normal(undo, head), None, false, None);
    if roll_back_staged
        && result.is_ok()
        && let OpKind::Normal(Undo::Custom { undo, .. }, _) = &mut kind
    {
        reapply_staged_undo(path, undo);
    }
    let result = match result {
        Ok(out) if out.contains("No local changes to save") => {
            Err(crate::i18n::text("repo-workflows-partial_stash-run-push-a7f01d16").to_owned())
        }
        Err(e) if e.trim() == "No staged changes" => {
            Err(crate::i18n::text("repo-workflows-partial_stash-run-push-e90b2782").to_owned())
        }
        Err(e) => Err(after_failed_push(
            path,
            before.as_deref(),
            &e,
            roll_back_staged,
        )),
        ok => ok,
    };
    (result, kind)
}

/// What to say when `git stash push` failed: git can save the stash and then fail to take the
/// changes out (a staged binary file, unstaged changes next to staged ones). Stashing only what
/// is staged takes them out with one `git apply`, which changes nothing when it fails: that
/// stash is dropped again, so nothing happened. Otherwise it stays, and the message says so.
fn after_failed_push(path: &Path, before: Option<&str>, e: &str, roll_back_staged: bool) -> String {
    let Ok(now) = git::resolve(path, "refs/stash") else {
        return e.to_owned();
    };
    if before == Some(now.as_str()) {
        return e.to_owned();
    }
    // The stash of what is staged has the index as its tree: the index is untouched still.
    let untouched = git::run(Some(path), &["diff", "--cached", "--quiet", &now, "--"]).is_ok();
    if roll_back_staged
        && untouched
        && git::run(Some(path), &["stash", "drop", "-q", "stash@{0}"]).is_ok()
    {
        let why = if e.contains("binary patch") {
            crate::i18n::text("repo-workflows-partial_stash-after-failed-push-289207a8")
        } else {
            crate::i18n::text("repo-workflows-partial_stash-after-failed-push-26daff34")
        };
        return crate::i18n::message(
            "repo-workflows-partial_stash-after-failed-push-8f99c6b0",
            &[
                ("why", (why).clone().into()),
                ("one-line-e-1", (one_line(e)).clone().into()),
            ],
        );
    }
    crate::i18n::message(
        "repo-workflows-partial_stash-after-failed-push-493b498c",
        &[
            ("super-short-now-1", (super::short(&now)).clone().into()),
            ("one-line-e-2", (one_line(e)).clone().into()),
        ],
    )
}

/// What git said, on one line: the status line shows the first line of an error, and the one
/// starting with `error:` before any other, which would hide what is said before it.
fn one_line(said: &str) -> String {
    said.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join("; ")
}

/// Saves a snapshot in the background (see [`git::snapshot`]) and stores it as the newest stash.
fn run_snapshot(
    path: &Path,
    message: &str,
    untracked: bool,
    head: String,
) -> (Result<String, String>, OpKind) {
    let snapshot = match git::snapshot(path, message, untracked) {
        Ok(Some(s)) => s,
        Ok(None) if untracked => {
            return (
                Err(
                    crate::i18n::text("repo-workflows-partial_stash-run-snapshot-f67020b0")
                        .to_owned(),
                ),
                OpKind::plain(),
            );
        }
        Ok(None) => {
            return (
                Err(
                    crate::i18n::text("repo-workflows-partial_stash-run-snapshot-63cc5481")
                        .to_owned(),
                ),
                OpKind::plain(),
            );
        }
        Err(e) => return (Err(e), OpKind::plain()),
    };
    let store = vec![store_args(&snapshot)];
    let undo = Undo::Custom {
        undo: [expect_ref("refs/stash")]
            .into_iter()
            .chain(ops(&[&["stash", "drop", "-q"]]))
            .collect(),
        redo: store.clone(),
    };
    let (result, kind, _) =
        run_steps_counted(path, &store, OpKind::Normal(undo, head), None, false, None);
    (result, kind)
}

impl RepoTab {
    /// Makes the stash asked for in the dialog (or the palette), in the background.
    pub(super) fn stash(&mut self, request: StashRequest, ctx: &egui::Context) {
        if self.busy_op.is_some() {
            return;
        }
        let label = request.label();
        let head = self.refs.head.clone();
        if request.mode == StashMode::Snapshot {
            let StashRequest {
                message, untracked, ..
            } = request;
            return self.stash_job(label, ctx, move |path| {
                run_snapshot(path, &message, untracked, head)
            });
        }
        match push_args(&request, &self.work) {
            Ok(args) => {
                let staged = request.mode == StashMode::Staged;
                self.stash_job(label, ctx, move |path| run_push(path, args, head, staged));
            }
            Err(e) => self.status = Some((format!("{label} failed: {e}"), true)),
        }
    }

    /// Runs `job` in the background as the tab's operation `label`, then takes in how it ended
    /// like any other operation (status line, undo, reload).
    fn stash_job(
        &mut self,
        label: &'static str,
        ctx: &egui::Context,
        job: impl FnOnce(&Path) -> (Result<String, String>, OpKind) + Send + 'static,
    ) {
        self.busy_op = Some(label.to_owned());
        self.op_started = Some(Instant::now());
        self.status = None;
        // Nothing here signs in: an operation run again after signing in must not be this one.
        self.last_op = None;
        let path = self.path.clone();
        let failed = move |e| done(label, Err(e), OpKind::plain());
        spawn_or(&self.tx, ctx, failed, move || {
            let (result, kind) = job(&path);
            done(label, result, kind)
        });
    }

    /// Starts asking the repository's git whether it can stash only the staged changes, unless
    /// it was asked already.
    pub(super) fn check_staged_stash(&mut self, ctx: &egui::Context) {
        if self.stash_picks.staged != StagedSupport::Unknown {
            return;
        }
        self.stash_picks.staged = StagedSupport::Checking;
        let path = self.path.clone();
        self.then(
            ctx,
            move || git::staged_stash_support(&path),
            |tab, found, _| {
                tab.stash_picks.staged = match found {
                    Ok(known) => StagedSupport::Known(known),
                    Err(e) => StagedSupport::Failed(e),
                }
            },
        );
    }
}

fn done(label: &str, result: Result<String, String>, kind: OpKind) -> Msg {
    Msg::OpDone {
        label: label.to_owned(),
        result,
        refresh: Refresh::All,
        kind,
        quiet: false,
        stopped: 0,
    }
}

#[cfg(test)]
mod tests;
