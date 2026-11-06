//! The agent fence in the tab: its list read with the repository, every linked worktree
//! checked against it in the background (a worktree touching it turns red in the sidebar),
//! the window that edits it and writes it into the agents' configs, and the confirmation a
//! merge of a red worktree's branch needs.

mod editor;
#[cfg(test)]
mod tests;

use std::collections::HashMap;
use std::path::PathBuf;

use super::{Action, Dialog, RepoTab};
use crate::fence::{self, Violation};
use crate::git::Worktree;

pub(crate) use editor::FenceEditor;

/// The title of the confirmation a merge past the fence needs; a merge asked for with it was
/// confirmed.
const CONFIRM: &str = "Merge past the agent fence";

/// What the tab knows of the fence.
#[derive(Default)]
pub(crate) struct FenceState {
    /// The list, as read last.
    pub(crate) list: Vec<String>,
    /// Each linked worktree checked (by its folder): the fenced paths it touched, or why it
    /// couldn't be checked.
    pub(crate) found: HashMap<PathBuf, Result<Vec<Violation>, String>>,
    checking: bool,
    /// Checked again once this check is in (the repository was read again meanwhile).
    again: bool,
    pub(crate) editor: Option<FenceEditor>,
}

impl FenceState {
    /// The fenced paths the worktree at `path` touched (none when not checked or clean).
    pub(crate) fn violations(&self, path: &std::path::Path) -> &[Violation] {
        match self.found.get(path) {
            Some(Ok(v)) => v,
            _ => &[],
        }
    }

    /// Why the worktree at `path` couldn't be checked, if it couldn't.
    pub(crate) fn error(&self, path: &std::path::Path) -> Option<&str> {
        match self.found.get(path) {
            Some(Err(e)) => Some(e),
            _ => None,
        }
    }
}

/// The worktrees checked: the linked ones there are, not the main one.
fn checked(worktrees: &[Worktree]) -> Vec<Worktree> {
    worktrees
        .iter()
        .filter(|w| !w.main && !w.bare && w.prunable.is_none() && !w.head.is_empty())
        .cloned()
        .collect()
}

impl RepoTab {
    /// Reads the list and checks the linked worktrees against it, in the background.
    pub(crate) fn check_fence(&mut self, ctx: &egui::Context) {
        if self.fence.checking {
            self.fence.again = true;
            return;
        }
        let worktrees = checked(&self.worktrees);
        let base = self
            .worktrees
            .iter()
            .find(|w| w.main)
            .map(|w| w.head.clone())
            .unwrap_or_default();
        self.fence.checking = true;
        let repo = self.path.clone();
        self.then(
            ctx,
            move || {
                let list = fence::read(&repo)?;
                let patterns = fence::parse_list(&list);
                let found: HashMap<PathBuf, Result<Vec<Violation>, String>> = worktrees
                    .iter()
                    .map(|w| (w.path.clone(), fence::check(&w.path, &base, &patterns)))
                    .collect();
                Ok::<_, String>((list, found))
            },
            |tab, result, ctx| {
                tab.fence.checking = false;
                match result {
                    Ok((list, found)) => {
                        tab.fence.list = list;
                        tab.fence.found = found;
                    }
                    Err(e) => tab.status = Some((e, true)),
                }
                if std::mem::take(&mut tab.fence.again) {
                    tab.check_fence(ctx);
                }
            },
        );
    }

    /// `action`, or the confirmation it needs first: a merge (or fast-forward) of a branch
    /// checked out in a worktree that touched the fence.
    pub(crate) fn fence_gate(&self, action: Action) -> Action {
        let Action::UndoableOps {
            label,
            ops,
            refresh,
            undo,
        } = action
        else {
            return action;
        };
        let pass = || Action::UndoableOps {
            label: label.clone(),
            ops: ops.clone(),
            refresh,
            undo: undo.clone(),
        };
        if label == CONFIRM {
            return pass();
        }
        let Some(branch) = merged_branch(&ops) else {
            return pass();
        };
        let Some(wt) = self.worktrees.iter().find(|w| {
            w.branch.as_deref() == Some(branch) && !self.fence.violations(&w.path).is_empty()
        }) else {
            return pass();
        };
        let body = confirm_body(branch, wt, self.fence.violations(&wt.path));
        Action::Dialog(Dialog::confirm(CONFIRM, body, "Merge anyway", ops, None).undo(undo))
    }
}

/// The local branch a `merge` step brings in, if the first step is one.
fn merged_branch(ops: &[Vec<String>]) -> Option<&str> {
    let first = ops.first()?;
    if first.first().map(String::as_str) != Some("merge") {
        return None;
    }
    first.last()?.strip_prefix("refs/heads/")
}

/// Most fenced paths a hover or a confirmation lists.
const LISTED: usize = 8;

/// The fenced paths, one per line, `how` each was touched, cut to [`LISTED`].
pub(crate) fn list_violations(found: &[Violation]) -> String {
    let mut out: Vec<String> = found
        .iter()
        .take(LISTED)
        .map(|v| format!("  {} ({}; fence: {})", v.path, v.how.label(), v.pattern))
        .collect();
    if found.len() > LISTED {
        out.push(format!("  and {} more", found.len() - LISTED));
    }
    out.join("\n")
}

fn confirm_body(branch: &str, wt: &Worktree, found: &[Violation]) -> String {
    format!(
        "{branch} is checked out in the worktree {}, which touched paths agents must stay out \
         of:\n{}\n\nMerge it all the same?",
        wt.name(),
        list_violations(found)
    )
}
