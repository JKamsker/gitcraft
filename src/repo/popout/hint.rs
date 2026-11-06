//! The main window's reminder of the views in windows of their own: one chip each, at the foot
//! of the window, that raises its window, with a button that brings it back.

use egui::{Align2, RichText, Sense, vec2};

use super::{Panel, Request};
use crate::icons;
use crate::repo::{Action, RepoTab};
use crate::theme::Tokens;

/// How far above the window's bottom edge the chips sit, clear of the panels' footers.
const LIFT: f32 = 40.0;

impl RepoTab {
    /// The chips of this tab's popped-out views, over the bottom of the main window.
    pub(super) fn popped_hint(&self, ctx: &egui::Context, acts: &mut Vec<Action>) {
        let t = Tokens::get(ctx);
        egui::Area::new(egui::Id::new(("popout-hint", &self.path)))
            .anchor(Align2::CENTER_BOTTOM, vec2(0.0, -LIFT))
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                egui::Frame::new()
                    .fill(t.card)
                    .stroke(egui::Stroke::new(1.0, t.separator))
                    .corner_radius(t.radius)
                    .inner_margin(egui::Margin::symmetric(8, 4))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            for panel in self.popouts.popped() {
                                chip(ui, &t, panel, acts);
                            }
                        });
                    });
            });
    }
}

/// One view's chip: its name (raises its window) and Bring back.
fn chip(ui: &mut egui::Ui, t: &Tokens, panel: Panel, acts: &mut Vec<Action>) {
    icons::inline(ui, "picture-in-picture-2", 13.0, t.text_faint);
    let text = format!("{} in its own window", capitalized(panel.noun()));
    let label = ui
        .add(
            egui::Label::new(RichText::new(text).size(12.0).color(t.text_dim))
                .sense(Sense::click()),
        )
        .on_hover_text("Show that window");
    if label.clicked() {
        acts.push(Action::Popout(Request::Focus(panel)));
    }
    super::button(ui, panel, true, acts);
    ui.add_space(6.0);
}

/// `noun` with its first letter capitalised ("CI view" stays as it is).
fn capitalized(noun: &str) -> String {
    let mut chars = noun.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}
