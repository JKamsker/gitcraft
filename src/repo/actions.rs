//! What the views ask the tab to do, and the actions several views build.

use std::path::PathBuf;

use super::pull::PullMode;
use super::{
    Dialog, DiffKey, Refresh, Undo, branch_visibility, ci_view, diff, file_view, ops, pr_review,
    quick_edit,
};
use crate::git::{self, InProgress};
use crate::terminal::ShellSpec;

/// Everything the views ask for; applied after drawing so views only borrow the state.
pub(crate) enum Action {
    Select(usize),
    SelectHash(String),
    SelectStash(usize),
    OpenDiff(DiffKey),
    /// Show a pull request in the big area.
    OpenPr(u64),
    /// Open the worktree of a pull request (proposing `branch`, at `sha`), making it first
    /// when there is none.
    OpenPrWorktree {
        number: u64,
        branch: String,
        sha: String,
    },
    ClosePr,
    ReloadPr,
    /// Something done on the pull request shown.
    Pr(pr_review::PrAct),
    /// Show the builds of a commit in the big area, a run of them first if it's given.
    OpenCi {
        sha: String,
        run: Option<u64>,
    },
    CloseCi,
    /// Something done in the pipeline view.
    Ci(ci_view::CiAct),
    CloseDiff,
    Ops {
        label: String,
        ops: Vec<Vec<String>>,
        refresh: Refresh,
    },
    UndoableOps {
        label: String,
        ops: Vec<Vec<String>>,
        refresh: Refresh,
        undo: Undo,
    },
    Checkout(String),
    /// Checks out a local branch behind its upstream, then fast-forwards it.
    CheckoutPull(String),
    CheckoutRemote(String),
    Dialog(Dialog),
    Commit,
    Fetch,
    /// Pull the way the settings say.
    Pull,
    PullWith(PullMode),
    Push,
    Reload,
    LoadMore,
    Undo,
    Redo,
    StashPush(String),
    /// Open a folder (a worktree) in its own tab.
    OpenTab(PathBuf),
    /// Open a worktree the way the settings say: in this tab, or in its own.
    OpenWorktree(PathBuf),
    ToggleTerminal,
    /// Collapse or bring back the branches panel.
    ToggleBranches,
    /// Open the branches panel at a section (by its key), or in its filter when `None`.
    OpenBranches(Option<&'static str>),
    /// Hide, show or solo branches in the graph.
    BranchVisibility(branch_visibility::Change),
    /// Collapse or bring back the detail panel.
    ToggleDetail,
    NewTerminal {
        shell: Option<ShellSpec>,
        cwd: Option<PathBuf>,
    },
    CloseTerminal(usize),
    SelectTerminal(usize),
    ToggleSearch,
    SearchStep(i32),
    CreatePr {
        branch: String,
        base: String,
        title: String,
        body: String,
        draft: bool,
    },
    OpenUrl(String),
    /// Open a path (relative to the repository) with the system's default app.
    OpenPath(PathBuf),
    OpenInEditor(PathBuf),
    SetIdentity(String, String),
    /// Takes the repository's own `user.name` / `user.email` out (which of them it sets), so
    /// that the profile's or the global config's apply again.
    ClearIdentity {
        name: bool,
        email: bool,
    },
    /// How diffs show: side by side, without whitespace changes, the whole file.
    SetDiffOptions {
        split: bool,
        ignore_ws: bool,
        whole_file: bool,
    },
    /// Diffs show the whole file as it is after the change, or the changes.
    DiffAsFile(bool),
    /// Markdown files show rendered in diffs, or as text.
    DiffRendered(bool),
    /// Scroll the diff to the next change (`1`) or the previous one (`-1`).
    DiffStep(i32),
    /// The commit pane lists every file of the commit, or only those it changed.
    ViewAllFiles(bool),
    SaveConflict,
    /// Let Mergiraf solve what it can of the open conflict.
    SolveWithMergiraf,
    /// A file's history, blame or contents in the central pane (`rev`: the commit they are at).
    FileView {
        path: String,
        rev: Option<String>,
        tab: file_view::FileTab,
    },
    CloseFileView,
    /// Open the interactive rebase editor for the commits after `onto`.
    /// Gives a commit of the checked-out branch a new message: amending the latest, rebasing
    /// for an older one.
    Reword {
        hash: String,
        message: String,
    },
    /// Open the editor of a commit's message in the detail pane.
    EditMessage(String),
    /// Save a commit as a patch file, asking where.
    SavePatch {
        hash: String,
        summary: String,
    },
    /// Apply a patch file, asking which.
    ApplyPatch,
    /// Drop or move a commit of the checked-out branch.
    QuickEdit {
        edit: quick_edit::Edit,
        hash: String,
    },
    InteractiveRebase {
        onto: String,
        label: String,
    },
    RunRebase,
    CloseRebase,
    /// Remember these HTTPS credentials and run the operation that needed them again.
    SignIn(git::Credentials),
    /// Ask before deleting the local tag `name` (at the commit `target`), once what it points
    /// at is known (read in the background for a repository on another machine).
    DeleteTag {
        name: String,
        target: String,
    },
    /// Add `pattern` to `.gitignore`; stop tracking `path` if git tracks it.
    Ignore {
        pattern: String,
        path: String,
    },
    /// Stage, unstage or discard these changed lines of the open diff.
    PickLines {
        pick: diff::Pick,
        lines: std::collections::BTreeSet<usize>,
    },
    /// Compare `base` with `target` (the working tree when `None`) in the right pane.
    Compare {
        base: String,
        base_label: String,
        target: Option<String>,
        target_label: String,
        since_fork: bool,
    },
    /// Show only the target's changes since the two diverged, or every difference.
    CompareSinceFork(bool),
    SwapCompare,
    CloseCompare,
    /// Search commit messages, or what commits changed in files.
    SearchCode(bool),
}

/// Compares two branches, given to git by their full refs (a tag of the same name is not
/// what is meant), with their names as labels.
pub(crate) fn compare_branches_action(
    base: BranchRef<'_>,
    target: BranchRef<'_>,
    since_fork: bool,
) -> Action {
    Action::Compare {
        base: base.full(),
        base_label: base.name.to_owned(),
        target: Some(target.full()),
        target_label: target.name.to_owned(),
        since_fork,
    }
}

/// A branch an action brings in or goes onto: git is given its full ref, as a tag (or the
/// other kind of branch) of the same short name would be what `x` means to it; people see the
/// name.
#[derive(Clone, Copy, Debug)]
pub(crate) struct BranchRef<'a> {
    /// `main`, or `origin/main` for a remote-tracking branch.
    pub name: &'a str,
    pub remote: bool,
}

