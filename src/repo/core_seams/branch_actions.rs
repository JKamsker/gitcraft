use super::*;

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
        captions: [None, None],
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
    Action::AskRebase {
        onto: onto.full(),
        label: format!("Rebase {current} onto {}", onto.name),
    }
}

/// Cherry-picks the commit `hash` onto what is checked out.
pub(crate) fn cherry_pick_action(hash: &str) -> Action {
    Action::UndoableOps {
        label: format!("Cherry-pick {}", crate::repo::short(hash)),
        ops: ops(&[&["cherry-pick", hash]]),
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
pub(in crate::repo) fn abort_action(state: InProgress) -> Action {
    let what = state.label().to_lowercase();
    Action::Dialog(Dialog::confirm(
        &format!("Abort {what}"),
        crate::i18n::message("repo-core-0017", &[("what", what.to_string().into())]),
        crate::i18n::text("repo-core-0018"),
        ops(&[&[state.command(), "--abort"]]),
        None,
    ))
}

impl Action {
    /// Registry identity of actions that have a user-editable shortcut. Dynamic targets
    /// remain in their action; a new-branch key always uses HEAD rather than the selection.
    pub(crate) fn key_command(&self) -> Option<Command> {
        Some(match self {
            Self::Key(command) => *command,
            Self::Undo => Command::Undo,
            Self::Redo => Command::Redo,
            Self::ToggleSearch | Self::FocusSearch => Command::Search,
            Self::ToggleTimeline => Command::Timeline,
            Self::Fetch => Command::Fetch,
            Self::Pull => Command::Pull,
            Self::Push => Command::Push,
            Self::Terminal(TerminalAct::Toggle) => Command::Terminal,
            Self::Commit => Command::Commit,
            Self::Reload => Command::Reload,
            Self::Dialog(Dialog::Stash(_)) => Command::Stash,
            _ => return None,
        })
    }
}
