//! The settings page's building blocks: a titled section holding a card, and rows in that card
//! with the setting's name in a column of its own and its controls beside it. Every row says
//! what it can be found by, so a search shows only the rows (and sections) it matches.

use std::cell::Cell;

use egui::{Align, Layout, Margin, RichText, Stroke, Ui, vec2};

use super::Page;
use super::preview::{self, Preview};
use crate::theme::{self, Tokens};

/// Width of the column of setting names, the same in every card so their controls line up.
const LABEL_WIDTH: f32 = 190.0;
/// Rows narrower than this put the setting's name above its controls.
const STACK_BELOW: f32 = 440.0;
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
    /// The page drawn, or `None` in the search results: where a row's preview shows.
    at: Option<Page>,
}

impl Filter {
    pub fn new(query: &str, page: &'static str, counting: bool) -> Self {
        Self {
            words: query.split_whitespace().map(str::to_lowercase).collect(),
            page,
            counting,
            found: Cell::new(0),
            at: None,
        }
    }

    /// Drawn as the page `page` (not among search results).
    pub fn on_page(mut self, page: Page) -> Self {
        self.at = Some(page);
        self
    }

    /// Whether `text` has one of the search's words: what of a long list to show while
    /// searching (only those, when any has one).
    pub fn names(&self, text: &str) -> bool {
        let text = text.to_lowercase();
        self.words.iter().any(|w| text.contains(w.as_str()))
    }

    pub fn searching(&self) -> bool {
        !self.words.is_empty()
    }

    /// Rows matched so far.
    pub fn found(&self) -> usize {
        self.found.get()
    }

    fn matches(&self, parts: &[&str]) -> bool {
        let text = parts
            .iter()
            .flat_map(|raw| [*raw, super::labels::display(raw)])
            .collect::<Vec<_>>()
            .join(" ")
            .to_lowercase();
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
    /// What the next row previews.
    next_preview: Option<Preview>,
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
            next_preview: None,
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
            RichText::new(super::labels::display(title))
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

    /// The next row shows `preview` beside the page while it is pointed at.
    pub fn preview(&mut self, preview: Preview) {
        self.next_preview = Some(preview);
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
            let name = |ui: &mut Ui| {
                ui.label(RichText::new(super::labels::display(label)).color(t.text));
                if let Some(hint) = hint {
                    ui.spacing_mut().item_spacing.y = 2.0;
                    ui.label(RichText::new(hint).color(t.text_faint).small());
                }
            };
            // Too narrow for the names' column (a small window, a large zoom): the name goes
            // above its controls.
            if ui.available_width() < STACK_BELOW {
                ui.vertical(|ui| {
                    ui.scope(name);
                    ui.add_space(4.0);
                    add(ui);
                });
                return;
            }
            let width = label_width(ui);
            ui.horizontal_top(|ui| {
                ui.allocate_ui_with_layout(vec2(width, 0.0), Layout::top_down(Align::Min), |ui| {
                    ui.set_width(width);
                    // Level with the text of a control of the usual height.
                    ui.add_space(4.0);
                    name(ui);
                });
                ui.vertical(add);
            });
        });
    }

    /// A row of the card's whole width, without a name, found by `keywords`.
    pub fn full<R>(&mut self, keywords: &str, add: impl FnOnce(&mut Ui) -> R) {
        self.full_with(&[keywords], add);
    }

    fn full_with<R>(&mut self, found_by: &[&str], add: impl FnOnce(&mut Ui) -> R) {
        let preview = self.next_preview.take();
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
        let row = egui::Frame::new()
            .inner_margin(self.margin)
            .show(self.ui, |ui| {
                ui.set_width(ui.available_width());
                ui.spacing_mut().item_spacing = spacing;
                add(ui);
            })
            .response
            .rect;
        if let Some(preview) = preview {
            let at = self.filter.at;
            let ctx = self.ui.ctx().clone();
            let key = egui::Id::new(("settings-row", self.section, found_by.first().copied()));
            if self.ui.rect_contains_pointer(row) {
                preview::point(&ctx, at, preview, key);
            }
            if at.is_none() {
                preview::found(&ctx, preview);
            }
            // The row whose preview shows is marked along its left edge.
            if preview::is_marked(&ctx, at, preview, key) {
                let bar = egui::Rect::from_min_size(
                    row.left_top() + vec2(0.0, 8.0),
                    vec2(3.0, (row.height() - 16.0).max(8.0)),
                );
                self.ui.painter().rect_filled(bar, 1.5, self.t.accent);
            }
        }
    }
}

/// Width of the names' column in a row of `ui`: less in a narrow card.
pub(super) fn label_width(ui: &Ui) -> f32 {
    LABEL_WIDTH.min(ui.available_width() * 0.36)
}

/// `width`, or less when `ui` has less room.
pub(super) fn fit(ui: &Ui, width: f32) -> f32 {
    width.min(ui.available_width()).max(60.0)
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

#[cfg(test)]
mod localization_tests {
    use super::*;
    use egui_kittest::{Harness, kittest::Queryable};

    #[test]
    fn actual_row_translates_and_keeps_control_identity_and_both_search_languages() {
        crate::i18n::set_language("de");
        let german = Filter::new("Verhalten Worktrees", "General", true);
        let english = Filter::new("Behaviour Worktrees", "General", true);
        assert!(german.matches(&["General", "Behaviour", "Worktrees"]));
        assert!(english.matches(&["General", "Behaviour", "Worktrees"]));
        assert_eq!(super::super::Page::General.title(), "General");
        assert_eq!(
            super::super::labels::display("user-owned opaque name"),
            "user-owned opaque name"
        );
        let fonts = Cell::new(false);
        let mut h = Harness::new_ui_state(
            move |ui, state: &mut (bool, Option<egui::Id>)| {
                if !fonts.replace(true) {
                    crate::theme::install_fonts(ui.ctx());
                    return;
                }
                let filter = Filter::new("", "General", false).on_page(Page::General);
                section(ui, &filter, "Behaviour", None, |card| {
                    card.row("Worktrees", "open same tab switch", |ui| {
                        let response = ui.checkbox(
                            &mut state.0,
                            crate::i18n::text("interaction-settings-same-tab"),
                        );
                        state.1 = Some(response.id);
                    });
                });
            },
            (false, None),
        );
        h.run();
        h.get_by_label("Verhalten");
        h.get_by_label("Worktrees");
        h.get_by_label("Im selben Tab öffnen").click();
        h.run();
        assert!(h.state().0);
        let german_id = h.state().1;
        crate::i18n::set_language("en-US");
        h.step();
        h.get_by_label("Behaviour");
        h.get_by_label("Open in the same tab");
        assert_eq!(h.state().1, german_id);
    }
}