impl<'a> BranchRef<'a> {
    pub(crate) fn local(name: &'a str) -> Self {
        Self {
            name,
            remote: false,
        }
    }

    pub(crate) fn remote(name: &'a str) -> Self {
        Self { name, remote: true }
    }

    /// `refs/heads/main`, `refs/remotes/origin/main`.
    pub(crate) fn full(&self) -> String {
        if self.remote {
            branch_visibility::remote_ref(self.name)
        } else {
            branch_visibility::local_ref(self.name)
        }
    }
}

/// The message git gives merging `branch` into `current` by its short name; given the full
/// ref, it would say `Merge branch 'refs/heads/x'`. Like git, without `into` for `main` and
/// `master`.
pub(crate) fn merge_message(branch: BranchRef<'_>, current: &str) -> String {
    let kind = if branch.remote {
        "remote-tracking branch"
    } else {
        "branch"
    };
    let mut message = format!("Merge {kind} '{}'", branch.name);
    if !matches!(current, "main" | "master") {
        message.push_str(&format!(" into {current}"));
    }
    message
}

pub(crate) fn merge_action(branch: BranchRef<'_>, current: &str) -> Action {
    let message = merge_message(branch, current);
    Action::UndoableOps {
        label: format!("Merge {} into {current}", branch.name),
        ops: ops(&[&["merge", "--no-edit", "-m", &message, &branch.full()]]),
        refresh: Refresh::All,
        undo: Undo::HeadMove { mode: "--keep" },
    }
}

/// Moves the current branch forward to `branch`; fails (and says so) if they have diverged.
pub(crate) fn fast_forward_action(branch: BranchRef<'_>, current: &str) -> Action {
    Action::UndoableOps {
        label: format!("Fast-forward {current} to {}", branch.name),
        ops: ops(&[&["merge", "--ff-only", &branch.full()]]),
        refresh: Refresh::All,
        undo: Undo::HeadMove { mode: "--keep" },
    }
}

pub(crate) fn rebase_action(onto: BranchRef<'_>, current: &str) -> Action {
    Action::UndoableOps {
        label: format!("Rebase {current} onto {}", onto.name),
        ops: ops(&[&["rebase", &onto.full()]]),
        refresh: Refresh::All,
        undo: Undo::HeadMove { mode: "--keep" },
    }
}

/// Opens the interactive rebase editor for the checked-out branch onto `onto`.
pub(crate) fn interactive_rebase_action(onto: BranchRef<'_>) -> Action {
    Action::InteractiveRebase {
        onto: onto.full(),
        label: onto.name.to_owned(),
    }
}

/// Asks before aborting: the conflicts resolved so far (in the index only) are thrown away, with
/// nothing to undo it.
pub(super) fn abort_action(state: InProgress) -> Action {
    let what = state.label().to_lowercase();
    Action::Dialog(Dialog::confirm(
        &format!("Abort {what}"),
        format!(
            "Stop the {what} and go back to where it started? Conflicts resolved so far are \
             lost, and can't be brought back."
        ),
        "Abort",
        ops(&[&[state.command(), "--abort"]]),
        None,
    ))
}
