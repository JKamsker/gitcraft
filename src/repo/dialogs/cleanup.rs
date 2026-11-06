//! _Select stale or merged branches…_: a checklist of the branches that could go, each with
//! why, deleted here (and on the remote, if asked) in one undoable go.

use egui::{RichText, Ui};

use super::{Outcome, cancel_button};
use crate::repo::branch_cleanup::Candidate;
use crate::repo::sidebar::delete_many;
use crate::repo::{Action, Refresh};
use crate::theme::{self, Tokens};

/// The checklist's state.
pub(crate) struct CleanupDialog {
    /// Each branch offered, and whether it is ticked.
    rows: Vec<(Candidate, bool)>,
    /// Also delete the branches they track on the remote.
    on_remote: bool,
    /// Which are merged was still being read when it opened.
    checking: bool,
    /// Reading which are merged failed.
    error: Option<String>,
}

impl CleanupDialog {
    pub fn new(candidates: Vec<Candidate>, checking: bool, error: Option<String>) -> Self {
        Self {
            rows: candidates
                .into_iter()
                .map(|c| {
                    let ticked = c.preselected();
                    (c, ticked)
                })
                .collect(),
            on_remote: false,
            checking,
            error,
        }
    }

    fn chosen(&self) -> Vec<&Candidate> {
        self.rows
            .iter()
            .filter(|(_, ticked)| *ticked)
            .map(|(c, _)| c)
            .collect()
    }

    /// The operation deleting the ticked branches.
    pub(crate) fn action(&self) -> Option<Action> {
        let chosen = self.chosen();
        if chosen.is_empty() {
            return None;
        }
        let on_remote = self.on_remote && chosen.iter().any(|c| c.remote.is_some());
        let (ops, undo) = delete_many(&chosen, on_remote);
        Some(Action::UndoableOps {
            label: delete_label(chosen.len(), on_remote),
            ops,
            refresh: Refresh::All,
            undo,
        })
    }
}

/// What the button and the undo entry say: "Delete 3 branches", "… here and on the remote".
fn delete_label(count: usize, on_remote: bool) -> String {
    let what = if count == 1 {
        "1 branch".to_owned()
    } else {
        format!("{count} branches")
    };
    if on_remote {
        format!("Delete {what} here and on the remote")
    } else {
        format!("Delete {what}")
    }
}

pub(super) fn cleanup(ui: &mut Ui, dialog: &mut CleanupDialog) -> Outcome {
    let t = Tokens::get(ui.ctx());
    ui.set_width(460.0);
    ui.label(RichText::new("Select stale or merged branches").font(theme::semibold(15.0)));
    ui.add_space(4.0);
    ui.add(
        egui::Label::new(
            RichText::new(
                "The ticked branches are deleted. Never the checked-out branch, the default \
                 branch or one checked out in a worktree. Undo brings them back.",
            )
            .color(t.text_dim),
        )
        .wrap(),
    );
    if let Some(e) = &dialog.error {
        ui.add_space(6.0);
        ui.add(egui::Label::new(RichText::new(e).color(t.danger)).wrap());
    } else if dialog.checking {
        ui.add_space(6.0);
        ui.label(
            RichText::new("Still checking which branches are merged: open this again in a moment.")
                .color(t.warning),
        );
    }
    ui.add_space(10.0);
    if dialog.rows.is_empty() {
        ui.label("No branch is merged, stale or deleted on its remote.");
        ui.add_space(14.0);
        return if cancel_button(ui) {
            Outcome::Cancel
        } else {
            Outcome::Open
        };
    }
    select_buttons(ui, &mut dialog.rows);
    checklist(ui, &mut dialog.rows, &t);
    let (count, remote_count) = {
        let chosen = dialog.chosen();
        (
            chosen.len(),
            chosen.iter().filter(|c| c.remote.is_some()).count(),
        )
    };
    ui.add_space(8.0);
    ui.add_enabled(
        remote_count > 0,
        egui::Checkbox::new(
            &mut dialog.on_remote,
            format!("Also delete them on the remote ({remote_count})"),
        ),
    )
    .on_hover_text("Deleting a branch on the remote changes it for everyone")
    .on_disabled_hover_text("None of the ticked branches is on a remote");
    ui.add_space(14.0);
    let on_remote = dialog.on_remote && remote_count > 0;
    let mut outcome = Outcome::Open;
    ui.horizontal(|ui| {
        let danger = egui::Button::new(
            RichText::new(delete_label(count, on_remote))
                .color(egui::Color32::WHITE)
                .font(theme::medium(12.5)),
        )
        .fill(t.danger)
        .min_size(egui::vec2(90.0, 30.0));
        if ui.add_enabled(count > 0, danger).clicked()
            && let Some(action) = dialog.action()
        {
            outcome = Outcome::Done(action);
        }
        if cancel_button(ui) {
            outcome = Outcome::Cancel;
        }
    });
    outcome
}

