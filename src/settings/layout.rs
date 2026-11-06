//! The settings page's building blocks: a titled section holding a card, and rows in that card
//! with the setting's name in a column of its own and its controls beside it. Every row says
//! what it can be found by, so a search shows only the rows (and sections) it matches.

use std::cell::Cell;

use egui::{Align, Layout, Margin, RichText, Stroke, Ui, vec2};

use crate::theme::{self, Tokens};

/// Width of the column of setting names, the same in every card so their controls line up.
pub(super) const LABEL_WIDTH: f32 = 190.0;
/// Room around a row's content.
const ROW_MARGIN: Margin = Margin::symmetric(16, 11);

/// What the page shows: every row of a page, or only those a search matches; or nothing at
/// all, only counting the rows a search matches.
pub(super) struct Filter {
    /// The search's words, lowercase; a row matches when it has every one. None: no search.
    words: Vec<String>,
    /// The page the sections belong to, which a search also looks at.
    pub page: &'static str,
    /// Only count the matching rows, drawing nothing.
    counting: bool,
    found: Cell<usize>,
}

impl Filter {
    pub fn new(query: &str, page: &'static str, counting: bool) -> Self {
        Self {
            words: query.split_whitespace().map(str::to_lowercase).collect(),
            page,
            counting,
            found: Cell::new(0),
        }
    }

    pub fn searching(&self) -> bool {
        !self.words.is_empty()
    }

    /// Rows matched so far.
    pub fn found(&self) -> usize {
        self.found.get()
    }

    fn matches(&self, parts: &[&str]) -> bool {
        let text = parts.join(" ").to_lowercase();
        self.words.iter().all(|w| text.contains(w.as_str()))
    }
}

/// A card of settings rows, separated by a line.
pub(super) struct Card<'a> {
    ui: &'a mut Ui,
    filter: &'a Filter,
    section: &'a str,
    /// Draws the rows; otherwise only counts those that match.
    draw: bool,
    found: usize,
    /// Spacing between widgets inside a row (the card itself has none between rows).
    spacing: egui::Vec2,
    margin: Margin,
    first: bool,
    t: Tokens,
}

/// A section of a page: its title (left out when it is the page's own), an optional line
/// about it, and a card of rows. During a search, only the rows it matches; nothing when none.
pub(super) fn section(
    ui: &mut Ui,
    filter: &Filter,
    title: &str,
    note: Option<&str>,
    mut rows: impl FnMut(&mut Card),
) {
    let t = Tokens::get(ui.ctx());
    let mut card = |ui: &mut Ui, draw: bool| {
        let spacing = ui.spacing().item_spacing;
        let mut card = Card {
            ui,
            filter,
            section: title,
            draw,
            found: 0,
            spacing,
            margin: ROW_MARGIN,
            first: true,
            t,
        };
        rows(&mut card);
        card.found
    };
    if filter.searching() {
        let found = card(ui, false);
        filter.found.set(filter.found() + found);
        if found == 0 {
            return;
        }
    }
    if filter.counting {
        return;
    }
    if title != filter.page {
        ui.add_space(18.0);
        ui.label(
            RichText::new(title)
                .font(theme::semibold(14.0))
                .color(t.text),
        );
    } else {
        ui.add_space(6.0);
    }
    if let Some(note) = note {
        ui.add_space(2.0);
        ui.label(RichText::new(note).color(t.text_faint));
    }
    ui.add_space(8.0);
    egui::Frame::new()
        .fill(t.card)
        .stroke(Stroke::new(1.0, t.separator))
        .corner_radius(t.radius_lg)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = 0.0;
            card(ui, true);
        });
}

impl Card<'_> {
    pub fn ctx(&self) -> egui::Context {
        self.ui.ctx().clone()
    }

    /// Less room above and below the rows that follow, for a long list of short rows.
    pub fn compact(&mut self) {
        self.margin = Margin::symmetric(16, 7);
    }

    /// A row: `label` in the names' column, `add` putting the controls beside it. A search
    /// finds it by its label, its section and page, or `keywords`.
    pub fn row<R>(&mut self, label: &str, keywords: &str, add: impl FnOnce(&mut Ui) -> R) {
        self.row_with(label, None, keywords, add);
    }

    /// A row with a short line under the setting's name, saying what it does.
    pub fn row_hint<R>(
        &mut self,
        label: &str,
        hint: &str,
        keywords: &str,
        add: impl FnOnce(&mut Ui) -> R,
    ) {
        self.row_with(label, Some(hint), keywords, add);
    }

    fn row_with<R>(
        &mut self,
        label: &str,
        hint: Option<&str>,
        keywords: &str,
        add: impl FnOnce(&mut Ui) -> R,
    ) {
        let t = self.t;
        let found_by = [label, hint.unwrap_or_default(), keywords];
        self.full_with(&found_by, |ui| {
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
                ui.vertical(add);
            });
        });
    }

    /// A row of the card's whole width, without a name, found by `keywords`.
    pub fn full<R>(&mut self, keywords: &str, add: impl FnOnce(&mut Ui) -> R) {
        self.full_with(&[keywords], add);
    }

    fn full_with<R>(&mut self, found_by: &[&str], add: impl FnOnce(&mut Ui) -> R) {
        if self.filter.searching() {
            let mut parts = vec![self.filter.page, self.section];
            parts.extend(found_by);
            if !self.filter.matches(&parts) {
                return;
            }
        }
        self.found += 1;
        if !self.draw {
            return;
        }
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
            .inner_margin(self.margin)
            .show(self.ui, |ui| {
                ui.set_width(ui.available_width());
                ui.spacing_mut().item_spacing = spacing;
                add(ui);
            });
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
