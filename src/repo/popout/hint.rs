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
    let text = crate::i18n::message(
        "repo-core-3041",
        &[("noun", format!("{}", capitalized(panel.noun())).into())],
    );
    let label = ui
        .add(
            egui::Label::new(RichText::new(text).size(12.0).color(t.text_dim))
                .sense(Sense::click()),
        )
        .on_hover_text("Show that window");
    crate::a11y::a11y(&label, egui::WidgetType::Button, || {
        crate::i18n::message(
            "repo-core-3043",
            &[("noun", format!("{}", panel.noun()).into())],
        )
    });
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

#[cfg(test)]
mod interaction_tests {
    use super::*;
    use egui_kittest::{Harness, kittest::Queryable};

    #[test]
    fn painted_action_has_button_role_keyboard_activation_and_focus_ring() {
        let mut h = Harness::new_ui_state(
            move |ui, state: &mut (bool, Vec<Action>)| {
                if !std::mem::replace(&mut state.0, true) {
                    crate::theme::install_fonts(ui.ctx());
                    egui_extras::install_image_loaders(ui.ctx());
                    return;
                }
                chip(ui, &Tokens::get(ui.ctx()), Panel::Diff, &mut state.1);
                crate::a11y::end_frame(ui.ctx());
            },
            (false, Vec::new()),
        );
        h.run_steps(2);
        let button = h.get_by_role_and_label(egui::accesskit::Role::Button, "Show the diff window");
        assert!(!button.is_focused());
        h.key_press(egui::Key::Tab);
        h.run();
        assert!(
            h.get_by_role_and_label(egui::accesskit::Role::Button, "Show the diff window")
                .is_focused()
        );
        let accent = Tokens::get(&h.ctx).accent;
        assert!(
            h.output().shapes.iter().any(|shape| matches!(
                &shape.shape, egui::Shape::Rect(rect)
                    if rect.stroke == egui::Stroke::new(2.0, accent)
            )),
            "keyboard focus must produce the painted accent ring"
        );
        h.key_press(egui::Key::Enter);
        h.run();
        assert!(matches!(
            h.state().1.as_slice(),
            [Action::Popout(Request::Focus(Panel::Diff))]
        ));
    }
}
