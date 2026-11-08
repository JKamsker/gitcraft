//! What can be done with a commit of the reflog view: a branch made there (the first and safe
//! choice), cherry-picked, checked out, its hash copied; and, behind the usual confirmation,
//! the checked-out branch reset to it.

use egui::Ui;

use super::Pick;
use crate::repo::graph_view::{Rewrite, reset_items};
use crate::repo::{Action, Dialog, Refresh, RepoTab, Undo, ops, short, widgets};

/// Makes a branch at the commit, not checked out: getting it back touches nothing else.
pub(super) fn create_branch(pick: &Pick) -> Action {
    Action::Dialog(Dialog::CreateBranch {
        at: pick.hash.clone(),
        at_label: short(&pick.hash).to_owned(),
        name: String::new(),
        checkout: false,
        focus: true,
    })
}

/// Applies the commit's changes on the checked-out branch, as the graph's menu does.
pub(super) fn cherry_pick(pick: &Pick) -> Action {
    Action::UndoableOps {
        label: format!("Cherry-pick {}", short(&pick.hash)),
        ops: ops(&[&["cherry-pick", &pick.hash]]),
        refresh: Refresh::All,
        undo: Undo::HeadMove { mode: "--keep" },
    }
}

/// Checks the commit out, detached.
pub(super) fn checkout(pick: &Pick) -> Action {
    Action::Checkout(pick.hash.clone())
}

/// Applies a dropped stash back, its staged changes staged again.
pub(super) fn apply_stash(pick: &Pick) -> Action {
    Action::Ops {
        label: format!("Apply stash {}", short(&pick.hash)),
        ops: ops(&[&["stash", "apply", "--index", &pick.hash]]),
        refresh: Refresh::All,
    }
}

/// Why the commit can't be cherry-picked, if it can't; `in_head`: what is checked out has it.
fn no_cherry_pick(pick: &Pick, head: &str, in_head: bool) -> Option<&'static str> {
    if pick.hash == head {
        Some("It is the commit checked out")
    } else if pick.stash {
        Some("A dropped stash: apply it instead")
    } else if pick.merge {
        Some("A merge commit is cherry-picked from the terminal, choosing its parent (-m)")
    } else if in_head {
        Some("What is checked out has it already")
    } else {
        None
    }
}

const BUSY: &str = "An operation is running";
const CHERRY_PICK_TIP: &str = "Apply its changes on top of what is checked out, as a new commit";

impl RepoTab {
    /// Whether the detail pane says what is checked out has `pick` (read for the selected one).
    fn in_head(&self, pick: &Pick) -> bool {
        self.detail
            .as_ref()
            .is_some_and(|d| d.hash == pick.hash && d.in_head)
    }

    /// The buttons under the list, for the commit selected.
    pub(super) fn reflog_buttons(&self, ui: &mut Ui, pick: &Pick, acts: &mut Vec<Action>) {
        if widgets::primary_button(ui, "Create branch here…", true, 0.0)
            .on_hover_text(
                "A new branch at this commit; nothing else changes (also a double-click)",
            )
            .clicked()
        {
            acts.push(create_branch(pick));
        }
        self.recover_items(ui, pick, acts);
        let more = crate::icons::small_button(ui, "chevron-down", "More");
        egui::Popup::menu(&more)
            .style(crate::theme::menu_style)
            .show(|ui| self.reflog_menu_items(ui, pick, acts));
    }

