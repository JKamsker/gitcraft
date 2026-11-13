//! _Select stale or merged branches…_: a checklist of the branches that could go, each with
//! why, deleted here (and on the remote, if asked) in one undoable go.

use egui::{RichText, Ui};

use super::{Outcome, cancel_button};
use crate::repo::branch_cleanup::Candidate;
use crate::repo::sidebar::delete_many;
use crate::repo::{Action, Refresh};
use crate::theme::{self, Tokens};

/// What _Select stale or merged branches…_ opens: the branches, or the worktrees of merged
/// ones (in a window of their own).
pub(crate) enum CleanupAct {
    Branches,
    Worktrees,
}

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
    /// Worktrees whose branch is merged: offered in a window of their own.
    worktrees: usize,
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
            worktrees: 0,
        }
    }

    /// How many worktrees of merged branches to offer beside the branches.
    pub(crate) fn offer_worktrees(&mut self, count: usize) {
        self.worktrees = count;
    }

    fn chosen(&self) -> Vec<&Candidate> {
        self.rows
            .iter()
            .filter(|(_, ticked)| *ticked)
            .map(|(c, _)| c)
            .collect()
    }

    /// Whether it opened while which branches are merged was still being read.
    pub(crate) fn is_checking(&self) -> bool {
        self.checking
    }

    /// The ticked branches whose remote branch would go too, and the remotes they are on.
    fn on_remotes(&self) -> (usize, Vec<&str>) {
        let mut remotes: Vec<&str> = Vec::new();
        let mut count = 0;
        for (remote, _, _) in self.chosen().iter().filter_map(|c| c.remote.as_ref()) {
            count += 1;
            if !remotes.contains(&remote.as_str()) {
                remotes.push(remote);
            }
        }
        (count, remotes)
    }

    /// The operation deleting the ticked branches.
    pub(crate) fn action(&self) -> Option<Action> {
        let chosen = self.chosen();
        if chosen.is_empty() {
            return None;
        }
        let (there, remotes) = self.on_remotes();
        let on_remote = self.on_remote && there > 0;
        let (ops, undo) = delete_many(&chosen, on_remote);
        Some(Action::UndoableOps {
            label: delete_label(chosen.len(), on_remote.then_some((there, &remotes[..]))),
            ops,
            refresh: Refresh::All,
            undo,
        })
    }
}

/// "3 branches", "1 branch".
fn branches(count: usize) -> String {
    if count == 1 {
        crate::i18n::text("repo-core-1269").to_owned()
    } else {
        crate::i18n::message("repo-core-1270", &[("count", (count).into())])
    }
}

/// What the button and the undo entry say: "Delete 3 branches", "Delete 3 branches (1 on
/// origin too)".
fn delete_label(count: usize, there: Option<(usize, &[&str])>) -> String {
    match there {
        Some((n, remotes)) => crate::i18n::message(
            "repo-core-1271",
            &[
                ("count", branches(count).to_string().into()),
                ("n", (n).into()),
                ("remotes", remotes.join(", ").to_string().into()),
            ],
        ),
        None => crate::i18n::message(
            "repo-core-1272",
            &[("count", branches(count).to_string().into())],
        ),
    }
}

