//! The matrix itself and the landing order under it.

use egui::{Align2, CornerRadius, Rect, Sense, Ui, pos2, vec2};

use super::compare;
use crate::collision::{Cell, Matrix, Step};
use crate::repo::Action;
use crate::theme::{self, Tokens};

const CELL: f32 = 30.0;
/// Width of the names on the left.
const NAMES: f32 = 190.0;

/// The matrix, then the suggested order.
pub(super) fn results(ui: &mut Ui, m: &Matrix, order: &[Step], acts: &mut Vec<Action>) {
    let t = Tokens::get(ui.ctx());
    if let Some(e) = m.first_error() {
        ui.colored_label(
            readable(&t, t.warning),
            format!("Some pairs couldn't be merged in memory: {e}"),
        );
        ui.add_space(6.0);
    }
    egui::ScrollArea::both()
        .id_salt("collision-grid")
        .max_height(300.0)
        .show(ui, |ui| matrix(ui, m, acts));
    ui.label(
        egui::RichText::new(format!(
            "→ is each branch merged into {}. A number is how many files conflict: hover for \
             them, click to compare the two.",
            m.target
        ))
        .size(11.5)
        .color(t.text_faint),
    );
    ui.add_space(12.0);
    ui.label(egui::RichText::new("Suggested landing order").font(theme::semibold(13.0)));
    ui.add_space(4.0);
    landing(ui, m, order);
}

/// Rows and columns numbered as the branches, the target's column last.
fn matrix(ui: &mut Ui, m: &Matrix, acts: &mut Vec<Action>) {
    let t = Tokens::get(ui.ctx());
    let n = m.branches.len();
    let width = NAMES + CELL * (n as f32 + 1.0) + 8.0;
    let (rect, _) = ui.allocate_exact_size(vec2(width, CELL * (n as f32 + 1.0)), Sense::hover());
    let painter = ui.painter_at(rect);
    let col_x = |j: usize| rect.left() + NAMES + CELL * j as f32;
    for j in 0..=n {
        let label = if j == n {
            "→".to_owned()
        } else {
            (j + 1).to_string()
        };
        painter.text(
            pos2(col_x(j) + CELL / 2.0, rect.top() + CELL / 2.0),
            Align2::CENTER_CENTER,
            label,
            theme::semibold(11.5),
            t.text_dim,
        );
    }
    let target_head = Rect::from_min_size(pos2(col_x(n), rect.top()), vec2(CELL, CELL));
    ui.interact(
        target_head,
        ui.id().with("collision-target-head"),
        Sense::hover(),
    )
    .on_hover_text(format!("Merged into {}", m.target));
    for i in 0..n {
        let y = rect.top() + CELL * (i as f32 + 1.0);
        let name = format!("{}  {}", i + 1, m.branches[i]);
        painter
            .with_clip_rect(Rect::from_min_size(
                pos2(rect.left(), y),
                vec2(NAMES - 6.0, CELL),
            ))
            .text(
                pos2(rect.left(), y + CELL / 2.0),
                Align2::LEFT_CENTER,
                name,
                egui::FontId::proportional(12.0),
                t.text,
            );
        for j in 0..=n {
            let cell_rect = Rect::from_min_size(pos2(col_x(j), y), vec2(CELL, CELL)).shrink(2.0);
            if j == i {
                continue;
            }
            let (cell, other) = if j == n {
                (m.with_target.get(i), m.target.as_str())
            } else {
                (
                    m.cells
                        .get(i)
                        .and_then(|r| r.get(j))
                        .and_then(Option::as_ref),
                    m.branches[j].as_str(),
                )
            };
            let Some(cell) = cell else {
                continue;
            };
            let id = ui.id().with(("collision-cell", i, j));
            let r = paint_cell(ui, cell_rect, cell, id).on_hover_text(cell_tip(
                &m.branches[i],
                other,
                cell,
            ));
            if r.clicked() && cell.as_ref().is_ok_and(|p| !p.clean()) {
                acts.push(compare(other, &m.branches[i]));
            }
        }
    }
}

