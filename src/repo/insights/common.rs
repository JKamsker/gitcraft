//! What the insights views share: reading in the background without an older answer replacing
//! a newer one, the bar saying where the data came from, legends whose marks differ by shape
//! and not only by colour, the colours of a heat scale, and what a view says while it reads,
//! when it failed, and when what it needs isn't there.

use std::sync::atomic::{AtomicU64, Ordering};

use egui::{Color32, Pos2, Rect, RichText, Sense, Shape, Stroke, Ui, pos2, vec2};

use crate::insights::{Evidence, Provenance};
use crate::repo::short;
use crate::theme::{self, Tokens};

static REQUESTS: AtomicU64 = AtomicU64::new(0);

/// A number no other read of any view has.
pub(crate) fn next_request() -> u64 {
    REQUESTS.fetch_add(1, Ordering::Relaxed) + 1
}

/// Something a view reads in the background: nothing yet, being read, or what came of it.
/// Only the answer to the latest read is taken.
pub(crate) struct Slot<T> {
    request: u64,
    pub(crate) loading: bool,
    pub(crate) value: Option<Result<T, String>>,
}

impl<T> Default for Slot<T> {
    fn default() -> Self {
        Self {
            request: 0,
            loading: false,
            value: None,
        }
    }
}

impl<T> Slot<T> {
    /// A read starts: its number, to give [`Self::accept`] with its answer. What was read
    /// before stays shown meanwhile.
    pub(crate) fn start(&mut self) -> u64 {
        self.request = next_request();
        self.loading = true;
        self.request
    }

    /// A read of something else starts: what was read before is dropped, not shown meanwhile
    /// as if it were the answer.
    pub(crate) fn restart(&mut self) -> u64 {
        self.value = None;
        self.start()
    }

    /// The answer to read `request`, taken when it is the latest. Whether it was.
    pub(crate) fn accept(&mut self, request: u64, value: Result<T, String>) -> bool {
        if request != self.request {
            return false;
        }
        self.loading = false;
        self.value = Some(value);
        true
    }

    pub(crate) fn ready(&self) -> Option<&T> {
        self.value.as_ref().and_then(|v| v.as_ref().ok())
    }

    pub(crate) fn ready_mut(&mut self) -> Option<&mut T> {
        self.value.as_mut().and_then(|v| v.as_mut().ok())
    }

    pub(crate) fn error(&self) -> Option<&str> {
        self.value
            .as_ref()
            .and_then(|v| v.as_ref().err())
            .map(String::as_str)
    }
}