pub(super) fn cleanup(ui: &mut Ui, dialog: &mut CleanupDialog) -> Outcome {
    let t = Tokens::get(ui.ctx());
    ui.set_width(460.0);
    ui.label(RichText::new(crate::i18n::text("repo-core-1273")).font(theme::semibold(15.0)));
    ui.add_space(4.0);
    ui.add(
        egui::Label::new(RichText::new(crate::i18n::text("repo-core-1274")).color(t.text_dim))
            .wrap(),
    );
    if let Some(e) = &dialog.error {
        ui.add_space(6.0);
        ui.add(egui::Label::new(RichText::new(e).color(t.danger)).wrap());
    } else if dialog.checking {
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.add(crate::spinner::Spinner::new());
            ui.label(RichText::new(crate::i18n::text("repo-core-1275")).color(t.text_dim));
        });
    }
    ui.add_space(10.0);
    if worktrees_offer(ui, dialog.worktrees, &t) {
        let act = CleanupAct::Worktrees;
        return Outcome::Done(Box::new(Action::Cleanup(act)));
    }
    if dialog.rows.is_empty() {
        if !dialog.checking {
            ui.label(crate::i18n::text("repo-core-1276"));
        }
        ui.add_space(14.0);
        return if cancel_button(ui) {
            Outcome::Cancel
        } else {
            Outcome::Open
        };
    }
    select_buttons(ui, &mut dialog.rows);
    checklist(ui, &mut dialog.rows, &t);
    let count = dialog.chosen().len();
    let (there, remotes) = dialog.on_remotes();
    let remotes: Vec<String> = remotes.into_iter().map(str::to_owned).collect();
    let remotes: Vec<&str> = remotes.iter().map(String::as_str).collect();
    ui.add_space(8.0);
    let also = if remotes.is_empty() {
        crate::i18n::text("repo-core-1277").to_owned()
    } else {
        crate::i18n::message(
            "repo-core-1278",
            &[
                ("there", branches(there).to_string().into()),
                ("remotes", remotes.join(", ").to_string().into()),
            ],
        )
    };
    ui.add_enabled(there > 0, egui::Checkbox::new(&mut dialog.on_remote, also))
        .on_hover_text(crate::i18n::text("repo-core-1279"))
        .on_disabled_hover_text(crate::i18n::text("repo-core-1280"));
    ui.add_space(14.0);
    let on_remote = (dialog.on_remote && there > 0).then_some((there, &remotes[..]));
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
            outcome = Outcome::Done(Box::new(action));
        }
        if cancel_button(ui) {
            outcome = Outcome::Cancel;
        }
    });
    outcome
}

/// "2 worktrees belong to merged branches" and the button that reviews them (the branches
/// checked out there are not offered above until their worktree is gone); whether clicked.
fn worktrees_offer(ui: &mut Ui, count: usize, t: &Tokens) -> bool {
    if count == 0 {
        return false;
    }
    let mut clicked = false;
    egui::Frame::new()
        .fill(t.accent.gamma_multiply(0.10))
        .corner_radius(t.radius_sm)
        .inner_margin(egui::Margin::symmetric(10, 6))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal_wrapped(|ui| {
                ui.label(crate::i18n::message(
                    "worktree-cleanup-offer",
                    &[("count", count.into())],
                ));
                clicked = ui
                    .button(crate::i18n::text("worktree-cleanup-offer-button"))
                    .clicked();
            });
        });
    ui.add_space(8.0);
    clicked
}

/// _All_ and _None_.
fn select_buttons(ui: &mut Ui, rows: &mut [(Candidate, bool)]) {
    ui.horizontal(|ui| {
        let ticked = rows.iter().filter(|(_, on)| *on).count();
        ui.label(crate::i18n::message(
            "repo-core-1281",
            &[
                ("ticked", format!("{}", ticked).into()),
                ("count", (rows.len()).into()),
            ],
        ));
        if ui
            .small_button(crate::i18n::text("repo-core-1282"))
            .clicked()
        {
            rows.iter_mut().for_each(|(_, on)| *on = true);
        }
        if ui
            .small_button(crate::i18n::text("repo-core-1283"))
            .clicked()
        {
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
                let mut why: Vec<String> = c.reasons.iter().map(|r| r.describe()).collect();
                if let Some((remote, name, _)) = &c.remote {
                    why.push(crate::i18n::message(
                        "repo-core-1285",
                        &[
                            ("remote", remote.to_string().into()),
                            ("name", name.to_string().into()),
                        ],
                    ));
                }
                let why = why.join(" · ");
                ui.horizontal(|ui| {
                    ui.checkbox(on, "");
                    ui.add(
                        egui::Label::new(RichText::new(&c.name).font(theme::medium(12.5)))
                            .truncate(),
                    )
                    .on_hover_text(format!("{}\n{why}", c.name));
                });
                ui.horizontal(|ui| {
                    ui.add_space(26.0);
                    ui.add(
                        egui::Label::new(RichText::new(&why).color(t.text_dim).small()).truncate(),
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
    fn only_merged_ones_start_ticked() {
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
        assert_eq!(ticked, ["done"]);
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
        assert_eq!(delete_label(1, None), "Delete 1 branch");
        assert_eq!(
            delete_label(12, Some((1, &["origin"]))),
            "Delete 12 branches (1 on origin too)"
        );
        assert_eq!(
            delete_label(3, Some((2, &["origin", "fork"]))),
            "Delete 3 branches (2 on origin, fork too)"
        );
    }
}