fn paint_cell(ui: &mut Ui, rect: Rect, cell: &Cell, id: egui::Id) -> egui::Response {
    let t = Tokens::get(ui.ctx());
    let (fill, text, color) = match cell {
        Ok(pair) if pair.clean() => (t.success.gamma_multiply(0.18), String::new(), t.success),
        Ok(pair) => (
            t.danger.gamma_multiply(0.25),
            pair.files.len().to_string(),
            readable(&t, t.danger),
        ),
        Err(_) => (
            t.warning.gamma_multiply(0.18),
            "?".to_owned(),
            readable(&t, t.warning),
        ),
    };
    let painter = ui.painter();
    painter.rect_filled(rect, CornerRadius::same(4), fill);
    painter.text(
        rect.center(),
        Align2::CENTER_CENTER,
        text,
        theme::semibold(11.5),
        color,
    );
    let sense = if matches!(cell, Ok(p) if !p.clean()) {
        Sense::click()
    } else {
        Sense::hover()
    };
    ui.interact(rect, id, sense)
}

/// Most conflicted files a cell's hover lists.
const LISTED: usize = 12;

fn cell_tip(a: &str, b: &str, cell: &Cell) -> String {
    match cell {
        Ok(pair) if pair.clean() => format!("{a} and {b} merge cleanly"),
        Ok(pair) => {
            let n = pair.files.len();
            let mut tip = format!(
                "{a} and {b} conflict on {n} file{}:",
                if n == 1 { "" } else { "s" }
            );
            for f in pair.files.iter().take(LISTED) {
                tip.push_str(&format!("\n  {f}"));
            }
            if n > LISTED {
                tip.push_str(&format!("\n  and {} more", n - LISTED));
            }
            tip.push_str("\nClick to compare them");
            tip
        }
        Err(e) => e.clone(),
    }
}

/// The order, each with what it would hit.
fn landing(ui: &mut Ui, m: &Matrix, order: &[Step]) {
    let t = Tokens::get(ui.ctx());
    for (at, step) in order.iter().enumerate() {
        let name = &m.branches[step.branch];
        let note = step_note(m, step);
        ui.horizontal_wrapped(|ui| {
            ui.label(egui::RichText::new(format!("{}.", at + 1)).color(t.text_faint));
            ui.label(egui::RichText::new(name).font(theme::medium(12.5)));
            let color = if step.clean() {
                readable(&t, t.success)
            } else if step.with_target == 0 && step.with_earlier.is_empty() {
                readable(&t, t.warning)
            } else {
                readable(&t, t.danger)
            };
            ui.label(egui::RichText::new(note).size(12.0).color(color));
        });
    }
}

/// "lands cleanly", or what the branch conflicts with by then.
pub(super) fn step_note(m: &Matrix, step: &Step) -> String {
    if step.clean() {
        return "merges cleanly with each one before it".to_owned();
    }
    let mut parts: Vec<String> = Vec::new();
    if step.with_target > 0 {
        parts.push(format!("{} ({})", m.target, files(step.with_target)));
    }
    for &(other, n) in &step.with_earlier {
        parts.push(format!("{} ({})", m.branches[other], files(n)));
    }
    let unknown: Vec<&str> = step
        .unknown
        .iter()
        .map(|u| u.map_or(m.target.as_str(), |i| m.branches[i].as_str()))
        .collect();
    let mut note = Vec::new();
    if !parts.is_empty() {
        note.push(format!("conflicts with {}", parts.join(", ")));
    }
    if !unknown.is_empty() {
        note.push(format!(
            "couldn't be merged with {} in memory",
            unknown.join(", ")
        ));
    }
    note.join("; ")
}

/// `color` darkened on light themes, so small text on its own tint stays readable.
fn readable(t: &Tokens, color: egui::Color32) -> egui::Color32 {
    if t.dark() {
        return color;
    }
    let [r, g, b, _] = color.to_array();
    let dim = |c: u8| (f32::from(c) * 0.7) as u8;
    egui::Color32::from_rgb(dim(r), dim(g), dim(b))
}

fn files(n: usize) -> String {
    format!("{n} file{}", if n == 1 { "" } else { "s" })
}