/// What a view draws of a slot that has no data to show: a spinner while it's read, its error
/// (`true` when Retry was clicked), or nothing (`false`) once there is data.
pub(crate) fn slot_state<T>(ui: &mut Ui, slot: &Slot<T>, what: &str) -> SlotState {
    match &slot.value {
        None if slot.loading => {
            loading(ui, what);
            SlotState::Waiting
        }
        None => SlotState::Waiting,
        Some(Err(e)) => {
            if failed(ui, what, e) {
                SlotState::Retry
            } else {
                SlotState::Waiting
            }
        }
        Some(Ok(_)) => SlotState::Ready,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SlotState {
    /// Nothing to draw yet (or an error was drawn).
    Waiting,
    /// The error's Retry was clicked.
    Retry,
    /// There is data: draw it.
    Ready,
}

/// A spinner, and what is being read.
pub(crate) fn loading(ui: &mut Ui, what: &str) {
    let t = Tokens::get(ui.ctx());
    ui.add_space(30.0);
    ui.vertical_centered(|ui| {
        ui.add(crate::spinner::Spinner::new());
        ui.add_space(6.0);
        ui.label(RichText::new(format!("Reading {what}…")).color(t.text_dim));
    });
}

/// A failed read, and a button to try again: whether it was clicked.
pub(crate) fn failed(ui: &mut Ui, what: &str, error: &str) -> bool {
    let t = Tokens::get(ui.ctx());
    let mut retry = false;
    ui.add_space(20.0);
    ui.vertical_centered(|ui| {
        ui.add(
            egui::Label::new(
                RichText::new(format!("Couldn't read {what}: {error}")).color(t.danger),
            )
            .wrap(),
        );
        ui.add_space(6.0);
        retry = ui.button("Retry").clicked();
    });
    retry
}

/// What a view needs and doesn't have (no forge, no coverage report, an unsupported
/// language): said plainly, with what would make it available.
pub(crate) fn unavailable(ui: &mut Ui, title: &str, why: &str) {
    let t = Tokens::get(ui.ctx());
    ui.add_space(24.0);
    ui.vertical_centered(|ui| {
        ui.label(
            RichText::new(title)
                .font(theme::medium(13.5))
                .color(t.text_dim),
        );
        ui.add_space(4.0);
        ui.add(egui::Label::new(RichText::new(why).color(t.text_faint)).wrap());
    });
}

/// Nothing to show, said so.
pub(crate) fn empty(ui: &mut Ui, text: &str) {
    let t = Tokens::get(ui.ctx());
    ui.add_space(20.0);
    ui.vertical_centered(|ui| ui.label(RichText::new(text).color(t.text_dim)));
}

/// The line saying where what's shown came from: the source, what it covers, the tips it was
/// read at, when; and in warning colours, why it is incomplete or stale.
pub(crate) fn provenance(ui: &mut Ui, p: &Provenance) {
    let t = Tokens::get(ui.ctx());
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        let faint = |s: String| RichText::new(s).size(11.5).color(t.text_faint);
        ui.label(faint(format!("{} · {}", p.source, p.scope)));
        for (name, sha) in &p.tips {
            ui.label(faint(format!("{name} @ {}", short(sha))))
                .on_hover_text(sha);
        }
        if p.collected > 0 {
            let when = crate::git::relative_time(p.collected, crate::git::now());
            ui.label(faint(format!("read {when}")));
        }
        if let Some(why) = &p.partial {
            crate::repo::widgets::chip(ui, "partial", t.warning).on_hover_text(why);
        }
        if let Some(why) = &p.stale {
            crate::repo::widgets::chip(ui, "stale", t.danger)
                .on_hover_text(format!("{why}: read again for what is there now"));
        }
    });
}

/// How a legend's entry, a node or a cell is marked: never by colour alone.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Mark {
    /// A filled square.
    Fill(Color32),
    /// An outlined square.
    Hollow(Color32),
    /// A square hatched with diagonal lines.
    Hatched(Color32),
    /// A line, solid or dashed.
    Line(Color32, bool),
    /// A dotted line.
    Dotted(Color32),
    /// A short text (a letter, `?`, `!`) on a tinted square.
    Glyph(&'static str, Color32),
}

/// A mark's line style, by how what it stands for is known: solid, dashed or dotted.
pub(crate) fn evidence_line(evidence: Evidence, color: Color32) -> Mark {
    match evidence {
        Evidence::Observed => Mark::Line(color, false),
        Evidence::Inferred => Mark::Line(color, true),
        Evidence::Unknown => Mark::Dotted(color),
    }
}

