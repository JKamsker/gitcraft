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
    commits: &[std::sync::Arc<crate::git::Commit>],
    graph: &crate::graph::Graph,
    head: &str,
    matches: &std::collections::HashSet<String>,
    active: &dyn Fn() -> bool,
) -> Result<Model, String> {
    let rows = commits.len();
    let mut bins = vec![Bin::default(); rows.min(BINS)];
    for projected in 0..rows {
        if !active() {
            return Err("Minimap analysis cancelled".into());
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
        for c in &commits[raw..raw + 1] {
            bin.head |= c.hash == head;
            bin.tag |= c
                .refs
                .iter()
                .any(|r| r.starts_with("tag: ") || r.starts_with("refs/tags/"));
            bin.hit |= matches.contains(&c.hash);
        }
    }
    Ok(Model { bins, rows })
}
impl RepoTab {
    fn prepare_minimap(&mut self, ctx: &egui::Context) {
        if self.history_minimap.dirty {
            let commits = self.commits.clone();
            let graph = self.graph.clone();
            let head = self.refs.head.clone();
            let matches = self.search.matches.clone();
            self.history_minimap.bins.clear();
            self.history_minimap.rows = 0;
            self.history_minimap
                .reader
                .queue(move |active| build(&commits, &graph, &head, &matches, active));
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
        let label = "Loaded history minimap. Drag to scroll; arrow and Page Up/Down keys move the viewport. Blue HEAD, green tags, yellow search hits.";
        let p = ui.painter_at(rect);
        p.rect_filled(rect, 3.0, t.dock);
        p.text(
            pos2(rect.center().x, rect.top() + 3.0),
            Align2::CENTER_TOP,
            "History",
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
        let label = if self.has_more {
            "Older unloaded ↓"
        } else {
            "History ends"
        };
        p.line_segment(
            [pos2(map.left(), map.bottom()), map.right_bottom()],
            Stroke::new(1.0, t.text_faint),
        );
        p.text(
            pos2(rect.center().x, rect.bottom() - 3.0),
            Align2::CENTER_BOTTOM,
            label,
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
        response.on_hover_text(format!("{} projected rows of loaded history. Drag to scroll; focused: arrows or Page Up/Down. Blue: HEAD · green: tags · yellow: search hits · grey: collapsed context. {}",self.history_minimap.rows,if self.has_more {"Older history is not loaded; scrolling to the boundary loads another page."} else {"All available history is loaded."}));
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use egui::vec2;
    #[test]
    fn actual_minimap_marks_loaded_refs_and_clicks_scroll_without_changing_selection() {
        use std::sync::Arc;
        let ctx = crate::theme::test_paint::context();
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
        tab.search.matches.insert(tab.commits[70].hash.clone());
        tab.selected = Some(0);
        tab.has_more = true;
        let model = build(
            &tab.commits,
            &tab.graph,
            &tab.refs.head,
            &tab.search.matches,
            &|| true,
        )
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
        ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(rect),
                ..Default::default()
            },
            |ui| tab.history_minimap(ui, rect, 0.0, 300.0),
        )
        .drop_without_applying_deltas();
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
