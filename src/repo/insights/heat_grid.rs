//! A grid of cells (rows by columns) for the heatmaps and matrices: only the cells in view are
//! drawn, the row labels stay on the left and the column labels on top while it scrolls, and
//! the keyboard moves a cursor over the cells (arrows, Home/End) and opens one with Enter.

use egui::{Align2, Color32, Id, Key, Pos2, Rect, Sense, Stroke, Ui, pos2, vec2};

use super::common::{self, Mark};
use crate::theme::{self, Tokens};

/// One cell: its colour, a mark over it that says the same as the colour does (or more), and
/// what its tooltip says.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Cell {
    pub(crate) fill: Color32,
    pub(crate) mark: Option<Mark>,
    pub(crate) tip: String,
}

/// How the grid is laid out.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Layout {
    pub(crate) cell: egui::Vec2,
    /// Width of the row labels.
    pub(crate) label_w: f32,
    /// Height of the column labels (drawn slanted).
    pub(crate) header_h: f32,
}

impl Default for Layout {
    fn default() -> Self {
        Self {
            cell: vec2(16.0, 16.0),
            label_w: 220.0,
            header_h: 70.0,
        }
    }
}

/// What happened in the grid this frame.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Picked {
    /// A cell clicked or opened with Enter: (row, column).
    pub(crate) cell: Option<(usize, usize)>,
    pub(crate) row: Option<usize>,
    pub(crate) column: Option<usize>,
}