/// Paints `mark` in `rect`.
pub(crate) fn paint_mark(ui: &Ui, rect: Rect, mark: Mark) {
    let t = Tokens::get(ui.ctx());
    let painter = ui.painter();
    match mark {
        Mark::Fill(c) => {
            painter.rect_filled(rect, 2.0, c);
        }
        Mark::Hollow(c) => {
            painter.rect_stroke(
                rect.shrink(0.5),
                2.0,
                Stroke::new(1.5, c),
                egui::StrokeKind::Inside,
            );
        }
        Mark::Hatched(c) => {
            painter.rect_stroke(
                rect.shrink(0.5),
                2.0,
                Stroke::new(1.0, c),
                egui::StrokeKind::Inside,
            );
            hatch(painter, rect, c);
        }
        Mark::Line(c, dashed) => {
            let y = rect.center().y;
            let (a, b) = (pos2(rect.left(), y), pos2(rect.right(), y));
            if dashed {
                painter.extend(Shape::dashed_line(&[a, b], Stroke::new(2.0, c), 4.0, 3.0));
            } else {
                painter.line_segment([a, b], Stroke::new(2.0, c));
            }
        }
        Mark::Dotted(c) => {
            let y = rect.center().y;
            painter.extend(Shape::dotted_line(
                &[pos2(rect.left(), y), pos2(rect.right(), y)],
                c,
                4.0,
                1.2,
            ));
        }
        Mark::Glyph(text, c) => {
            painter.rect_filled(rect, 2.0, tint(c, 0.25, &t));
            painter.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                text,
                theme::semibold((rect.height() - 2.0).max(8.0)),
                c,
            );
        }
    }
}

/// Diagonal lines across `rect`, clipped to it.
pub(crate) fn hatch(painter: &egui::Painter, rect: Rect, color: Color32) {
    let painter = painter.with_clip_rect(rect);
    let step = 4.0;
    let mut x = rect.left() - rect.height();
    while x < rect.right() {
        painter.line_segment(
            [pos2(x, rect.bottom()), pos2(x + rect.height(), rect.top())],
            Stroke::new(1.0, color),
        );
        x += step;
    }
}

/// A line from `a` to `b` drawn as `evidence` says: solid, dashed or dotted.
pub(crate) fn evidence_segment(
    painter: &egui::Painter,
    points: &[Pos2],
    evidence: Evidence,
    stroke: Stroke,
) {
    match evidence {
        Evidence::Observed => {
            painter.add(Shape::line(points.to_vec(), stroke));
        }
        Evidence::Inferred => {
            painter.extend(Shape::dashed_line(points, stroke, 6.0, 4.0));
        }
        Evidence::Unknown => {
            painter.extend(Shape::dotted_line(
                points,
                stroke.color,
                5.0,
                stroke.width.max(1.0),
            ));
        }
    }
}

/// A legend: each mark and what it means, in a wrapped row.
pub(crate) fn legend(ui: &mut Ui, entries: &[(Mark, &str)]) {
    let t = Tokens::get(ui.ctx());
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        for (mark, label) in entries {
            let (rect, _) = ui.allocate_exact_size(vec2(16.0, 12.0), Sense::hover());
            paint_mark(ui, rect, *mark);
            ui.label(RichText::new(*label).size(11.5).color(t.text_dim));
            ui.add_space(8.0);
        }
    });
}

/// `color` at `alpha` over the theme's surfaces, readable as a fill.
pub(crate) fn tint(color: Color32, alpha: f32, t: &Tokens) -> Color32 {
    theme::palette::readable_tint(color, alpha, &[t.card, t.dock, t.field], &[color])
}

/// The colour of `value` (0 none, 1 the most) on a heat scale from the surface to the accent.
/// What a cell counts is said in its text or tooltip too.
pub(crate) fn heat(t: &Tokens, value: f32) -> Color32 {
    let v = value.clamp(0.0, 1.0);
    if v <= 0.0 {
        return t.field;
    }
    // Never quite the surface: a cell that counts something is told from one that doesn't.
    lerp(t.field, t.accent, 0.18 + 0.82 * v)
}

/// The colour of `value` on a scale from good (`-1`) through neutral (`0`) to bad (`1`).
pub(crate) fn diverging(t: &Tokens, value: f32) -> Color32 {
    let v = value.clamp(-1.0, 1.0);
    if v < 0.0 {
        lerp(t.field, t.success, 0.2 + 0.8 * -v)
    } else if v > 0.0 {
        lerp(t.field, t.danger, 0.2 + 0.8 * v)
    } else {
        t.field
    }
}

