//! The ribbon map: the old version's commits in a column on the left, the new version's on the
//! right, oldest at the top, and a curved ribbon from each old commit to what it became. A
//! ribbon's colour and its commits' letters say what became of it; its middle line, solid or
//! dashed, whether that is exact or inferred. Only the rows in view are drawn.

use egui::{Align2, Color32, Key, Pos2, Rect, Sense, Shape, Stroke, Ui, pos2, vec2};

use super::{Pick, Picked, color, label, named};
use crate::insights::Evidence;
use crate::insights::ribbons::{End, Ribbon, Ribbons};
use crate::repo::insights::common::{self, Mark};
use crate::theme::{self, Tokens};

const ROW_H: f32 = 30.0;
const BOX_H: f32 = 24.0;
/// Points along each ribbon.
const STEPS: usize = 24;

/// Where the columns are.
struct Columns {
    left: f32,
    right: f32,
    width: f32,
    top: f32,
}

impl Columns {
    fn old_box(&self, i: usize) -> Rect {
        Rect::from_min_size(
            pos2(
                self.left,
                self.top + i as f32 * ROW_H + (ROW_H - BOX_H) / 2.0,
            ),
            vec2(self.width, BOX_H),
        )
    }

    fn new_box(&self, i: usize) -> Rect {
        Rect::from_min_size(
            pos2(
                self.right,
                self.top + i as f32 * ROW_H + (ROW_H - BOX_H) / 2.0,
            ),
            vec2(self.width, BOX_H),
        )
    }

    /// The points along ribbon `r`, as far as `progress` (0 to 1) has drawn it.
    fn curve(&self, r: &Ribbon, progress: f32) -> Vec<Pos2> {
        let a = self.old_box(r.old).right_center();
        let b = self.new_box(r.new).left_center();
        let pull = (b.x - a.x) / 2.0;
        let (c1, c2) = (a + vec2(pull, 0.0), b - vec2(pull, 0.0));
        let last = ((STEPS as f32 * progress).ceil() as usize).min(STEPS);
        (0..=last)
            .map(|k| {
                let s = (k as f32 / STEPS as f32).min(progress);
                let u = 1.0 - s;
                let w = [u * u * u, 3.0 * u * u * s, 3.0 * u * s * s, s * s * s];
                pos2(
                    w[0] * a.x + w[1] * c1.x + w[2] * c2.x + w[3] * b.x,
                    w[0] * a.y + w[1] * c1.y + w[2] * c2.y + w[3] * b.y,
                )
            })
            .collect()
    }
}

