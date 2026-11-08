//! A bounded overview of projected, loaded history. Bin counts cap paint work independently of history depth.
use super::ROW_PITCH;
use crate::repo::RepoTab;
use crate::theme::Tokens;
use egui::{Align2, FontId, Rect, Sense, Stroke, StrokeKind, Ui, pos2};
const BINS: usize = 512;
const LANES: usize = 24;
#[derive(Clone, Default)]
struct Bin {
    lanes: u32,
    head: bool,
    tag: bool,
    hit: bool,
    gap: bool,
}
pub(in crate::repo) struct State {
    pub dirty: bool,
    bins: Vec<Bin>,
    rows: usize,
    pub offset: Option<f32>,
    drag_anchor: f32,
    reader: crate::repo::history_reader::Reader<Model>,
}
impl Default for State {
    fn default() -> Self {
        Self {
            dirty: true,
            bins: Vec::new(),
            rows: 0,
            offset: None,
            drag_anchor: 0.0,
            reader: Default::default(),
        }
    }
}
fn bin_for(row: usize, rows: usize, bins: usize) -> usize {
    (row.saturating_mul(bins) / rows.max(1)).min(bins.saturating_sub(1))
}
fn offset_at(fraction: f32, rows: usize, viewport: f32, anchor: f32) -> f32 {
    (fraction.clamp(0.0, 1.0) * rows as f32 * ROW_PITCH - anchor)
        .clamp(0.0, (rows as f32 * ROW_PITCH - viewport).max(0.0))
}
struct Model {
    bins: Vec<Bin>,
    rows: usize,
}
fn build(
    graph: &crate::graph::Graph,
    head: &str,
    matches: &std::collections::HashSet<String>,
    active: &dyn Fn() -> bool,
) -> Result<Model, String> {
    let rows = graph.rows.len();
    let mut bins = vec![Bin::default(); rows.min(BINS)];
    for projected in 0..rows {
        if !active() {
            return Err(crate::i18n::text("repo-core-2669").into());
        }
        let raw = projected;
        let Some(g) = graph.rows.get(raw) else {
            continue;
        };
        let bin = &mut bins[bin_for(projected, rows, rows.min(BINS))];
        for col in std::iter::once(&g.col)
            .chain(g.through.iter())
            .chain(g.incoming.iter())
            .chain(g.outgoing.iter())
        {
            if *col < LANES {
                bin.lanes |= 1 << col;
            }
        }
        bin.head |= g.hash.as_ref() == head;
        bin.tag |= g.tagged;
        bin.hit |= matches.contains(g.hash.as_ref());
    }
    Ok(Model { bins, rows })
}
fn lane_coverage(width: usize) -> String {
    if width > LANES {
        crate::i18n::message(
            "repo-core-2670",
            &[
                ("LANES", format!("{}", LANES).into()),
                ("width", format!("{}", width).into()),
            ],
        )
    } else {
        crate::i18n::message(
            "repo-core-2671",
            &[("max", format!("{}", width.max(1)).into())],
        )
    }
}
impl RepoTab {
    fn prepare_minimap(&mut self, ctx: &egui::Context) {
        if self.history_minimap.dirty {
            let graph = self.graph.clone();
            let head = self.refs.head.clone();
            let matches = self.search.matches.clone();
            self.history_minimap.bins.clear();
            self.history_minimap.rows = 0;
            self.history_minimap
                .reader
                .queue(move |active| build(&graph, &head, &matches, active));
            self.history_minimap.dirty = false;
        }
        if let Some(Ok(model)) = self.history_minimap.reader.poll(&self.tx, ctx) {
            self.history_minimap.bins = model.bins;
            self.history_minimap.rows = model.rows;
        }
    }
    pub(in crate::repo) fn cancel_minimap(&mut self) {
        self.history_minimap.reader.cancel();
        self.history_minimap.dirty = true;
    }
    pub(super) fn history_minimap(&mut self, ui: &mut Ui, rect: Rect, offset: f32, viewport: f32) {
        self.prepare_minimap(ui.ctx());
        let t = Tokens::get(ui.ctx());
        let response = ui.interact(
            rect,
            ui.id().with("history-minimap"),
            Sense::click_and_drag(),
        );
        let coverage = lane_coverage(self.graph.width);
        let label = crate::i18n::message(
            "repo-core-2673",
            &[("coverage", coverage.to_string().into())],
        );
        let p = ui.painter_at(rect);
        p.rect_filled(rect, 3.0, t.dock);
        p.text(
            pos2(rect.center().x, rect.top() + 3.0),
            Align2::CENTER_TOP,
            if self.graph.width > LANES {
                crate::i18n::text("repo-core-2674")
            } else {
                crate::i18n::text("repo-core-2497")
            },
            FontId::proportional(10.0),
            t.text_dim,
        );
        let map = Rect::from_min_max(
            pos2(rect.left() + 4.0, rect.top() + 19.0),
            pos2(rect.right() - 4.0, rect.bottom() - 20.0),
        );
        if map.height() <= 0.0 || self.history_minimap.rows == 0 {
            crate::a11y::value::slider(&response, label, 0.0, 0.0..=0.0, ROW_PITCH, true);
            return;
        }
        let n = self.history_minimap.bins.len();
        let shown = self.graph.width.clamp(1, LANES);
        for (i, bin) in self.history_minimap.bins.iter().enumerate() {
            let y = map.top() + (i as f32 + 0.5) / n as f32 * map.height();
            let height = (map.height() / n as f32).max(1.0);
            for lane in 0..shown {
                if bin.lanes & (1 << lane) != 0 {
                    let x = map.left() + 3.0 + lane as f32 / shown as f32 * (map.width() - 13.0);
                    p.line_segment(
                        [pos2(x, y), pos2(x, y + height)],
                        Stroke::new(1.2, if bin.gap { t.text_faint } else { t.text_dim }),
                    );
                }
            }
            if bin.tag {
                p.circle_filled(pos2(map.right() - 5.0, y), 2.0, t.success);
            }
            if bin.hit {
                p.line_segment(
                    [pos2(map.left(), y), pos2(map.left() + 4.0, y)],
                    Stroke::new(2.0, t.warning),
                );
            }
            if bin.head {
                p.line_segment(
                    [pos2(map.right() - 8.0, y), pos2(map.right(), y)],
                    Stroke::new(2.0, t.accent),
                );
            }
        }
        let full_h = self.history_minimap.rows as f32 * ROW_PITCH;
        let top = (offset / full_h).clamp(0.0, 1.0);
        let bottom = ((offset + viewport) / full_h).clamp(top, 1.0);
        let visible = Rect::from_min_max(
            pos2(map.left(), map.top() + top * map.height()),
            pos2(
                map.right(),
                (map.top() + bottom * map.height()).max(map.top() + top * map.height() + 3.0),
            ),
        );
        p.rect_stroke(visible, 1.0, Stroke::new(1.5, t.accent), StrokeKind::Inside);
        let footer_label = if self.has_more {
            crate::i18n::text("repo-core-2675")
        } else {
            crate::i18n::text("repo-core-2676")
        };
        p.line_segment(
            [pos2(map.left(), map.bottom()), map.right_bottom()],
            Stroke::new(1.0, t.text_faint),
        );
        p.text(
            pos2(rect.center().x, rect.bottom() - 3.0),
            Align2::CENTER_BOTTOM,
            footer_label,
            FontId::proportional(8.0),
            t.text_dim,
        );
        if response.drag_started()
            && let Some(pointer) = response.interact_pointer_pos()
        {
            self.history_minimap.drag_anchor = if visible.contains(pointer) {
                ((pointer.y - map.top()) / map.height() * full_h - offset).clamp(0.0, viewport)
            } else {
                viewport / 2.0
            };
        }
        if (response.dragged() || response.clicked())
            && let Some(pointer) = response.interact_pointer_pos()
        {
            let anchor = if response.clicked() {
                viewport / 2.0
            } else {
                self.history_minimap.drag_anchor
            };
            self.history_minimap.offset = Some(offset_at(
                (pointer.y - map.top()) / map.height(),
                self.history_minimap.rows,
                viewport,
                anchor,
            ));
            response.request_focus();
            ui.ctx().request_repaint();
        }
        if response.has_focus() {
            let delta = ui.input(|i| {
                if i.key_pressed(egui::Key::ArrowDown) {
                    ROW_PITCH
                } else if i.key_pressed(egui::Key::ArrowUp) {
                    -ROW_PITCH
                } else if i.key_pressed(egui::Key::PageDown) {
                    viewport
                } else if i.key_pressed(egui::Key::PageUp) {
                    -viewport
                } else {
                    0.0
                }
            });
            if delta != 0.0 {
                self.history_minimap.offset =
                    Some((offset + delta).clamp(0.0, (full_h - viewport).max(0.0)));
            }
        }
        if let Some(at) = crate::a11y::value::slider(
            &response,
            label,
            self.history_minimap.offset.unwrap_or(offset),
            0.0..=(full_h - viewport).max(0.0),
            ROW_PITCH,
            true,
        ) {
            self.history_minimap.offset = Some(at);
            ui.ctx().request_repaint();
        }
        response.on_hover_text(crate::i18n::message(
            "repo-core-2677",
            &[
                ("rows", format!("{}", self.history_minimap.rows).into()),
                ("coverage", coverage.to_string().into()),
                (
                    "loaded",
                    (if self.has_more {
                        crate::i18n::text("repo-core-2678")
                    } else {
                        crate::i18n::text("repo-core-2679")
                    })
                    .to_string()
                    .into(),
                ),
            ],
        ));
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use egui::vec2;
    #[test]
    fn large_minimap_capture_shares_rows_edges_and_search_set() {
        use std::sync::Arc;
        let ctx = egui::Context::default();
        let mut tab = RepoTab::new_deferred("/nonexistent/minimap-capture".into(), &ctx);
        let commits: Vec<_> = (0..10_000)
            .map(|i| {
                Arc::new(crate::git::Commit {
                    hash: format!("{i:040x}"),
                    parents: if i < 9_999 {
                        vec![format!("{:040x}", i + 1)]
                    } else {
                        Vec::new()
                    },
                    ..Default::default()
                })
            })
            .collect();
        tab.graph = crate::graph::layout(&commits);
        let before = tab.graph.rows.as_ptr();
        let first_edges = tab.graph.rows[0].outgoing.as_ptr();
        let graph = tab.graph.clone();
        let matches = tab.search.matches.clone();
        assert_eq!(graph.rows.as_ptr(), before);
        assert_eq!(graph.rows[0].outgoing.as_ptr(), first_edges);
        assert!(Arc::ptr_eq(&matches, &tab.search.matches));
        assert_eq!(Arc::strong_count(&graph.rows), 2);
        // UI queue capture uses those same Arc clones; preparation changes no row.
        tab.prepare_minimap(&ctx);
        assert_eq!(tab.graph.rows.as_ptr(), before);
        assert!(!tab.history_minimap.dirty);
        assert!(Arc::strong_count(&graph.rows) >= 2);
        assert!(lane_coverage(32).contains("first 24 of 32 lanes"));
    }
    #[test]
    fn minimap_uses_shared_focus_ring_and_keyboard_scroll() {
        use egui_kittest::{Harness, kittest::Queryable};
        let ctx = egui::Context::default();
        let mut tab = RepoTab::new_deferred("/nonexistent/minimap-focus".into(), &ctx);
        tab.history_minimap.dirty = false;
        tab.history_minimap.rows = 100;
        tab.history_minimap.bins = vec![
            Bin {
                lanes: 1,
                ..Default::default()
            };
            100
        ];
        tab.graph.width = 32;
        let mut harness = Harness::builder().with_size([150.0, 450.0]).build_ui_state(
            |ui, tab: &mut RepoTab| {
                let rect = Rect::from_min_size(pos2(8.0, 8.0), vec2(110.0, 400.0));
                tab.history_minimap(ui, rect, 0.0, 300.0);
            },
            tab,
        );
        harness.run();
        harness.key_press(egui::Key::Tab);
        harness.run();
        assert!(
            harness
                .get_by_label_contains("first 24 of 32 lanes")
                .is_focused()
        );
        harness.key_press(egui::Key::PageDown);
        harness.run();
        assert_eq!(harness.state().history_minimap.offset, Some(300.0));
    }
    #[test]
    fn actual_minimap_marks_loaded_refs_and_clicks_scroll_without_changing_selection()
    -> Result<(), String> {
        use std::sync::Arc;
        let ctx = crate::theme::test_paint::context();
        ctx.enable_accesskit();
        let mut tab = RepoTab::new_deferred("/nonexistent/minimap-fixture".into(), &ctx);
        tab.commits = (0..100)
            .map(|i| {
                Arc::new(crate::git::Commit {
                    hash: format!("{i:040x}"),
                    parents: if i < 99 {
                        vec![format!("{:040x}", i + 1)]
                    } else {
                        Vec::new()
                    },
                    refs: if i == 50 {
                        vec!["tag: v1".into()]
                    } else {
                        Vec::new()
                    },
                    ..Default::default()
                })
            })
            .collect();
        tab.graph = crate::graph::layout(&tab.commits);
        tab.refs.head = tab.commits[20].hash.clone();
        std::sync::Arc::make_mut(&mut tab.search.matches).insert(tab.commits[70].hash.clone());
        tab.selected = Some(0);
        tab.has_more = true;
        let model = build(&tab.graph, &tab.refs.head, &tab.search.matches, &|| true)
            .unwrap_or_else(|_| Model {
                bins: Vec::new(),
                rows: 0,
            });
        tab.history_minimap.bins = model.bins;
        tab.history_minimap.rows = model.rows;
        tab.history_minimap.dirty = false;
        assert!(tab.history_minimap.bins[20].head);
        assert!(tab.history_minimap.bins[50].tag);
        assert!(tab.history_minimap.bins[70].hit);
        let rect = Rect::from_min_size(egui::Pos2::ZERO, vec2(78.0, 400.0));
        let point = pos2(38.0, 300.0);
        for has_more in [false, true] {
            tab.has_more = has_more;
            let output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(rect),
                    ..Default::default()
                },
                |ui| tab.history_minimap(ui, rect, 0.0, 300.0),
            );
            let update = output
                .platform_output
                .accesskit_update
                .as_ref()
                .ok_or("No AccessKit update")?;
            let (_, slider) = update
                .nodes
                .iter()
                .find(|(_, node)| node.role() == egui::accesskit::Role::Slider)
                .ok_or("No minimap Slider")?;
            assert_eq!(
                slider.label(),
                Some(
                    "Loaded history minimap. Drag to scroll; arrow and Page Up/Down keys move the viewport. Blue HEAD, green tags, yellow search hits."
                )
            );
            assert_eq!(slider.numeric_value(), Some(0.0));
            assert_eq!(
                slider.max_numeric_value(),
                Some(f64::from(100.0 * ROW_PITCH - 300.0))
            );
            output.drop_without_applying_deltas();
        }
        for pressed in [true, false] {
            let events = vec![
                egui::Event::PointerMoved(point),
                egui::Event::PointerButton {
                    pos: point,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: Default::default(),
                },
            ];
            ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(rect),
                    events,
                    ..Default::default()
                },
                |ui| tab.history_minimap(ui, rect, 0.0, 300.0),
            )
            .drop_without_applying_deltas();
        }
        assert!(
            tab.history_minimap
                .offset
                .is_some_and(|offset| offset > 1500.0)
        );
        assert_eq!(tab.selected, Some(0));
        Ok(())
    }
    #[test]
    fn endpoints_and_drag_anchor_respect_loaded_extent() {
        assert_eq!(bin_for(99_999, 100_000, BINS), BINS - 1);
        assert_eq!(offset_at(0.0, 1000, 300.0, 150.0), 0.0);
        assert_eq!(offset_at(1.0, 1000, 300.0, 150.0), 29_700.0);
        assert_eq!(offset_at(0.5, 1000, 300.0, 50.0), 14_950.0);
        assert_eq!(offset_at(1.0, 2, 300.0, 150.0), 0.0);
    }
}