/// Text readable on `fill`.
pub(crate) fn text_on(fill: Color32) -> Color32 {
    let [r, g, b, _] = fill.to_srgba_unmultiplied();
    let luma = 0.299 * r as f32 + 0.587 * g as f32 + 0.114 * b as f32;
    if luma > 150.0 {
        Color32::from_rgb(20, 20, 24)
    } else {
        Color32::from_rgb(240, 240, 244)
    }
}

pub(crate) fn lerp(a: Color32, b: Color32, f: f32) -> Color32 {
    let f = f.clamp(0.0, 1.0);
    let mix = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * f).round() as u8;
    Color32::from_rgb(mix(a.r(), b.r()), mix(a.g(), b.g()), mix(a.b(), b.b()))
}

/// A small heading inside a view.
pub(crate) fn heading(ui: &mut Ui, text: &str) {
    let t = Tokens::get(ui.ctx());
    ui.add_space(8.0);
    ui.label(
        RichText::new(text)
            .font(theme::semibold(13.0))
            .color(t.text),
    );
    ui.add_space(2.0);
}

/// A row of a table: cells laid out at the given widths (the last takes what is left), the row
/// clickable as a whole. `header`: drawn as the table's header, not clickable.
pub(crate) fn table_row(
    ui: &mut Ui,
    widths: &[f32],
    cells: &[String],
    header: bool,
    selected: bool,
) -> egui::Response {
    let t = Tokens::get(ui.ctx());
    let h = 22.0;
    let sense = if header {
        Sense::hover()
    } else {
        Sense::click()
    };
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), h), sense);
    if !header {
        crate::a11y::a11y_selected(&resp, cells.join(", "), selected);
    }
    let painter = ui.painter_at(rect);
    if selected {
        painter.rect_filled(rect, 2.0, t.row_selected);
    } else if resp.hovered() && !header {
        painter.rect_filled(rect, 2.0, t.hover);
    }
    let mut x = rect.left() + 6.0;
    for (i, cell) in cells.iter().enumerate() {
        let w = widths
            .get(i)
            .copied()
            .unwrap_or((rect.right() - x).max(40.0));
        let font = if header {
            theme::medium(11.5)
        } else {
            egui::FontId::proportional(12.0)
        };
        let color = if header { t.text_dim } else { t.text };
        let galley = painter.layout(cell.clone(), font, color, (w - 8.0).max(10.0));
        let cell_rect = Rect::from_min_size(pos2(x, rect.top()), vec2(w - 8.0, h));
        painter.with_clip_rect(cell_rect).galley(
            pos2(x, rect.center().y - galley.size().y.min(h) / 2.0),
            galley,
            color,
        );
        x += w;
    }
    if header {
        painter.hline(
            rect.x_range(),
            rect.bottom() - 0.5,
            Stroke::new(1.0, t.separator),
        );
    }
    resp
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_slot_takes_only_the_answer_to_its_latest_read() {
        let mut slot: Slot<u32> = Slot::default();
        let first = slot.start();
        let second = slot.start();
        assert!(!slot.accept(first, Ok(1)));
        assert!(slot.loading);
        assert!(slot.accept(second, Ok(2)));
        assert_eq!(slot.ready(), Some(&2));
        assert!(!slot.loading);
        let third = slot.start();
        // What was read stays shown while it is read again.
        assert_eq!(slot.ready(), Some(&2));
        assert!(slot.accept(third, Err("offline".to_owned())));
        assert_eq!(slot.error(), Some("offline"));
    }

    #[test]
    fn heat_never_looks_like_nothing_once_it_counts_something() {
        let t = Tokens::default();
        assert_eq!(heat(&t, 0.0), t.field);
        assert_ne!(heat(&t, 0.01), t.field);
        assert_eq!(heat(&t, 1.0), lerp(t.field, t.accent, 1.0));
        assert_ne!(diverging(&t, -1.0), diverging(&t, 1.0));
    }
}
