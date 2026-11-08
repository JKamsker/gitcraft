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
    #[test]
    fn resizing_auto_refs_fixes_the_width_without_an_initial_jump() {
        let prefs = Cols {
            refs: crate::repo::columns::MIN_REFS_W,
            graph: 60.0,
            hash: 0.0,
            author: 150.0,
            when: 100.0,
            date: 0.0,
            push: 0.0,
        };
        let mut settings = Settings::default();
        let width = Bar::Refs.drag(&prefs, None, 10.0, 900.0);
        save_drag(&mut settings, Bar::Refs, &prefs, &width);
        assert_eq!(settings.refs_width, prefs.refs + 10.0);
        assert!(!settings.auto_refs_width);
        let mut at_minimum = Settings::default();
        let width = Bar::Refs.drag(&prefs, None, -100.0, 900.0);
        save_drag(&mut at_minimum, Bar::Refs, &prefs, &width);
        assert_eq!(at_minimum.refs_width, prefs.refs);

        // Resizing another column must never persist a temporary auto-sized refs width.
        let mut settings = Settings::default();
        let width = Bar::Author.drag(&prefs, None, -100.0, 500.0);
        save_drag(&mut settings, Bar::Author, &prefs, &width);
        assert_eq!(settings.refs_width, Settings::default().refs_width);
        assert!(settings.fit_visible_refs());
    }
}

#[cfg(test)]
mod interaction_tests {
    use super::*;
    use crate::repo::graph_view::{lane_layout, preferred_cols};

    struct HandleFixture {
        ctx: egui::Context,
        tab: RepoTab,
        header: Rect,
    }

    impl HandleFixture {
        fn new(settings: &Settings, available: f32) -> Self {
            let ctx = egui::Context::default();
            let tab = RepoTab::new_deferred("/nonexistent/column-drag".into(), &ctx);
            settings.set(&ctx);
            Self {
                ctx,
                tab,
                header: Rect::from_min_size(egui::Pos2::ZERO, vec2(available, 26.0)),
            }
        }

        fn shown(&self) -> Cols {
            let settings = Settings::get(&self.ctx);
            preferred_cols(&settings, lane_layout(&settings, 3))
                .allocated(self.header.width(), settings.description_space)
        }

        fn frame(&self, events: Vec<egui::Event>) {
            self.ctx
                .run_ui(
                    egui::RawInput {
                        screen_rect: Some(Rect::from_min_size(
                            egui::Pos2::ZERO,
                            vec2(self.header.width(), 200.0),
                        )),
                        events,
                        ..Default::default()
                    },
                    |ui| {
                        let settings = Settings::get(&self.ctx);
                        let prefs = preferred_cols(&settings, lane_layout(&settings, 3));
                        self.tab
                            .column_handles(ui, self.header, &prefs, &self.shown(), &settings);
                    },
                )
                .drop_without_applying_deltas();
        }

        fn press(&self, bar: Bar, offset: f32) -> egui::Pos2 {
            let point = pos2(self.shown().border(self.header, bar) + offset, 13.0);
            self.frame(vec![egui::Event::PointerMoved(point)]);
            self.button(point, true);
            point
        }

