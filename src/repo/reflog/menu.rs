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

/// Why the commit can't be cherry-picked, if it can't.
fn no_cherry_pick(pick: &Pick, head: &str) -> Option<&'static str> {
    if pick.hash == head {
        Some("It is the commit checked out")
    } else if pick.merge {
        Some("A merge commit is cherry-picked from the terminal, choosing its parent (-m)")
    } else {
        None
    }
}

impl RepoTab {
    /// The buttons under the list, for the commit selected.
    pub(super) fn reflog_buttons(&self, ui: &mut Ui, pick: &Pick, acts: &mut Vec<Action>) {
        let idle = self.busy_op.is_none();
        let head = self.refs.head.as_str();
        if widgets::primary_button(ui, "Create branch here…", true, 0.0)
            .on_hover_text(
                "A new branch at this commit; nothing else changes (also a double-click)",
            )
            .clicked()
        {
            acts.push(create_branch(pick));
        }
        let why = no_cherry_pick(pick, head);
        if ui
            .add_enabled(idle && why.is_none(), egui::Button::new("Cherry-pick"))
            .on_hover_text("Apply its changes on the checked-out branch as a new commit")
            .on_disabled_hover_text(why.unwrap_or("An operation is running"))
            .clicked()
        {
            acts.push(cherry_pick(pick));
        }
        if ui
            .add_enabled(
                idle && pick.hash != head,
                egui::Button::new("Check out (detached)"),
            )
            .on_disabled_hover_text(if pick.hash == head {
                "It is the commit checked out"
            } else {
                "An operation is running"
            })
            .clicked()
        {
            acts.push(checkout(pick));
        }
        if ui.button("Copy SHA").clicked() {
            ui.ctx().copy_text(pick.hash.clone());
        }
        let more = crate::icons::small_button(ui, "chevron-down", "More");
        egui::Popup::menu(&more)
            .style(crate::theme::menu_style)
            .show(|ui| self.reflog_menu_items(ui, pick, acts));
    }

    /// A commit's right-click menu, and the More button's.
    pub(super) fn reflog_menu_items(&self, ui: &mut Ui, pick: &Pick, acts: &mut Vec<Action>) {
        let idle = self.busy_op.is_none();
        let head = self.refs.head.as_str();
        if ui.button("Create branch here…").clicked() {
            acts.push(create_branch(pick));
        }
        let why = no_cherry_pick(pick, head);
        if ui
            .add_enabled(idle && why.is_none(), egui::Button::new("Cherry-pick"))
            .on_disabled_hover_text(why.unwrap_or("An operation is running"))
            .clicked()
        {
            acts.push(cherry_pick(pick));
        }
        if ui
            .add_enabled(
                idle && pick.hash != head,
                egui::Button::new("Check out (detached)"),
            )
            .clicked()
        {
            acts.push(checkout(pick));
        }
        if ui.button("Copy SHA").clicked() {
            ui.ctx().copy_text(pick.hash.clone());
        }
        if self.graph_has(&pick.hash) && ui.button("Show in graph").clicked() {
            acts.push(Action::SelectHash(pick.hash.clone()));
        }
        ui.separator();
        let target = self
            .refs
            .current
            .clone()
            .unwrap_or_else(|| "HEAD".to_owned());
        let rewrite = Rewrite {
            hash: &pick.hash,
            short: short(&pick.hash),
            summary: &pick.summary,
            target: &target,
            head,
        };
        let can_reset = idle && !head.is_empty() && pick.hash != head;
        ui.add_enabled_ui(can_reset, |ui| {
            ui.menu_button(format!("Reset {target} to here"), |ui| {
                reset_items(ui, &rewrite, acts);
            });
        });
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
        assert!(no_cherry_pick(&pick("a", false), "b").is_none());
        assert!(no_cherry_pick(&pick("a", true), "b").is_some());
        assert!(no_cherry_pick(&pick("a", false), "a").is_some());
    }
}
