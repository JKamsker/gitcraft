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
                99.0_f32, 100.0, 101.0, 129.0, 130.0, 131.0, 5000.0, 131.0, 130.0, 129.0, 101.0,
                100.0, 99.0, 0.0,
            ] {
                fixture.frame(vec![egui::Event::PointerMoved(press + vec2(dx, 0.0))]);
                let saved = Settings::get(&fixture.ctx);
                let after = fixture.shown();
                let change = dx.min(130.0);
                assert_eq!(after.refs, shown.refs + change, "dx={dx}, offset={offset}");
                assert_eq!(after.description(fixture.header).width(), 270.0 - change);
                assert_refs_exchange(&saved, &start, &after, &shown, dx);
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

fn assert_refs_exchange(saved: &Settings, start: &Settings, after: &Cols, shown: &Cols, dx: f32) {
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