        fn button(&self, pos: egui::Pos2, pressed: bool) {
            self.frame(vec![egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            }]);
        }
    }

    #[test]
    fn compressed_refs_cross_the_saved_width_without_jumping() {
        for auto_refs_width in [false, true] {
            for offset in [-4.0, 4.0] {
                let initial = Settings {
                    refs_width: 280.0,
                    graph_width: 100.0,
                    author_width: 280.0,
                    when_width: 100.0,
                    show_hash: false,
                    show_date: false,
                    auto_refs_width,
                    ..Default::default()
                };
                let fixture = HandleFixture::new(&initial, 760.0);
                let original = fixture.shown();
                assert_eq!(
                    (
                        original.refs,
                        original.graph,
                        original.author,
                        original.when
                    ),
                    (180.0, 60.0, 280.0, 100.0)
                );
                assert_eq!(original.description(fixture.header).width(), 140.0);

                // Create the reservation by shrinking Author through its real pointer handle.
                let author_press = fixture.press(Bar::Author, offset);
                let author_end = author_press + vec2(130.0, 0.0);
                fixture.frame(vec![egui::Event::PointerMoved(author_end)]);
                fixture.button(author_end, false);
                let start = Settings::get(&fixture.ctx);
                let shown = fixture.shown();
                assert_eq!(
                    (shown.refs, shown.graph, shown.author, shown.when),
                    (180.0, 60.0, 150.0, 100.0)
                );
                assert_eq!(start.refs_width, 280.0);
                assert_eq!(shown.description(fixture.header).width(), 270.0);

                let press = fixture.press(Bar::Refs, offset);
                // Cross both the old preference and the Description-floor clamp in both
                // directions, overshoot, and return exactly to the original grab point.
                for dx in [
                    99.0_f32, 100.0, 101.0, 129.0, 130.0, 131.0, 5000.0, 131.0, 130.0, 129.0,
                    101.0, 100.0, 99.0, 0.0,
                ] {
                    fixture.frame(vec![egui::Event::PointerMoved(press + vec2(dx, 0.0))]);
                    let saved = Settings::get(&fixture.ctx);
                    let after = fixture.shown();
                    let change = dx.min(130.0);
                    assert_eq!(after.refs, shown.refs + change, "dx={dx}, offset={offset}");
                    assert_eq!(after.description(fixture.header).width(), 270.0 - change);
                    for other in Bar::ALL {
                        if other != Bar::Refs {
                            assert_eq!(preference(&saved, other), preference(&start, other));
                            assert_eq!(after.width(other), shown.width(other));
                        }
                    }
                    if dx == 0.0 {
                        assert_eq!(saved.refs_width, start.refs_width);
                        assert_eq!(saved.description_space, start.description_space);
                        assert_eq!(saved.auto_refs_width, start.auto_refs_width);
                    } else {
                        assert_eq!(saved.refs_width, after.refs);
                        assert!(!saved.auto_refs_width);
                    }
                }
                fixture.button(press, false);
                assert_eq!(fixture.shown(), shown);

                // Releasing at the old preferred width must preserve the exchanged space.
                let press = fixture.press(Bar::Refs, offset);
                let end = press + vec2(100.0, 0.0);
                fixture.frame(vec![egui::Event::PointerMoved(end)]);
                fixture.button(end, false);
                fixture.frame(vec![]);
                let saved = Settings::get(&fixture.ctx);
                assert_eq!(fixture.shown().refs, 280.0);
                assert_eq!(fixture.shown().description(fixture.header).width(), 170.0);
                assert_eq!(
                    saved.description_space,
                    DescriptionSpace {
                        viewport: 760.0,
                        width: 170.0,
                        compression: Some([0.0, 40.0, 0.0, 0.0, 0.0, 0.0]),
                    }
                );
                assert!(!saved.auto_refs_width);
            }
        }
    }

    #[test]
    fn blocked_pointer_drags_preserve_preferences_and_description_reservation() {
        for auto_refs_width in [false, true] {
            for bar in Bar::ALL {
                let initial = Settings {
                    refs_width: 280.0,
                    graph_width: 0.0,
                    author_width: 280.0,
                    show_hash: true,
                    show_date: true,
                    auto_refs_width,
                    description_space: DescriptionSpace {
                        viewport: 760.0,
                        width: 270.0,
                        ..Default::default()
                    },
                    ..Default::default()
                };
                // Every column is at its minimum, as is Description: both directions block.
                let fixture = HandleFixture::new(&initial, 500.0);
                let shown = fixture.shown();
                let press = fixture.press(bar, 4.0);
                for dx in [20.0, 5000.0, 0.0, -20.0, -5000.0, 0.0] {
                    fixture.frame(vec![egui::Event::PointerMoved(press + vec2(dx, 0.0))]);
                    let saved = Settings::get(&fixture.ctx);
                    assert_eq!(fixture.shown(), shown, "{bar:?}, dx={dx}");
                    for other in Bar::ALL {
                        assert_eq!(preference(&saved, other), preference(&initial, other));
                    }
                    assert_eq!(saved.description_space, initial.description_space);
                    assert_eq!(saved.auto_refs_width, initial.auto_refs_width);
                }
                fixture.button(press, false);
                assert_eq!(fixture.shown(), shown);
            }
        }
    }

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
                            let shown = prefs.allocated(header.width(), settings.description_space);
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
                assert_eq!(
                    Settings::get(&ctx).auto_refs_width,
                    initial.auto_refs_width,
                    "returning to the press must preserve automatic ref sizing"
                );
                frame(vec![button(press, false)]);
            }
        }
    }
}
