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
            crate::i18n::message(
                "repo-workflows-collision_view-results-45512235",
                &[("e", crate::collision::display::diagnostic(e).into())],
            ),
        );
        ui.add_space(6.0);
    }
    egui::ScrollArea::both()
        .id_salt("collision-grid")
        .max_height(300.0)
        .show(ui, |ui| matrix(ui, m, acts));
    ui.label(
        egui::RichText::new(crate::i18n::message(
            "repo-workflows-collision_view-results-87f3fc81",
            &[("m-target-1", (m.target).clone().into())],
        ))
        .size(11.5)
        .color(t.text_faint),
    );
    ui.add_space(12.0);
    ui.label(
        egui::RichText::new(crate::i18n::text(
            "repo-workflows-collision_view-results-9b24a27f",
        ))
        .font(theme::semibold(13.0)),
    );
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
    .on_hover_text(crate::i18n::message(
        "repo-workflows-collision_view-matrix-f5346a60",
        &[("m-target-1", (m.target).clone().into())],
    ));
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
            crate::a11y::a11y(
                &r,
                if cell.as_ref().is_ok_and(|p| !p.clean()) {
                    egui::WidgetType::Button
                } else {
                    egui::WidgetType::Label
                },
                || cell_tip(&m.branches[i], other, cell),
            );
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
        Ok(pair) if pair.clean() => crate::i18n::message(
            "repo-workflows-collision_view-cell-tip-b3f28cca",
            &[("a", (a).clone().into()), ("b", (b).clone().into())],
        ),
        Ok(pair) => {
            let n = pair.files.len();
            let mut tip = crate::i18n::message(
                "repo-workflows-collision-conflicts",
                &[("a", (a).into()), ("b", (b).into()), ("count", (n).into())],
            );
            for f in pair.files.iter().take(LISTED) {
                tip.push_str(&format!("\n  {f}"));
            }
            if n > LISTED {
                tip.push_str(&crate::i18n::message(
                    "repo-workflows-collision_view-cell-tip-81bf5184",
                    &[("n-listed-1", (n - LISTED).clone().into())],
                ));
            }
            tip.push_str(crate::i18n::text(
                "repo-workflows-collision_view-cell-tip-9b99b61c",
            ));
            tip
        }
        Err(e) => crate::collision::display::diagnostic(e),
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
        return crate::i18n::text("repo-workflows-collision_view-step-note-cb1d05dc").to_owned();
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
        note.push(crate::i18n::message(
            "repo-workflows-collision_view-step-note-5ef5ed3f",
            &[("parts-join-1", (parts.join(", ")).clone().into())],
        ));
    }
    if !unknown.is_empty() {
        note.push(crate::i18n::message(
            "repo-workflows-collision_view-step-note-5896a7b7",
            &[("unknown-join-1", (unknown.join(", ")).clone().into())],
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
    crate::i18n::message("repo-workflows-collision-files", &[("count", (n).into())])
}

#[cfg(test)]
mod fluent_guard_tests {
    use super::*;
    #[test]
    fn fluent_partition_collision_actual_cell_tip_preserves_raw_classification() {
        let raw = "The collision matrix needs git 2.38 or later (git merge-tree --write-tree)";
        let cell: Cell = Err(raw.into());
        for lang in ["en-US", "de"] {
            crate::i18n::set_language(lang);
            let tip = cell_tip("feature/1234", "target/ß", &cell);
            assert_eq!(tip, crate::collision::display::diagnostic(raw));
            assert_eq!(tip.contains("benötigt"), lang == "de");
            assert!(cell.as_ref().is_err_and(|e| e.contains("needs git")));
            assert_eq!(
                cell_tip("a", "b", &Err("Provider opaque: ß\n1234".into())),
                "Provider opaque: ß\n1234"
            );
        }
        crate::i18n::set_language("en-US");
    }
}