    /// A commit's right-click menu, and the More button's.
    pub(super) fn reflog_menu_items(&self, ui: &mut Ui, pick: &Pick, acts: &mut Vec<Action>) {
        let idle = self.busy_op.is_none();
        let head = self.refs.head.as_str();
        if crate::menu_item::MenuItem::new("Create branch here…")
            .icon("git-branch-plus")
            .show(ui)
            .clicked()
        {
            acts.push(create_branch(pick));
        }
        self.recover_items(ui, pick, acts);
        if self.graph_has(&pick.hash)
            && crate::menu_item::MenuItem::new(crate::i18n::text(
                "repo-workflows-reflog-reflog-menu-items-916e34d4",
            ))
            .icon("command")
            .show(ui)
            .clicked()
        {
            acts.push(Action::SelectHash(pick.hash.clone()));
        }
        ui.separator();
        let target = self
            .refs
            .current
            .clone()
            .unwrap_or_else(|| "HEAD".to_owned());
        let upstream = self
            .refs
            .current
            .as_deref()
            .and_then(|b| self.upstream_ref(b));
        let rewrite = Rewrite {
            tab: self,
            merge: false,
            upstream: upstream.as_deref(),
            hash: &pick.hash,
            short: short(&pick.hash),
            summary: &pick.summary,
            target: &target,
            head,
        };
        let why = if head.is_empty() {
            Some("Nothing is checked out yet")
        } else if pick.hash == head {
            Some("It is the commit checked out")
        } else if !idle {
            Some(BUSY)
        } else {
            None
        };
        {
            crate::menu_item::MenuItem::new(format!("Reset {target} to here"))
                .icon("undo-2")
                .refused(why)
                .submenu(ui, |ui| {
                    reset_items(ui, &rewrite, acts);
                })
                .response
                .on_disabled_hover_text(why.unwrap_or(BUSY));
        }
    }

    /// Apply (a dropped stash), cherry-pick, check out and copy, as buttons or menu entries.
    fn recover_items(&self, ui: &mut Ui, pick: &Pick, acts: &mut Vec<Action>) {
        let idle = self.busy_op.is_none();
        let head = self.refs.head.as_str();
        if pick.stash
            && crate::menu_item::MenuItem::new("Apply stash")
                .icon("archive")
                .enabled(idle)
                .show(ui)
                .on_hover_text("Put its changes back on the working tree (git stash apply)")
                .on_disabled_hover_text(BUSY)
                .clicked()
        {
            acts.push(apply_stash(pick));
        }
        let why = no_cherry_pick(pick, head, self.in_head(pick));
        if crate::menu_item::MenuItem::new(crate::i18n::text(
            "repo-workflows-reflog-recover-items-4c49fdb6",
        ))
        .icon("command")
        .enabled(idle)
        .refused(why)
        .show(ui)
        .on_hover_text(CHERRY_PICK_TIP)
        .on_disabled_hover_text(why.unwrap_or(BUSY))
        .clicked()
        {
            acts.push(cherry_pick(pick));
        }
        if crate::menu_item::MenuItem::new("Check out (detached)")
            .icon("log-in")
            .enabled(idle)
            .refused((pick.hash == head).then_some("It is the commit checked out"))
            .show(ui)
            .on_hover_text("Check out this commit, on no branch")
            .on_disabled_hover_text(if pick.hash == head {
                "It is the commit checked out"
            } else {
                BUSY
            })
            .clicked()
        {
            acts.push(checkout(pick));
        }
        if crate::menu_item::MenuItem::new("Copy commit hash")
            .icon("copy")
            .show(ui)
            .clicked()
        {
            ui.ctx().copy_text(pick.hash.clone());
        }
    }

    /// Whether the graph has a row for `hash` (in the history read so far).
    fn graph_has(&self, hash: &str) -> bool {
        self.index.contains_key(hash)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pick(hash: &str, merge: bool) -> Pick {
        Pick {
            hash: hash.to_owned(),
            summary: "s".to_owned(),
            merge,
            stash: false,
        }
    }

    #[test]
    fn a_branch_made_here_is_not_checked_out() {
        assert!(matches!(
            create_branch(&pick("0123456789abcdef", false)),
            Action::Dialog(Dialog::CreateBranch { at, at_label, checkout: false, .. })
                if at == "0123456789abcdef" && at_label == "01234567"
        ));
    }

    #[test]
    fn merges_and_the_checked_out_commit_are_not_cherry_picked() {
        assert!(no_cherry_pick(&pick("a", false), "b", false).is_none());
        assert!(no_cherry_pick(&pick("a", true), "b", false).is_some());
        assert!(no_cherry_pick(&pick("a", false), "a", false).is_some());
        // What is checked out has it already: it would come out empty.
        assert!(no_cherry_pick(&pick("a", false), "b", true).is_some());
        let stash = Pick {
            stash: true,
            ..pick("s", true)
        };
        assert_eq!(
            no_cherry_pick(&stash, "b", false),
            Some("A dropped stash: apply it instead")
        );
        assert!(
            matches!(apply_stash(&stash), Action::Ops { ops, .. } if ops == [["stash", "apply", "--index", "s"]])
        );
    }
}
