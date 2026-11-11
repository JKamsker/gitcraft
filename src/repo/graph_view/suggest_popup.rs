//! The list of suggestions under the graph's search field, and reading what they come from
//! (authors, committers, files) in the background.

use std::sync::Arc;

use egui::{Align2, Rect, Sense, Stroke, Ui, vec2};

use super::suggest::{Listing, PathIndex, Source, SourceKind};
use crate::git;
use crate::repo::RepoTab;
use crate::theme::{self, Tokens};

const ROW_H: f32 = 24.0;

impl RepoTab {
    /// Draws `listing` under the field at `field`; the suggestion clicked, if any.
    pub(super) fn suggestions_popup(
        &mut self,
        ui: &Ui,
        field: Rect,
        listing: &Listing,
    ) -> Option<usize> {
        self.search.suggest.popup = None;
        if listing.items.is_empty() && listing.status.is_none() {
            return None;
        }
        let t = Tokens::get(ui.ctx());
        let width = field.width().clamp(320.0, 560.0);
        let mut clicked = None;
        let shown = egui::Area::new(ui.id().with("search-suggestions"))
            .order(egui::Order::Foreground)
            .fixed_pos(field.left_bottom() + vec2(0.0, 4.0))
            .show(ui.ctx(), |ui| {
                egui::Frame::new()
                    .fill(t.card)
                    .stroke(Stroke::new(1.0, t.field_border))
                    .corner_radius(t.radius)
                    .inner_margin(egui::Margin::same(4))
                    .shadow(ui.ctx().global_style().visuals.popup_shadow)
                    .show(ui, |ui| {
                        ui.set_width(width);
                        ui.spacing_mut().item_spacing.y = 0.0;
                        if let Some(status) = &listing.status
                            && listing.items.is_empty()
                        {
                            ui.label(egui::RichText::new(status).color(t.text_faint).small());
                        }
                        for (i, item) in listing.items.iter().enumerate() {
                            let chosen = self.search.suggest.chosen == Some(i);
                            if suggestion_row(ui, &t, &item.label, &item.detail, chosen) {
                                clicked = Some(i);
                            }
                        }
                    });
            });
        self.search.suggest.popup = Some(shown.response.rect);
        clicked
    }

    /// Reads the people or files suggestions are made from, once per opening of the search.
    pub(super) fn load_suggestions(&mut self, kind: SourceKind, ctx: &egui::Context) {
        let repo = self.path.clone();
        let s = &mut self.search.suggest;
        s.version += 1;
        let generation = s.generation;
        match kind {
            SourceKind::Authors => s.authors = Source::Loading,
            SourceKind::Committers => s.committers = Source::Loading,
            SourceKind::Paths => s.paths = Source::Loading,
        }
        match kind {
            SourceKind::Authors | SourceKind::Committers => {
                let committers = kind == SourceKind::Committers;
                self.then(
                    ctx,
                    move || git::search_query::people(&repo, committers),
                    move |tab, found, _| {
                        if tab.search.suggest.generation != generation {
                            return;
                        }
                        let source = into_source(found.map(Arc::new));
                        tab.search.suggest.version += 1;
                        if committers {
                            tab.search.suggest.committers = source;
                        } else {
                            tab.search.suggest.authors = source;
                        }
                    },
                );
            }
            SourceKind::Paths => self.then(
                ctx,
                move || match git::tree_files(&repo, "HEAD") {
                    Ok(files) => Ok(PathIndex::new(files)),
                    // Nothing committed yet: no files to offer.
                    Err(e) if e.contains("HEAD") => Ok(PathIndex::new(Vec::new())),
                    Err(e) => Err(e),
                },
                move |tab, found, _| {
                    if tab.search.suggest.generation != generation {
                        return;
                    }
                    tab.search.suggest.paths = into_source(found.map(Arc::new));
                    tab.search.suggest.version += 1;
                },
            ),
        }
    }
}

fn into_source<T>(found: Result<Arc<T>, String>) -> Source<T> {
    match found {
        Ok(items) => Source::Ready(items),
        Err(e) => Source::Failed(e),
    }
}

