//! Config files by key shown: one row per key path that changed, `server.timeout: 30 → 60`,
//! keys that only moved listed apart; a click goes to the key's line in the diff.

use egui::{Color32, Ui};

use super::config::{Change, ConfigDiff, Entry};
use crate::repo::Action;
use crate::theme::{self, Tokens};

const ROW_H: f32 = 22.0;

pub(super) fn show(ui: &mut Ui, diff: &ConfigDiff, acts: &mut Vec<Action>) {
    let t = Tokens::get(ui.ctx());
    let count = |c: Change| diff.entries.iter().filter(|e| e.change == c).count();
    let (changed, added, removed, moved) = (
        count(Change::Changed),
        count(Change::Added),
        count(Change::Removed),
        count(Change::Reordered),
    );
    egui::Frame::new()
        .inner_margin(egui::Margin::symmetric(16, 8))
        .show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(
                    egui::RichText::new(format!("{} by key", diff.format))
                        .font(theme::medium(12.5))
                        .color(t.text),
                );
                let summary = if diff.entries.is_empty() {
                    "the same keys and values: only formatting, comments or quoting changed"
                        .to_owned()
                } else {
                    format!("{changed} changed · {added} added · {removed} removed · {moved} moved")
                };
                ui.label(egui::RichText::new(summary).color(t.text_dim));
            });
        });
    // Changes first, then the keys that only moved.
    let rows: Vec<&Entry> = diff
        .entries
        .iter()
        .filter(|e| e.change != Change::Reordered)
        .chain(
            diff.entries
                .iter()
                .filter(|e| e.change == Change::Reordered),
        )
        .collect();
    let font = theme::mono(12.0);
    ui.spacing_mut().item_spacing.y = 0.0;
    egui::ScrollArea::both()
        .id_salt("rich_config")
        .auto_shrink(false)
        .show_rows(ui, ROW_H, rows.len(), |ui, range| {
            for k in range {
                row(ui, &t, &font, rows[k], acts);
            }
        });
}

fn row(ui: &mut Ui, t: &Tokens, font: &egui::FontId, e: &Entry, acts: &mut Vec<Action>) {
    let (mark, color, fill) = match e.change {
        Change::Added => ("+", t.success, t.diff_add_bg),
        Change::Removed => ("−", t.danger, t.diff_del_bg),
        Change::Changed => ("~", t.accent, Color32::TRANSPARENT),
        Change::Reordered => ("⇅", t.text_faint, Color32::TRANSPARENT),
    };
    let mut job = egui::text::LayoutJob::default();
    let mut add = |text: &str, color: Color32| {
        job.append(
            text,
            0.0,
            egui::text::TextFormat::simple(font.clone(), color),
        );
    };
    add(&format!("{mark} "), color);
    add(&e.path, t.text);
    add(": ", t.text_faint);
    match (e.change, &e.old, &e.new) {
        (Change::Changed, Some(o), Some(n)) => {
            add(o, t.danger);
            add(" → ", t.text_faint);
            add(n, t.success);
            if let Some((a, b)) = e.kinds {
                add(&format!("   ({a} → {b})"), t.warning);
            }
        }
        (Change::Removed, Some(o), _) => add(o, t.text_dim),
        (Change::Reordered, _, Some(n)) => {
            add(n, t.text_faint);
            add("   moved, same value", t.text_faint);
        }
        (_, _, Some(n)) => add(n, t.text_dim),
        _ => {}
    }
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
    let text = format!(
        "{}: {}",
        e.path,
        match (&e.old, &e.new) {
            (Some(o), Some(n)) if e.change == Change::Changed => format!("{o} → {n}"),
            (_, Some(n)) => n.clone(),
            (Some(o), None) => o.clone(),
            _ => String::new(),
        }
    );
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, &text));
    let line = e.new_line.or(e.old_line).unwrap_or(0);
    let resp = resp
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .on_hover_text(format!("Go to line {line} in the diff"));
    if resp.clicked() {
        // A removed key is only before the change; anything else is shown where it is now.
        let (old, new) = match e.change {
            Change::Removed => (e.old_line, None),
            _ => (None, e.new_line),
        };
        acts.push(Action::DiffGoToLine { old, new });
    }
}