/// The grid of `rows` by `columns`; `cell` gives each cell (`None`: empty), `row_label` and
/// `column_label` their labels (the full text is in their tooltips). `selected`: the cell
/// marked as selected.
#[allow(clippy::too_many_arguments)]
pub(crate) fn show(
    ui: &mut Ui,
    salt: impl std::hash::Hash + std::fmt::Debug,
    layout: Layout,
    (rows, columns): (usize, usize),
    row_label: impl Fn(usize) -> String,
    column_label: impl Fn(usize) -> String,
    cell: impl Fn(usize, usize) -> Option<Cell>,
    selected: Option<(usize, usize)>,
) -> Picked {
    let t = Tokens::get(ui.ctx());
    let id = Id::new(("heat-grid", salt));
    let mut picked = Picked::default();
    let size = vec2(
        layout.label_w + columns as f32 * layout.cell.x + 8.0,
        layout.header_h + rows as f32 * layout.cell.y + 8.0,
    );
    let cursor_id = id.with("cursor");
    let mut cursor: Option<(usize, usize)> = ui.data(|d| d.get_temp(cursor_id)).or(selected);
    egui::ScrollArea::both()
        .id_salt(id)
        .auto_shrink([false, false])
        .show_viewport(ui, |ui, viewport| {
            let (outer, resp) = ui.allocate_exact_size(size, Sense::click());
            let resp = resp.on_hover_cursor(egui::CursorIcon::PointingHand);
            crate::a11y::a11y(
                &resp,
                egui::WidgetType::Other,
                format!("Grid of {rows} rows and {columns} columns; arrows move, Enter opens"),
            );
            let origin = outer.min;
            let visible = viewport.translate(origin.to_vec2());
            let cells_left = visible.left() + layout.label_w;
            let cells_top = visible.top() + layout.header_h;
            // Which cell is where, with the labels pinned to the visible edges.
            let x_of = |c: usize| origin.x + layout.label_w + c as f32 * layout.cell.x;
            let y_of = |r: usize| origin.y + layout.header_h + r as f32 * layout.cell.y;
            let first_col = ((visible.left() - origin.x) / layout.cell.x)
                .floor()
                .max(0.0) as usize;
            let last_col = (((visible.right() - origin.x - layout.label_w) / layout.cell.x).ceil()
                as usize
                + 1)
            .min(columns);
            let first_row = ((visible.top() - origin.y) / layout.cell.y)
                .floor()
                .max(0.0) as usize;
            let last_row =
                (((visible.bottom() - origin.y - layout.header_h) / layout.cell.y).ceil() as usize
                    + 1)
                .min(rows);
            let painter = ui.painter_at(visible);
            let cell_area = Rect::from_min_max(pos2(cells_left, cells_top), visible.max);
            let cells_painter = painter.with_clip_rect(cell_area);
            let at = |pos: Pos2| -> Option<(usize, usize)> {
                if !cell_area.contains(pos) {
                    return None;
                }
                let c = ((pos.x - origin.x - layout.label_w) / layout.cell.x).floor();
                let r = ((pos.y - origin.y - layout.header_h) / layout.cell.y).floor();
                (c >= 0.0 && r >= 0.0 && (c as usize) < columns && (r as usize) < rows)
                    .then_some((r as usize, c as usize))
            };
            for r in first_row..last_row {
                for c in first_col..last_col {
                    let rect = Rect::from_min_size(pos2(x_of(c), y_of(r)), layout.cell).shrink(1.0);
                    match cell(r, c) {
                        Some(found) => {
                            cells_painter.rect_filled(rect, 2.0, found.fill);
                            if let Some(mark) = found.mark {
                                paint_in(&cells_painter, rect.shrink(2.0), mark);
                            }
                        }
                        None => {
                            cells_painter.rect_filled(rect, 2.0, t.field.gamma_multiply(0.5));
                        }
                    }
                }
            }
            let ring = |rc: (usize, usize), color: Color32, width: f32| {
                let rect =
                    Rect::from_min_size(pos2(x_of(rc.1), y_of(rc.0)), layout.cell).shrink(0.5);
                cells_painter.rect_stroke(
                    rect,
                    2.0,
                    Stroke::new(width, color),
                    egui::StrokeKind::Outside,
                );
            };
            if let Some(rc) = selected {
                ring(rc, t.accent, 2.0);
            }
            // The row labels, pinned on the left.
            let labels = painter.with_clip_rect(Rect::from_min_max(
                pos2(visible.left(), cells_top),
                pos2(cells_left, visible.bottom()),
            ));
            labels.rect_filled(
                Rect::from_min_max(
                    pos2(visible.left(), cells_top),
                    pos2(cells_left, visible.bottom()),
                ),
                0.0,
                t.dock,
            );
            for r in first_row..last_row {
                let y = y_of(r) + layout.cell.y / 2.0;
                let text = row_label(r);
                let galley = labels.layout_no_wrap(
                    text,
                    egui::FontId::proportional((layout.cell.y - 4.0).clamp(9.0, 12.0)),
                    if selected.is_some_and(|s| s.0 == r) {
                        t.text
                    } else {
                        t.text_dim
                    },
                );
                let x = (cells_left - 6.0 - galley.size().x).max(visible.left() + 4.0);
                labels.galley(pos2(x, y - galley.size().y / 2.0), galley, t.text_dim);
            }
            // The column labels, pinned on top and slanted.
            let header = Rect::from_min_max(
                pos2(cells_left, visible.top()),
                pos2(visible.right(), cells_top),
            );
            let tops = painter.with_clip_rect(header);
            tops.rect_filled(header, 0.0, t.dock);
            for c in first_col..last_col {
                let x = x_of(c) + layout.cell.x / 2.0;
                let text = column_label(c);
                let galley = tops.layout_no_wrap(
                    text,
                    egui::FontId::proportional(10.5),
                    if selected.is_some_and(|s| s.1 == c) {
                        t.text
                    } else {
                        t.text_faint
                    },
                );
                let angle = -std::f32::consts::FRAC_PI_4;
                let pos = pos2(x - 2.0, cells_top - 4.0);
                tops.add(
                    egui::epaint::TextShape::new(pos, galley, t.text_faint)
                        .with_angle_and_anchor(angle, Align2::LEFT_BOTTOM),
                );
            }
            // The corner, over both.
            painter.rect_filled(
                Rect::from_min_max(visible.min, pos2(cells_left, cells_top)),
                0.0,
                t.dock,
            );
            // Pointer: hover shows the cell's tooltip; clicking opens it, or a row or column
            // label picks the row or column.
            let pointer = resp.hover_pos();
            let hovered = pointer.and_then(at);
            if let Some((r, c)) = hovered {
                ring((r, c), t.text_dim, 1.0);
                let tip = cell(r, c).map(|found| found.tip).unwrap_or_default();
                let head = format!("{} · {}", row_label(r), column_label(c));
                resp.clone().on_hover_ui_at_pointer(|ui| {
                    ui.label(egui::RichText::new(head).font(theme::medium(12.0)));
                    if !tip.is_empty() {
                        ui.label(tip);
                    }
                });
            } else if let Some(pos) = pointer {
                if pos.x < cells_left && pos.y >= cells_top {
                    let r = ((pos.y - origin.y - layout.header_h) / layout.cell.y).floor();
                    if r >= 0.0 && (r as usize) < rows {
                        resp.clone().on_hover_text_at_pointer(row_label(r as usize));
                        if resp.clicked() {
                            picked.row = Some(r as usize);
                        }
                    }
                } else if pos.y < cells_top && pos.x >= cells_left {
                    let c = ((pos.x - origin.x - layout.label_w) / layout.cell.x).floor();
                    if c >= 0.0 && (c as usize) < columns {
                        resp.clone()
                            .on_hover_text_at_pointer(column_label(c as usize));
                        if resp.clicked() {
                            picked.column = Some(c as usize);
                        }
                    }
                }
            }
            if resp.clicked()
                && let Some(rc) = hovered
            {
                picked.cell = Some(rc);
                cursor = Some(rc);
                resp.request_focus();
            }
            // Keyboard: arrows move the cursor, Enter opens the cell under it.
            if resp.has_focus() && rows > 0 && columns > 0 {
                let (mut r, mut c) = cursor.unwrap_or((0, 0));
                let moved = ui.input(|i| {
                    let mut moved = false;
                    for (key, dr, dc) in [
                        (Key::ArrowUp, -1, 0),
                        (Key::ArrowDown, 1, 0),
                        (Key::ArrowLeft, 0, -1),
                        (Key::ArrowRight, 0, 1),
                    ] {
                        if i.key_pressed(key) {
                            r = (r as i64 + dr).clamp(0, rows as i64 - 1) as usize;
                            c = (c as i64 + dc).clamp(0, columns as i64 - 1) as usize;
                            moved = true;
                        }
                    }
                    if i.key_pressed(Key::Home) {
                        c = 0;
                        moved = true;
                    }
                    if i.key_pressed(Key::End) {
                        c = columns - 1;
                        moved = true;
                    }
                    moved
                });
                cursor = Some((r, c));
                if moved {
                    let rect = Rect::from_min_size(pos2(x_of(c), y_of(r)), layout.cell);
                    ui.scroll_to_rect(rect.expand2(vec2(layout.label_w, layout.header_h)), None);
                }
                ring((r, c), t.accent_fg, 2.0);
                if ui.input(|i| i.key_pressed(Key::Enter)) {
                    picked.cell = Some((r, c));
                }
                let tip = cell(r, c).map(|found| found.tip).unwrap_or_default();
                let name = format!("{} · {}: {tip}", row_label(r), column_label(c));
                resp.widget_info(|| {
                    egui::WidgetInfo::labeled(egui::WidgetType::Other, true, name.clone())
                });
            }
        });
    ui.data_mut(|d| match cursor {
        Some(rc) => {
            d.insert_temp(cursor_id, rc);
        }
        None => d.remove::<(usize, usize)>(cursor_id),
    });
    picked
}

/// A cell's mark, through the grid's clipped painter: a hatch, a letter, an outline, or a dot.
fn paint_in(painter: &egui::Painter, rect: Rect, mark: Mark) {
    match mark {
        Mark::Hatched(c) => common::hatch(painter, rect, c),
        Mark::Glyph(text, c) => {
            painter.text(
                rect.center(),
                Align2::CENTER_CENTER,
                text,
                theme::semibold(rect.height().clamp(7.0, 12.0)),
                c,
            );
        }
        Mark::Hollow(c) => {
            painter.rect_stroke(rect, 1.0, Stroke::new(1.5, c), egui::StrokeKind::Inside);
        }
        Mark::Fill(c) | Mark::Line(c, _) | Mark::Dotted(c) => {
            painter.circle_filled(rect.center(), rect.width().min(rect.height()) / 4.0, c);
        }
    }
}