#[cfg(test)]
mod merge_regressions {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn reserved_minimap_is_painted_outside_horizontal_graph_clip() -> Result<(), String> {
        let ctx = crate::theme::test_paint::context();
        let mut tab = RepoTab::new_deferred("/nonexistent/minimap-merge-recording".into(), &ctx);
        tab.commits = (0..100)
            .map(|i| {
                Arc::new(crate::git::Commit {
                    hash: format!("{i:040x}"),
                    parents: if i < 99 {
                        vec![format!("{:040x}", i + 1)]
                    } else {
                        vec![]
                    },
                    summary: format!("supplied row {i}"),
                    ..Default::default()
                })
            })
            .collect();
        tab.graph = crate::graph::layout(&tab.commits);
        tab.history = tab.commits.clone();
        let model = build(&tab.graph, "", &Default::default(), &|| true)?;
        tab.history_minimap.dirty = false;
        tab.history_minimap.bins = model.bins;
        tab.history_minimap.rows = model.rows;
        let mut h = egui_kittest::Harness::builder()
            .with_size([900.0, 500.0])
            .build_ui_state(
                |ui, tab: &mut RepoTab| {
                    crate::theme::install_fonts(ui.ctx());
                    egui_extras::install_image_loaders(ui.ctx());
                    tab.graph_rows_view(ui, &mut vec![]);
                },
                tab,
            );
        h.run();
        let label = crate::i18n::text("repo-core-2497");
        let text = h
            .output()
            .shapes
            .iter()
            .find_map(|s| match &s.shape {
                egui::Shape::Text(t) if t.galley.job.text == label => {
                    Some((t.pos, t.galley.size(), s.clip_rect))
                }
                _ => None,
            })
            .ok_or("minimap title was not painted")?;
        let bounds = Rect::from_min_size(text.0, text.1);
        assert!(
            text.2.contains_rect(bounds),
            "minimap title {bounds:?} clipped by {:?}",
            text.2
        );
        assert!(
            bounds.left() > 780.0,
            "not the reserved right-hand strip: {bounds:?}"
        );
        assert_eq!(h.state().commits.len(), 100);
        Ok(())
    }
}
