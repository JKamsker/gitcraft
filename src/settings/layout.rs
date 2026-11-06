//! The settings page's building blocks: a titled section holding a card, and rows in that card
//! with the setting's name in a column of its own and its controls beside it.

use egui::{Align, Layout, Margin, RichText, Stroke, Ui, vec2};

use crate::theme::{self, Tokens};

/// Width of the column of setting names, the same in every card so their controls line up.
const LABEL_WIDTH: f32 = 190.0;
/// Room around a row's content.
const ROW_MARGIN: Margin = Margin::symmetric(16, 11);

/// A card of settings rows, separated by a line.
pub(super) struct Card<'a> {
    ui: &'a mut Ui,
    /// Spacing between widgets inside a row (the card itself has none between rows).
    spacing: egui::Vec2,
    first: bool,
    t: Tokens,
}

/// A section of the page: its title, an optional line about it, and a card of rows.
pub(super) fn section(ui: &mut Ui, title: &str, note: Option<&str>, rows: impl FnOnce(&mut Card)) {
    let t = Tokens::get(ui.ctx());
    ui.add_space(26.0);
    ui.label(
        RichText::new(title)
            .font(theme::semibold(14.0))
            .color(t.text),
    );
    if let Some(note) = note {
        ui.add_space(1.0);
        ui.label(RichText::new(note).color(t.text_faint));
    }
    ui.add_space(8.0);
    egui::Frame::new()
        .fill(t.card)
        .stroke(Stroke::new(1.0, t.separator))
        .corner_radius(t.radius_lg)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            let spacing = ui.spacing().item_spacing;
            ui.spacing_mut().item_spacing.y = 0.0;
            rows(&mut Card {
                ui,
                spacing,
                first: true,
                t,
            });
        });
}

impl Card<'_> {
    pub fn ctx(&self) -> &egui::Context {
        self.ui.ctx()
    }

    /// A row: `label` in the names' column, `add` putting the controls beside it.
    pub fn row<R>(&mut self, label: &str, add: impl FnOnce(&mut Ui) -> R) -> R {
        self.row_with(label, None, add)
    }

    /// A row with a short line under the setting's name, saying what it does.
    pub fn row_hint<R>(&mut self, label: &str, hint: &str, add: impl FnOnce(&mut Ui) -> R) -> R {
        self.row_with(label, Some(hint), add)
    }

    fn row_with<R>(
        &mut self,
        label: &str,
        hint: Option<&str>,
        add: impl FnOnce(&mut Ui) -> R,
    ) -> R {
        let t = self.t;
        self.full(|ui| {
            ui.horizontal_top(|ui| {
                ui.allocate_ui_with_layout(
                    vec2(LABEL_WIDTH, 0.0),
                    Layout::top_down(Align::Min),
                    |ui| {
                        ui.set_width(LABEL_WIDTH);
                        // Level with the text of a control of the usual height.
                        ui.add_space(4.0);
                        ui.label(RichText::new(label).color(t.text));
                        if let Some(hint) = hint {
                            ui.spacing_mut().item_spacing.y = 2.0;
                            ui.label(RichText::new(hint).color(t.text_faint).small());
                        }
                    },
                );
                ui.vertical(add).inner
            })
            .inner
        })
    }

    /// A row of the card's whole width, without a name.
    pub fn full<R>(&mut self, add: impl FnOnce(&mut Ui) -> R) -> R {
        if !self.first {
            let y = self.ui.cursor().top();
            let x = self.ui.max_rect().x_range();
            self.ui
                .painter()
                .hline(x, y, Stroke::new(1.0, self.t.separator));
        }
        self.first = false;
        let spacing = self.spacing;
        egui::Frame::new()
            .inner_margin(ROW_MARGIN)
            .show(self.ui, |ui| {
                ui.set_width(ui.available_width());
                ui.spacing_mut().item_spacing = spacing;
                add(ui)
            })
            .inner
    }
}

/// Faint text beside or under a control: what a value means, or what was found.
pub(super) fn note(ui: &mut Ui, text: impl Into<String>) -> egui::Response {
    let t = Tokens::get(ui.ctx());
    ui.label(RichText::new(text).color(t.text_faint).small())
}

/// A slider in the page's style: wider, round handle, filled up to it.
pub(super) fn slider<'a>(slider: egui::Slider<'a>) -> egui::Slider<'a> {
    slider
        .handle_shape(egui::style::HandleShape::Circle)
        .trailing_fill(true)
}

/// A one-line text field as tall as the other controls.
pub(super) fn field(text: &mut String) -> egui::TextEdit<'_> {
    egui::TextEdit::singleline(text)
        .margin(vec2(6.0, 4.0))
        .min_size(vec2(0.0, 24.0))
        .vertical_align(Align::Center)
}
