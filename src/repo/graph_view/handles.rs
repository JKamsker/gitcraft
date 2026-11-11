//! The drag handles on the column borders of the graph's header.

use egui::{Rect, Sense, Stroke, Ui, pos2, vec2};

use super::{HANDLE_COLOR, HANDLE_WIDTH};
use crate::repo::RepoTab;
use crate::repo::columns::{Bar, Cols, DescriptionSpace};
use crate::settings::Settings;
use crate::theme::Tokens;

/// One pointer gesture, including its original grab offset and saved preference.
#[derive(Clone, Copy)]
struct DragStart {
    prefs: Cols,
    shown: Cols,
    description: DescriptionSpace,
    available: f32,
    pointer: f32,
    preference: f32,
    auto_refs_width: bool,
}

/// Persist only the intended column; temporary viewport compression is never saved.
fn save_drag(next: &mut Settings, bar: Bar, _prefs: &Cols, widths: &Cols) {
    next.last_resized = Some(bar);
    if bar == Bar::Refs {
        next.auto_refs_width = false;
    }
    *preference_mut(next, bar) = widths.width(bar);

    next.column_push = widths.push;
}

fn preference_mut(settings: &mut Settings, bar: Bar) -> &mut f32 {
    match bar {
        Bar::Refs => &mut settings.refs_width,
        Bar::Graph => &mut settings.graph_width,
        Bar::Hash => &mut settings.hash_width,
        Bar::Author => &mut settings.author_width,
        Bar::When => &mut settings.when_width,
        Bar::Date => &mut settings.date_width,
    }
}
fn preference(settings: &Settings, bar: Bar) -> f32 {
    match bar {
        Bar::Refs => settings.refs_width,
        Bar::Graph => settings.graph_width,
        Bar::Hash => settings.hash_width,
        Bar::Author => settings.author_width,
        Bar::When => settings.when_width,
        Bar::Date => settings.date_width,
    }
}

impl RepoTab {
    /// Drag handles on the column borders of the header. A drag changes the preferred width
    /// of the handle's column in the settings (the app saves them from there); `columns`
    /// works out what that squeezes.
    ///
    /// Call this after the header background is painted, or the fill covers the lines.
    pub(super) fn column_handles(
        &self,
        ui: &mut Ui,
        header: Rect,
        prefs: &Cols,
        cols: &Cols,
        settings: &Settings,
    ) {
        let t = Tokens::get(ui.ctx());
        let mut next = settings.clone();
        let borders = Bar::ALL.map(|bar| (bar, cols.border(header, bar), cols.width(bar) > 0.0));
        for (bar, x, shown) in borders {
            if !shown {
                continue;
            }
            let grab =
                Rect::from_center_size(pos2(x, header.center().y), vec2(10.0, header.height()));
            let resp = ui
                .interact(
                    grab,
                    ui.id().with(("col-handle", bar)),
                    // Not a Tab stop: the keyboard can't drag it.
                    Sense::CLICK | Sense::DRAG,
                )
                .on_hover_cursor(egui::CursorIcon::ResizeHorizontal);
            crate::repo::widgets::a11y(&resp, egui::WidgetType::ResizeHandle, || {
                crate::i18n::message(
                    "repo-core-2609",
                    &[("bar", bar_name(bar).to_string().into())],
                )
            });
            let active = resp.hovered() || resp.dragged();
            ui.painter().line_segment(
                [pos2(x, header.top() + 5.0), pos2(x, header.bottom() - 5.0)],
                Stroke::new(HANDLE_WIDTH, if active { t.accent } else { HANDLE_COLOR }),
            );
            // A dragged graph width can cut lanes off; a double-click fits it to them again.
            if bar == Bar::Graph && resp.double_clicked() {
                next.graph_width = 0.0;
                continue;
            }
            let drag_id = ui.id().with(("column-drag-start", bar));
            // egui's press origin preserves the offset anywhere within the ten-point hit area.
            if resp.drag_started() {
                let origin = ui.input(|i| i.pointer.press_origin()).map_or(x, |p| p.x);
                ui.data_mut(|d| {
                    d.insert_temp(
                        drag_id,
                        DragStart {
                            prefs: *prefs,
                            shown: *cols,
                            description: settings.description_space,
                            available: header.width(),
                            pointer: origin,
                            preference: preference(settings, bar),
                            auto_refs_width: settings.auto_refs_width,
                        },
                    )
                });
            }
            if resp.drag_stopped() {
                ui.data_mut(|d| d.remove::<DragStart>(drag_id));
            }
            if !resp.dragged() {
                continue;
            }
            let Some(start) = ui.data(|d| d.get_temp::<DragStart>(drag_id)) else {
                continue;
            };
            let dx = resp
                .interact_pointer_pos()
                .map_or(0.0, |p| p.x - start.pointer);
            // Compute the proposed displayed width independently of the saved preference.
            // A compressed column can reach that preference away from the pointer origin.
            let widths = bar.drag_shown(&start.shown, &start.shown, dx, start.available);
            save_drag(&mut next, bar, prefs, &widths);
            // Returning to the origin and blocked gestures both leave the shown width intact.
            if widths.width(bar) == start.shown.width(bar) {
                *preference_mut(&mut next, bar) = start.preference;
                next.description_space = start.description;
                if bar == Bar::Refs {
                    next.auto_refs_width = start.auto_refs_width;
                }
            } else {
                // Retain each sibling's compression, not just the total Description space.
                // prefs was captured after readable_viewport's automatic refs adjustment.
                next.description_space =
                    DescriptionSpace::exchanged(start.available, &start.prefs, &widths, bar);
            }
        }
        if next != *settings {
            next.set(ui.ctx());
            // The new widths apply from the next frame: make sure there is one.
            ui.ctx().request_repaint();
        }
    }
}

/// The column a bar resizes, as a screen reader says it.
fn bar_name(bar: Bar) -> &'static str {
    match bar {
        Bar::Refs => "branches",
        Bar::Graph => "graph",
        Bar::Hash => "hash",
        Bar::Author => "author",
        Bar::When => "age",
        Bar::Date => crate::i18n::text("repo-core-2271"),
    }
}

#[cfg(test)]
#[path = "handles_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "handles_interaction_tests.rs"]
mod interaction_tests;
