//! API contract changes shown: the breaking ones first, each with its endpoint, then what
//! was added; a click opens its place in the file.

use egui::Ui;

use super::api::{ApiChange, ApiDiff, Severity};
use crate::repo::Action;
use crate::theme::{self, Tokens};

const ROW_H: f32 = 24.0;

pub(super) fn show(ui: &mut Ui, diff: &ApiDiff, acts: &mut Vec<Action>) {
    let t = Tokens::get(ui.ctx());
    let breaking = diff
        .changes
        .iter()
        .filter(|c| c.severity == Severity::Breaking)
        .count();
    let added = diff.changes.len() - breaking;
    egui::Frame::new()
        .inner_margin(egui::Margin::symmetric(16, 8))
        .show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(
                    egui::RichText::new(crate::i18n::message("repo-core-2032", &[("version", format!("{}", diff.version).into())]))
                        .font(theme::medium(12.5))
                        .color(t.text),
                );
                let summary = match (breaking, added) {
                    (0, 0) => {
                        crate::i18n::text("repo-core-2033").to_owned()
                    }
                    (0, n) => crate::i18n::message("repo-core-2034", &[("n", (n).into())]),
                    (1, n) => crate::i18n::message("repo-core-2035", &[("n", (n).into())]),
                    (b, n) => crate::i18n::message("repo-core-2036", &[("b", format!("{}", b).into()), ("n", (n).into())]),
                };
                let color = if breaking > 0 { t.danger } else { t.text_dim };
                ui.label(egui::RichText::new(summary).color(color));
            });
        });
    ui.spacing_mut().item_spacing.y = 0.0;
    egui::ScrollArea::both()
        .id_salt("rich_api")
        .auto_shrink(false)
        .show_rows(ui, ROW_H, diff.changes.len(), |ui, range| {
            for c in &diff.changes[range] {
                row(ui, &t, c, acts);
            }
        });
}

fn row(ui: &mut Ui, t: &Tokens, c: &ApiChange, acts: &mut Vec<Action>) {
    let (mark, color, fill) = match c.severity {
        Severity::Breaking => ("!", t.danger, t.diff_del_bg),
        Severity::Added => ("+", t.success, egui::Color32::TRANSPARENT),
    };
    let font = egui::FontId::proportional(13.0);
    let mut job = egui::text::LayoutJob::default();
    let mut add = |text: &str, font: egui::FontId, color| {
        job.append(text, 0.0, egui::text::TextFormat::simple(font, color));
    };
    add(&format!("{mark}  "), theme::mono(12.5), color);
    if !c.endpoint.is_empty() {
        add(&c.endpoint, theme::mono(12.5), t.text);
        add("   ", font.clone(), t.text);
    }
    add(&c.what, font, t.text_dim);
    let width = ui.available_width().max(200.0);
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(width, ROW_H), egui::Sense::click());
    let fill = if resp.hovered() { t.hover } else { fill };
    ui.painter().rect_filled(rect, 0.0, fill);
    let galley = ui.fonts_mut(|f| f.layout_job(job));
    ui.painter().galley(
        egui::pos2(rect.left() + 16.0, rect.center().y - galley.size().y / 2.0),
        galley,
        t.text,
    );
    let label = format!("{} {}", c.endpoint, c.what);
    crate::a11y::a11y(&resp, egui::WidgetType::Button, &label);
    let place = match (c.old_line, c.new_line) {
        (_, Some(n)) => crate::i18n::message("repo-core-2039", &[("n", (n).into())]),
        (Some(o), None) => crate::i18n::message("repo-core-2040", &[("o", format!("{}", o).into())]),
        _ => String::new(),
    };
    // The whole text, which a narrow pane cuts.
    let tip = format!("{label}\n{place}");
    let resp = resp
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .on_hover_text(tip);
    if resp.clicked() {
        let (old, new) = match c.new_line {
            Some(n) => (None, Some(n)),
            None => (c.old_line, None),
        };
        acts.push(Action::DiffGoToLine { old, new });
    }
}
