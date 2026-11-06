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
                        let source = into_source(found.map(Arc::new));
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
                |tab, found, _| tab.search.suggest.paths = into_source(found.map(Arc::new)),
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
    let width = ui.available_width();
    let (rect, resp) = ui.allocate_exact_size(vec2(width, ROW_H), Sense::click());
    if chosen || resp.hovered() {
        let fill = if chosen { t.row_selected } else { t.hover };
        ui.painter().rect_filled(rect, t.radius_sm, fill);
    }
    if chosen {
        ui.scroll_to_rect(rect, None);
    }
    let painter = ui.painter_at(rect.shrink2(vec2(8.0, 0.0)));
    let font = theme::medium(13.0);
    let label_rect = painter.text(
        rect.left_center() + vec2(8.0, 0.0),
        Align2::LEFT_CENTER,
        label,
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
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
        .clicked()
}