/// The ribbon map of `data`, `selected` marked, the ribbons drawn as far as `progress`, at most
/// `max_h` tall (it scrolls). Arrows step through the ribbons once it has the focus.
pub(super) fn show(
    ui: &mut Ui,
    data: &Ribbons,
    selected: Option<Pick>,
    progress: f32,
    sides: bool,
    max_h: f32,
) -> Picked {
    let t = Tokens::get(ui.ctx());
    let mut picked = Picked::default();
    let rows = data.old.len().max(data.new.len());
    let width = ui.available_width();
    egui::ScrollArea::vertical()
        .id_salt("ribbons-map")
        .max_height(max_h)
        .auto_shrink([false, true])
        .show_viewport(ui, |ui, viewport| {
            let size = vec2(width, rows as f32 * ROW_H + 8.0);
            let (outer, resp) = ui.allocate_exact_size(size, Sense::click());
            crate::a11y::a11y(
                &resp,
                egui::WidgetType::Other,
                format!(
                    "Ribbon map: {} old commits, {} new, {} ribbons; up and down step through \
                     the ribbons",
                    data.old.len(),
                    data.new.len(),
                    data.ribbons.len()
                ),
            );
            let gap = (width * 0.3).clamp(80.0, 280.0);
            let col_w = ((width - gap - 8.0) / 2.0).max(120.0);
            let cols = Columns {
                left: outer.left() + 4.0,
                right: outer.right() - col_w - 4.0,
                width: col_w,
                top: outer.top() + 4.0,
            };
            let visible = viewport.translate(outer.min.to_vec2());
            let painter = ui.painter_at(visible);
            let in_view = |r: &Ribbon| {
                let (a, b) = (cols.old_box(r.old), cols.new_box(r.new));
                a.top().min(b.top()) <= visible.bottom()
                    && a.bottom().max(b.bottom()) >= visible.top()
            };
            let pointer = resp.hover_pos();
            // What the pointer is over: a commit first, else the nearest ribbon close by.
            let over_box = pointer.and_then(|p| {
                let row = ((p.y - cols.top) / ROW_H).floor();
                if row < 0.0 {
                    return None;
                }
                let row = row as usize;
                if row < data.old.len() && cols.old_box(row).contains(p) {
                    Some(Pick::Old(row))
                } else if row < data.new.len() && cols.new_box(row).contains(p) {
                    Some(Pick::New(row))
                } else {
                    None
                }
            });
            let curves: Vec<Option<Vec<Pos2>>> = data
                .ribbons
                .iter()
                .map(|r| in_view(r).then(|| cols.curve(r, progress)))
                .collect();
            let over_ribbon = match (pointer, over_box) {
                (Some(p), None) => curves
                    .iter()
                    .enumerate()
                    .filter_map(|(i, c)| Some((i, distance(p, c.as_ref()?))))
                    .filter(|(_, d)| *d <= 6.0)
                    .min_by(|a, b| a.1.total_cmp(&b.1))
                    .map(|(i, _)| Pick::Ribbon(i)),
                _ => None,
            };
            let hovered = over_box.or(over_ribbon);
            let focus = selected.or(hovered);
            // The ribbons of what is selected or hovered stand out; the others fade.
            let lit = |i: usize, r: &Ribbon| match focus {
                None => true,
                Some(Pick::Ribbon(j)) => i == j,
                Some(Pick::Old(o)) => r.old == o,
                Some(Pick::New(n)) => r.new == n,
            };
            for (i, (r, curve)) in data.ribbons.iter().zip(&curves).enumerate() {
                let Some(points) = curve else {
                    continue;
                };
                let base = color(r.class, &t);
                let c = if lit(i, r) {
                    base
                } else {
                    base.gamma_multiply(0.3)
                };
                let width = if r.evidence == Evidence::Observed {
                    8.0
                } else {
                    6.0
                };
                band(&painter, points, width, common::tint(c, 0.35, &t));
                let line = if Some(Pick::Ribbon(i)) == selected {
                    2.5
                } else {
                    1.5
                };
                common::evidence_segment(&painter, points, r.evidence, Stroke::new(line, c));
            }
            let first = ((visible.top() - cols.top) / ROW_H).floor().max(0.0) as usize;
            let last = (((visible.bottom() - cols.top) / ROW_H).ceil() as usize + 1).min(rows);
            for row in first..last {
                if let Some(end) = data.old.get(row) {
                    let on = matches!(focus, Some(Pick::Old(o)) if o == row);
                    commit_box(ui, &painter, cols.old_box(row), end, true, on, &t);
                }
                if let Some(end) = data.new.get(row) {
                    let on = matches!(focus, Some(Pick::New(n)) if n == row);
                    commit_box(ui, &painter, cols.new_box(row), end, false, on, &t);
                }
            }
            if let Some(h) = hovered {
                let tip = tooltip(data, h, sides);
                resp.clone().on_hover_text_at_pointer(tip);
            }
            if hovered.is_some() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            }
            if resp.clicked() {
                resp.request_focus();
                picked.pick = hovered;
                picked.commit = match hovered {
                    Some(Pick::Old(i)) => data.old.get(i).map(|e| e.commit.hash.clone()),
                    Some(Pick::New(i)) => data.new.get(i).map(|e| e.commit.hash.clone()),
                    _ => None,
                };
            }
            if resp.has_focus() && !data.ribbons.is_empty() {
                let step = ui.input(|i| {
                    if i.key_pressed(Key::ArrowDown) {
                        1
                    } else if i.key_pressed(Key::ArrowUp) {
                        -1
                    } else {
                        0
                    }
                });
                if step != 0 {
                    let order = in_order(data);
                    let at = match selected {
                        Some(Pick::Ribbon(i)) => order.iter().position(|&j| j == i),
                        _ => None,
                    };
                    let next = match at {
                        Some(k) => (k as i64 + step).clamp(0, order.len() as i64 - 1) as usize,
                        None if step > 0 => 0,
                        None => order.len() - 1,
                    };
                    let i = order[next];
                    if Some(Pick::Ribbon(i)) != selected {
                        picked.pick = Some(Pick::Ribbon(i));
                        let r = &data.ribbons[i];
                        let rect = cols.old_box(r.old).union(cols.new_box(r.new));
                        ui.scroll_to_rect(rect, None);
                    }
                }
                ui.painter().rect_stroke(
                    visible.shrink(1.0),
                    2.0,
                    Stroke::new(1.0, t.accent),
                    egui::StrokeKind::Inside,
                );
            }
        });
    picked
}

