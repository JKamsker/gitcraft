//! A bounded overview of projected, loaded history. Bin counts cap paint work independently of history depth.
use super::ROW_PITCH;
use crate::repo::RepoTab;
use crate::settings::{HistoryMap, Settings};
use crate::theme::Tokens;
use egui::{Align2, FontId, Rect, Sense, Stroke, StrokeKind, Ui, pos2, vec2};
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
/// The map's right-click menu: Large, Small, or hidden (Settings → Appearance brings it back).
fn size_menu(response: &egui::Response) {
    crate::menu_item::context(response)
        .style(crate::theme::menu_style)
        .show(|ui| {
            let mut settings = Settings::get(ui.ctx());
            let before = settings.history_map;
            HistoryMap::menu(ui, &mut settings.history_map);
            if settings.history_map != before {
                settings.set(ui.ctx());
            }
        });
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
        size_menu(&response);
        // Small leaves out the caption and the footer: the lanes take the whole strip.
        let small = Settings::history_map(ui.ctx()) == HistoryMap::Small;
        let p = ui.painter_at(rect);
        p.rect_filled(rect, 3.0, t.dock);
        if !small {
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
        }
        let map = if small {
            rect.shrink2(vec2(3.0, 4.0))
        } else {
            Rect::from_min_max(
                pos2(rect.left() + 4.0, rect.top() + 19.0),
                pos2(rect.right() - 4.0, rect.bottom() - 20.0),
            )
        };
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
        if !small {
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
        }
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
#[path = "minimap_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "minimap_merge_regressions.rs"]
mod merge_regressions;
