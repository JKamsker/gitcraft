//! What a commit's menu and the command palette do to the checked-out branch from a commit:
//! cherry-pick it, revert it, reset to it.

use super::operations::{expect_ref_at, expect_symref};
use super::{Action, Dialog, Refresh, Undo, ops};

/// The commit an action rewrites the checked-out branch from.
pub(crate) struct Rewrite<'a> {
    pub hash: &'a str,
    pub short: &'a str,
    pub summary: &'a str,
    /// The checked-out branch, or `HEAD` when detached.
    pub target: &'a str,
    /// HEAD's commit, as the view saw it.
    pub head: &'a str,
}

pub(crate) fn cherry_pick_action(hash: &str, short: &str) -> Action {
    Action::UndoableOps {
        label: format!("Cherry-pick {short}"),
        ops: ops(&[&["cherry-pick", hash]]),
        refresh: Refresh::All,
        undo: Undo::HeadMove { mode: "--keep" },
    }
}

pub(crate) fn revert_action(hash: &str, short: &str) -> Action {
    Action::UndoableOps {
        label: format!("Revert {short}"),
        ops: ops(&[&["revert", "--no-edit", hash]]),
        refresh: Refresh::All,
        undo: Undo::HeadMove { mode: "--keep" },
    }
}

/// A way to reset: its name and what it does to the changes.
pub(crate) struct ResetMode {
    pub label: &'static str,
    /// The first sentence of `explain`, for a menu entry.
    pub short: &'static str,
    explain: &'static str,
    mode: &'static str,
    undo_mode: &'static str,
}

pub(crate) const RESET_MODES: [ResetMode; 4] = [
    ResetMode {
        label: "Soft",
        short: "Keeps every change, staged",
        explain: "Keeps every change, staged.",
        mode: "--soft",
        undo_mode: "--soft",
    },
    ResetMode {
        label: "Mixed",
        short: "Keeps every change, unstaged",
        explain: "Keeps every change, unstaged.",
        mode: "--mixed",
        undo_mode: "--mixed",
    },
    ResetMode {
        label: "Keep",
        short: "Keeps local changes; stops if changed files would be touched",
        explain: "Moves the branch while preserving uncommitted changes. Stops without moving \
                  the branch if a file changed locally must also change for this reset.",
        mode: "--keep",
        undo_mode: "--keep",
    },
    ResetMode {
        label: "Hard",
        short: "Throws away uncommitted changes and the commits after this one",
        explain: "Throws away uncommitted changes and the commits after this one. Uncommitted \
                  work can't be recovered.",
        mode: "--hard",
        undo_mode: "--keep",
    },
];

/// Asks before moving `r.target` to the commit the way `mode` says.
pub(crate) fn reset_action(r: &Rewrite<'_>, mode: &ResetMode) -> Action {
    let Rewrite {
        hash,
        short,
        summary,
        target,
        head,
    } = *r;
    Action::Dialog(
        Dialog::confirm(
            &format!("{} reset", mode.label),
            format!("Move {target} to {short} ({summary})? {}", mode.explain),
            &format!("Reset {}", mode.label.to_lowercase()),
            reset_steps(mode.mode, hash, target, head),
            None,
        )
        .undo(Undo::HeadMove {
            mode: mode.undo_mode,
        }),
    )
}

/// `git reset <mode> <hash>` of the branch `target` (or of a detached `HEAD`), which the dialog
/// asking first named, at `head`: it resets whatever is checked out when it runs, so it refuses
/// once that has moved (another branch checked out, a commit made, in the terminal, while the
/// dialog was open). A hard reset would throw away that branch's uncommitted work. That it is
/// still `target` that is checked out is checked by name: another branch at the same commit
/// resolves the same.
pub(crate) fn reset_steps(mode: &str, hash: &str, target: &str, head: &str) -> Vec<Vec<String>> {
    let checked_out = if target == "HEAD" {
        String::new()
    } else {
        format!("refs/heads/{target}")
    };
    let mut steps = vec![
        expect_symref("HEAD", &checked_out),
        expect_ref_at("HEAD", head),
    ];
    steps.extend(ops(&[&["reset", mode, hash]]));
    steps
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn keep_is_a_shared_reset_mode_with_a_guarded_plan_and_safe_undo() -> Result<(), String> {
        assert_eq!(RESET_MODES.len(), 4);
        let keep = RESET_MODES
            .iter()
            .find(|m| m.label == "Keep")
            .ok_or("Missing Keep reset")?;
        assert_eq!(keep.mode, "--keep");
        assert_eq!(keep.undo_mode, "--keep");
        let plan = reset_steps(keep.mode, "target", "main", "expected");
        assert_eq!(plan[0], expect_symref("HEAD", "refs/heads/main"));
        assert_eq!(plan[1], expect_ref_at("HEAD", "expected"));
        assert_eq!(plan[2], vec!["reset", "--keep", "target"]);
        Ok(())
    }
}
