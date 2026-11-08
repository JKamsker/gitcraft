//! Hiding and soloing branches from the sidebar: the eye on a branch's row, the entries of its
//! menu, and how many branches the graph shows.

use egui::{Rect, Response, RichText, Ui, pos2, vec2};

use crate::icons;
use crate::menu_item::MenuItem;
use crate::repo::branch_visibility::Change;
use crate::repo::{Action, RepoTab};
use crate::theme::Tokens;

impl RepoTab {
    /// Whether the graph shows the branch `full` (`refs/heads/main`); the checked-out one it
    /// always does.
    pub(super) fn branch_shown(&self, full: &str, is_head: bool) -> bool {
        is_head || self.visibility.shows(full)
    }

    /// Hide or show, solo or unsolo the branch `full`; show them all while some are left out.
    pub(super) fn visibility_items(
        &self,
        ui: &mut Ui,
        acts: &mut Vec<Action>,
        full: &str,
        is_head: bool,
    ) {
        let hide = if self.branch_shown(full, is_head) {
            crate::i18n::text("repo-core-3549")
        } else {
            crate::i18n::text("repo-core-3550")
        };
        if MenuItem::new(hide)
            .icon("eye-off")
            .refused(is_head.then_some(crate::i18n::text("repo-core-3552")))
            .show(ui)
            .clicked()
        {
            acts.push(Action::BranchVisibility(Change::Toggle(full.to_owned())));
        }
        let solo = if self.visibility.solo.contains(full) {
            crate::i18n::text("repo-core-3553")
        } else {
            crate::i18n::text("repo-core-3554")
        };
        if ui
            .button(solo)
            .on_hover_text(crate::i18n::text("repo-core-3555"))
            .clicked()
        {
            acts.push(Action::BranchVisibility(Change::Solo(full.to_owned())));
        }
        if self.visibility.is_filtering()
            && ui.button(crate::i18n::text("repo-core-3556")).clicked()
        {
            acts.push(Action::BranchVisibility(Change::ShowAll));
        }
    }

    /// "Viewing 12 of 55" and a way to show them all, while the graph leaves branches out.
    pub(super) fn visibility_counter(&self, ui: &mut Ui, acts: &mut Vec<Action>) {
        if !self.visibility.is_filtering() {
            return;
        }
        let t = Tokens::get(ui.ctx());
        let (shown, all) = self.visibility.counts(&self.refs);
        ui.horizontal(|ui| {
            ui.add_space(12.0);
            icons::inline(ui, "eye", 14.0, t.text_faint);
            let what = if self.visibility.is_soloing() {
                crate::i18n::text("repo-core-3557")
            } else {
                crate::i18n::text("repo-core-3558")
            };
            ui.label(
                RichText::new(crate::i18n::message(
                    "repo-core-3559",
                    &[
                        ("shown", (shown).into()),
                        ("all", format!("{}", all).into()),
                    ],
                ))
                .color(t.text_dim),
            )
            .on_hover_text(crate::i18n::message(
                "repo-core-3560",
                &[("what", what.to_string().into())],
            ));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add_space(10.0);
                if ui.link(crate::i18n::text("repo-core-3561")).clicked() {
                    acts.push(Action::BranchVisibility(Change::ShowAll));
                }
            });
        });
        ui.add_space(6.0);
    }
}

/// The eye at the left of the row `row` of the branch `full`: crossed out while the graph leaves
/// the branch out, else only while the row is hovered. Clicking it hides or shows the branch.
pub(super) fn eye(ui: &Ui, row: &Response, full: &str, shown: bool) -> Option<Action> {
    let rect = Rect::from_center_size(
        pos2(row.rect.left() + 14.0, row.rect.center().y),
        vec2(18.0, 18.0),
    );
    // A Tab stop only while it shows to the keyboard (the row or the eye has it) or the branch
    // is hidden, so a branch row isn't two stops.
    let id = row.id.with("eye");
    let keyboard = crate::a11y::keyboard_on(row) || ui.memory(|m| m.has_focus(id));
    let resp = ui.interact(rect, id, crate::a11y::list_row_sense(keyboard || !shown));
    let tip = if shown {
        crate::i18n::text("repo-core-3549")
    } else {
        crate::i18n::text("repo-core-3550")
    };
    crate::repo::widgets::a11y(&resp, egui::WidgetType::Button, || {
        crate::a11y::labels::join([tip, crate::a11y::labels::ref_name(full)])
    });
    if shown && !keyboard && !row.contains_pointer() && !resp.contains_pointer() {
        return None;
    }
    let t = Tokens::get(ui.ctx());
    let tint = if resp.hovered() { t.text } else { t.text_faint };
    icons::paint(ui, rect, if shown { "eye" } else { "eye-off" }, 13.0, tint);
    resp.on_hover_text(tip)
        .clicked()
        .then(|| Action::BranchVisibility(Change::Toggle(full.to_owned())))
}
