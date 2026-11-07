//! The drag handles on the column borders of the graph's header.

use egui::{Rect, Sense, Stroke, Ui, pos2, vec2};

use super::{HANDLE_COLOR, HANDLE_WIDTH};
use crate::repo::RepoTab;
use crate::repo::columns::{Bar, Cols};
use crate::settings::Settings;
use crate::theme::Tokens;

/// One pointer gesture, including its original grab offset and saved preference.
#[derive(Clone, Copy)]
struct DragStart {
    prefs: Cols,
    available: f32,
    pointer: f32,
    preference: f32,
}

/// Persist only the intended column; temporary viewport compression is never saved.
fn save_drag(next: &mut Settings, bar: Bar, _prefs: &Cols, widths: &Cols) {
    next.last_resized = Some(bar);
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
                format!("Resize the {} column", bar_name(bar))
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
                            available: header.width(),
                            pointer: origin,
                            preference: preference(settings, bar),
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
            let widths = bar.drag(&start.prefs, None, dx, start.available);
            save_drag(&mut next, bar, prefs, &widths);
            if widths.width(bar) == start.prefs.width(bar) {
                *preference_mut(&mut next, bar) = start.preference;
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
        Bar::Date => "date",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dragging_another_bar_keeps_the_graph_fitted_to_its_lanes() {
        let prefs = Cols {
            refs: 200.0,
            graph: 100.0,
            hash: 0.0,
            author: 150.0,
            when: 0.0,
            date: 0.0,
            push: 0.0,
        };
        // A narrow view: the graph is squeezed on screen.
        let widths = Bar::Author.drag(&prefs, None, -10.0, 550.0);
        assert_eq!(widths.graph, prefs.graph);
        let mut next = Settings::default();
        save_drag(&mut next, Bar::Author, &prefs, &widths);
        assert_eq!(next.graph_width, 0.0);
        assert_eq!(next.author_width, widths.author);
        assert_eq!(next.last_resized, Some(Bar::Author));
        // Its own bar still sets it.
        let widths = Bar::Graph.drag(&prefs, None, 10.0, 700.0);
        save_drag(&mut next, Bar::Graph, &prefs, &widths);
        assert_eq!(next.graph_width, 110.0);
    }
}

#[cfg(test)]
mod interaction_tests {
    use super::*;
    use crate::repo::graph_view::{lane_layout, preferred_cols};

    #[test]
    fn snapshot_drags_preserve_the_initial_hit_offset_and_other_preferences() {
        for bar in Bar::ALL {
            for offset in [-4.0, 4.0] {
                let ctx = egui::Context::default();
                let tab = RepoTab::new_deferred("/nonexistent/column-drag".into(), &ctx);
                let initial = Settings {
                    graph_width: 100.0,
                    show_hash: true,
                    show_date: true,
                    ..Default::default()
                };
                initial.set(&ctx);
                let frame = |events| {
                    ctx.run_ui(
                        egui::RawInput {
                            screen_rect: Some(Rect::from_min_size(
                                egui::Pos2::ZERO,
                                vec2(1600.0, 200.0),
                            )),
                            events,
                            ..Default::default()
                        },
                        |ui| {
                            let settings = Settings::get(&ctx);
                            let prefs = preferred_cols(&settings, lane_layout(&settings, 3));
                            let header = Rect::from_min_size(egui::Pos2::ZERO, vec2(1600.0, 26.0));
                            let shown = prefs.squeezed(header.width(), None);
                            tab.column_handles(ui, header, &prefs, &shown, &settings);
                        },
                    )
                    .drop_without_applying_deltas();
                };
                let prefs = preferred_cols(&initial, lane_layout(&initial, 3));
                let header = Rect::from_min_size(egui::Pos2::ZERO, vec2(1600.0, 26.0));
                let press = pos2(prefs.border(header, bar) + offset, 13.0);
                let button = |pos, pressed| egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                };
                frame(vec![egui::Event::PointerMoved(press)]);
                frame(vec![button(press, true)]);
                for delta in [10.0, 20.0, 5000.0, 20.0] {
                    frame(vec![egui::Event::PointerMoved(press + vec2(delta, 0.0))]);
                    let saved = Settings::get(&ctx);
                    for other in Bar::ALL {
                        if other != bar {
                            assert_eq!(
                                preference(&saved, other),
                                preference(&initial, other),
                                "{bar:?} touched {other:?}"
                            );
                        }
                    }
                    if delta == 20.0 {
                        let sign = if matches!(bar, Bar::Refs | Bar::Graph) {
                            1.0
                        } else {
                            -1.0
                        };
                        assert_eq!(
                            preference(&saved, bar),
                            preference(&initial, bar) + sign * delta,
                            "{bar:?} offset {offset}"
                        );
                    }
                }
                frame(vec![egui::Event::PointerMoved(press)]);
                assert_eq!(
                    preference(&Settings::get(&ctx), bar),
                    preference(&initial, bar)
                );
                frame(vec![button(press, false)]);
            }
        }
    }
}
