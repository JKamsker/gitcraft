//! Picking working-tree files to stash (Ctrl+click), the bar that stashes those picked, and
//! the stash entries of a file's menu.

use std::collections::BTreeSet;

use egui::{RichText, Ui};

use crate::repo::{Action, Dialog, RepoTab};
use crate::theme::{self, Tokens};

impl RepoTab {
    /// Over the changes, while files are picked: how many, Stash… and Clear.
    pub(super) fn stash_picks_bar(&self, ui: &mut Ui, acts: &mut Vec<Action>) {
        let picked = &self.stash_picks.picked;
        if picked.is_empty() {
            return;
        }
        let t = Tokens::get(ui.ctx());
        let idle = self.busy_op.is_none();
        egui::Frame::new()
            .fill(t.accent.gamma_multiply(0.12))
            .inner_margin(egui::Margin::symmetric(12, 6))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(picked_label(picked.len()))
                            .color(t.text)
                            .font(theme::medium(12.0)),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("Clear").clicked() {
                            acts.push(Action::ClearStashPicks);
                        }
                        let stash = ui
                            .add_enabled(idle, egui::Button::new("Stash…"))
                            .on_hover_text("Stash only the files picked")
                            .on_disabled_hover_text("Wait for the current operation to finish");
                        if stash.clicked() {
                            acts.push(Action::Dialog(Dialog::stash_files(
                                picked.iter().cloned().collect(),
                            )));
                        }
                    });
                });
            });
    }
}

/// `3 files picked to stash`.
fn picked_label(n: usize) -> String {
    format!("{n} file{} picked to stash", if n == 1 { "" } else { "s" })
}

/// The stash entries of the menu of the working-tree file `path`.
/// The mark of a file picked to stash: a bar in the accent colour along its row's left edge,
/// apart from the highlight of the file whose diff is open.
pub(super) fn mark_picked(ui: &Ui, row: egui::Rect) {
    let t = Tokens::get(ui.ctx());
    let bar = egui::Rect::from_min_size(row.min, egui::vec2(3.0, row.height()));
    ui.painter().rect_filled(bar, 0.0, t.accent);
}

/// `problem`: why git can't stash this file by its name, if it can't (its entries are then
/// greyed out, saying why).
pub(super) fn stash_menu_items(
    ui: &mut Ui,
    path: &str,
    problem: Option<&str>,
    picked: &BTreeSet<String>,
    acts: &mut Vec<Action>,
) {
    if let Some(why) = problem {
        let why = crate::repo::dialogs::sentence(why);
        ui.add_enabled(false, egui::Button::new("Stash this file…"))
            .on_disabled_hover_text(why);
        return;
    }
    let is_picked = picked.contains(path);
    if is_picked && picked.len() > 1 {
        if ui
            .button(format!("Stash the {} picked files…", picked.len()))
            .clicked()
        {
            acts.push(Action::Dialog(Dialog::stash_files(
                picked.iter().cloned().collect(),
            )));
        }
    } else if ui.button("Stash this file…").clicked() {
        acts.push(Action::Dialog(Dialog::stash_files(vec![path.to_owned()])));
    }
    let pick = if is_picked {
        "Unpick (don't stash it)".to_owned()
    } else {
        format!("Pick to stash ({}+click)", crate::manual::command())
    };
    if ui.button(pick).clicked() {
        acts.push(Action::PickToStash(path.to_owned()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_bar_counts_the_files() {
        assert_eq!(picked_label(1), "1 file picked to stash");
        assert_eq!(picked_label(12), "12 files picked to stash");
    }
}