/// _All_ and _None_.
fn select_buttons(ui: &mut Ui, rows: &mut [(Candidate, bool)]) {
    ui.horizontal(|ui| {
        let ticked = rows.iter().filter(|(_, on)| *on).count();
        ui.label(format!("{ticked} of {} ticked", rows.len()));
        if ui.small_button("All").clicked() {
            rows.iter_mut().for_each(|(_, on)| *on = true);
        }
        if ui.small_button("None").clicked() {
            rows.iter_mut().for_each(|(_, on)| *on = false);
        }
    });
    ui.add_space(4.0);
}

/// A row per branch: its box and name, why it is offered, and its remote branch.
fn checklist(ui: &mut Ui, rows: &mut [(Candidate, bool)], t: &Tokens) {
    egui::ScrollArea::vertical()
        .id_salt("cleanup-checklist")
        .max_height(300.0)
        .auto_shrink([false, true])
        .show(ui, |ui| {
            for (c, on) in rows.iter_mut() {
                ui.checkbox(on, RichText::new(&c.name).font(theme::medium(12.5)))
                    .on_hover_text(&c.name);
                let mut why: Vec<String> = c.reasons.iter().map(|r| r.describe()).collect();
                if let Some((remote, name, _)) = &c.remote {
                    why.push(format!("on {remote} as {name}"));
                }
                ui.horizontal(|ui| {
                    ui.add_space(26.0);
                    ui.add(
                        egui::Label::new(RichText::new(why.join(" · ")).color(t.text_dim).small())
                            .truncate(),
                    );
                });
                ui.add_space(4.0);
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repo::branch_health::{Merged, Reason};

    fn candidate(name: &str, reasons: Vec<Reason>, remote: bool) -> Candidate {
        Candidate {
            name: name.to_owned(),
            target: "a".repeat(40),
            remote: remote.then(|| ("origin".to_owned(), name.to_owned(), "b".repeat(40))),
            track: Vec::new(),
            reasons,
        }
    }

    #[test]
    fn merged_ones_start_ticked_and_stale_ones_not() {
        let merged = Reason::Merged(Merged::Into("main".to_owned()));
        let dialog = CleanupDialog::new(
            vec![
                candidate("done", vec![merged], true),
                candidate("old", vec![Reason::Stale(200)], false),
                candidate("gone", vec![Reason::UpstreamGone], false),
                candidate(
                    "gone-old",
                    vec![Reason::UpstreamGone, Reason::Stale(99)],
                    false,
                ),
            ],
            false,
            None,
        );
        let ticked: Vec<&str> = dialog.chosen().iter().map(|c| c.name.as_str()).collect();
        assert_eq!(ticked, ["done", "gone-old"]);
    }

    #[test]
    fn nothing_ticked_deletes_nothing() -> Result<(), String> {
        let mut dialog = CleanupDialog::new(
            vec![candidate("x", vec![Reason::Stale(100)], false)],
            false,
            None,
        );
        assert!(dialog.action().is_none());
        dialog.rows[0].1 = true;
        // The remote box is only for branches on a remote.
        dialog.on_remote = true;
        let Some(Action::UndoableOps { label, ops, .. }) = dialog.action() else {
            return Err("no delete".to_owned());
        };
        assert_eq!(label, "Delete 1 branch");
        assert!(!ops.iter().any(|op| op.first().is_some_and(|a| a == "push")));
        Ok(())
    }

    #[test]
    fn labels_count_the_branches() {
        assert_eq!(delete_label(1, false), "Delete 1 branch");
        assert_eq!(
            delete_label(12, true),
            "Delete 12 branches here and on the remote"
        );
    }
}