/// One suggestion: its label, then its detail, faded. True when clicked.
fn suggestion_row(ui: &mut Ui, t: &Tokens, label: &str, detail: &str, chosen: bool) -> bool {
    let full_label = label;
    let width = ui.available_width();
    let (_, rect) = ui.allocate_space(vec2(width, ROW_H));
    let resp = ui.interact(
        rect,
        ui.id().with(("search-suggestion", full_label, detail)),
        Sense::click(),
    );
    crate::a11y::a11y_selected(&resp, || format!("{full_label}, {detail}"), chosen);
    if chosen || resp.hovered() {
        let fill = if chosen { t.row_selected } else { t.hover };
        ui.painter().rect_filled(rect, t.radius_sm, fill);
    }
    if chosen {
        ui.scroll_to_rect(rect, None);
    }
    let painter = ui.painter_at(rect.shrink2(vec2(8.0, 0.0)));
    let font = theme::medium(13.0);
    // The label keeps both ends (a path's file name), the detail what fits after it.
    let room = rect.width() - 16.0;
    let label = elide_middle(ui, label, &font, room * 0.75);
    let label_rect = painter.text(
        rect.left_center() + vec2(8.0, 0.0),
        Align2::LEFT_CENTER,
        &label,
        font,
        t.text,
    );
    painter.text(
        label_rect.right_center() + vec2(10.0, 0.0),
        Align2::LEFT_CENTER,
        detail,
        egui::FontId::proportional(12.0),
        t.text_dim,
    );
    let resp =
        if label_rect.width() + 10.0 + text_width(ui, detail, &egui::FontId::proportional(12.0))
            > room
        {
            resp.on_hover_text(format!("{full}\n{detail}", full = full_label))
        } else {
            resp
        };
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
        .clicked()
}

fn text_width(ui: &Ui, text: &str, font: &egui::FontId) -> f32 {
    ui.fonts_mut(|f| {
        f.layout_no_wrap(text.to_owned(), font.clone(), egui::Color32::WHITE)
            .size()
            .x
    })
}

/// `text`, its middle replaced by “…” until it is at most `max` wide.
fn elide_middle(ui: &Ui, text: &str, font: &egui::FontId, max: f32) -> String {
    if text_width(ui, text, font) <= max {
        return text.to_owned();
    }
    let chars: Vec<char> = text.chars().collect();
    let cut = |keep: usize| -> String {
        let head = keep / 2;
        let tail = keep - head;
        let mut out: String = chars[..head].iter().collect();
        out.push('…');
        out.extend(&chars[chars.len() - tail..]);
        out
    };
    // The most characters kept that still fit.
    let (mut lo, mut hi) = (0, chars.len());
    while lo < hi {
        let mid = (lo + hi).div_ceil(2);
        if text_width(ui, &cut(mid), font) <= max {
            lo = mid;
        } else {
            hi = mid - 1;
        }
    }
    cut(lo)
}

#[cfg(test)]
mod accessibility_tests {
    use super::*;
    use egui_kittest::{
        Harness,
        kittest::{NodeT, Queryable},
    };
    #[test]
    fn suggestion_retains_full_path_and_selection_and_keyboard_activation() {
        let path = "src/a-very-long-unicode-directory/日本語/file.rs";
        let fonts = std::cell::Cell::new(false);
        let mut h = Harness::builder().with_size([220.0, 120.0]).build_ui_state(
            move |ui, count: &mut usize| {
                if !fonts.replace(true) {
                    crate::theme::install_fonts(ui.ctx());
                    return;
                }
                let t = Tokens::get(ui.ctx());
                if suggestion_row(ui, &t, path, "12 matching commits", true) {
                    *count += 1;
                }
                crate::a11y::end_frame(ui.ctx());
            },
            0usize,
        );
        h.run();
        let label = format!("{path}, 12 matching commits");
        let node = h.get_by_label(&label);
        assert_eq!(
            node.accesskit_node().toggled(),
            Some(egui::accesskit::Toggled::True)
        );
        h.key_press(egui::Key::Tab);
        h.run();
        assert!(h.get_by_label(&label).is_focused());
        h.key_press(egui::Key::Enter);
        h.run();
        assert_eq!(*h.state(), 1);
    }
}