/// The ribbons top to bottom: by the new commit, then the old one.
fn in_order(data: &Ribbons) -> Vec<usize> {
    let mut order: Vec<usize> = (0..data.ribbons.len()).collect();
    order.sort_by_key(|&i| (data.ribbons[i].new, data.ribbons[i].old));
    order
}

/// A band `width` thick along `points`, filled with `fill`.
fn band(painter: &egui::Painter, points: &[Pos2], width: f32, fill: Color32) {
    if points.len() < 2 {
        return;
    }
    let mut mesh = egui::Mesh::default();
    for p in points {
        mesh.colored_vertex(*p - vec2(0.0, width / 2.0), fill);
        mesh.colored_vertex(*p + vec2(0.0, width / 2.0), fill);
    }
    for k in 0..points.len() as u32 - 1 {
        let i = 2 * k;
        mesh.add_triangle(i, i + 1, i + 2);
        mesh.add_triangle(i + 1, i + 3, i + 2);
    }
    painter.add(Shape::mesh(mesh));
}

/// How far `p` is from the line through `points`.
fn distance(p: Pos2, points: &[Pos2]) -> f32 {
    points
        .windows(2)
        .map(|w| {
            let (a, b) = (w[0], w[1]);
            let ab = b - a;
            let len = ab.length_sq();
            let s = if len > 0.0 {
                ((p - a).dot(ab) / len).clamp(0.0, 1.0)
            } else {
                0.0
            };
            (a + ab * s).distance(p)
        })
        .fold(f32::INFINITY, f32::min)
}

/// One commit: its letter on the side the ribbons leave from, its place, hash and summary.
fn commit_box(
    ui: &Ui,
    painter: &egui::Painter,
    rect: Rect,
    end: &End,
    old: bool,
    on: bool,
    t: &Tokens,
) {
    let c = color(end.class, t);
    painter.rect_filled(rect, 3.0, if on { t.row_selected } else { t.card });
    let stroke = if on {
        Stroke::new(2.0, t.accent)
    } else {
        Stroke::new(1.0, c.gamma_multiply(0.7))
    };
    painter.rect_stroke(rect, 3.0, stroke, egui::StrokeKind::Inside);
    let glyph = if old {
        Rect::from_min_size(
            pos2(rect.right() - 20.0, rect.top() + 4.0),
            vec2(16.0, 16.0),
        )
    } else {
        Rect::from_min_size(pos2(rect.left() + 4.0, rect.top() + 4.0), vec2(16.0, 16.0))
    };
    common::paint_mark(ui, glyph, Mark::Glyph(end.class.glyph(), c));
    let text = if old {
        Rect::from_min_max(
            rect.min + vec2(8.0, 0.0),
            pos2(glyph.left() - 4.0, rect.bottom()),
        )
    } else {
        Rect::from_min_max(
            pos2(glyph.right() + 6.0, rect.top()),
            rect.max - vec2(6.0, 0.0),
        )
    };
    let moved = if end.moved { "↕ " } else { "" };
    let color = if end.commit.empty {
        t.text_faint
    } else {
        t.text
    };
    painter
        .with_clip_rect(text.intersect(painter.clip_rect()))
        .text(
            text.left_center(),
            Align2::LEFT_CENTER,
            format!("{moved}{}", named(&end.commit)),
            theme::mono(11.5),
            color,
        );
}

/// What hovering `pick` says.
fn tooltip(data: &Ribbons, pick: Pick, sides: bool) -> String {
    match pick {
        Pick::Ribbon(i) => {
            let Some(r) = data.ribbons.get(i) else {
                return String::new();
            };
            let moved = if r.moved { ", in another place" } else { "" };
            format!(
                "{}{moved}: {}\n{}\n→ {}\n{}: {}\nClick to see its patches",
                label(r.class, sides),
                r.class.meaning(),
                named(&data.old[r.old].commit),
                named(&data.new[r.new].commit),
                r.evidence.label(),
                r.why
            )
        }
        Pick::Old(i) | Pick::New(i) => {
            let list = if matches!(pick, Pick::Old(_)) {
                &data.old
            } else {
                &data.new
            };
            let Some(end) = list.get(i) else {
                return String::new();
            };
            format!(
                "{}\n{}: {}\nClick to select the commit and see its patch",
                named(&end.commit),
                label(end.class, sides),
                end.class.meaning()
            )
        }
    }
}
