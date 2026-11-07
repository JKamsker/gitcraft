//! Read-only source rows with only novel syntax tokens emphasized.

use egui::{
    Align2, Sense, Ui, pos2,
    text::{LayoutJob, TextFormat},
    vec2,
};

use crate::theme::{self, Tokens};

use super::model::{Line, Row, Structural};

const HEIGHT: f32 = 20.0;
const GUTTER: f32 = 52.0;

pub(in crate::repo::diff) fn show(ui: &mut Ui, data: &Structural) {
    let t = Tokens::get(ui.ctx());
    ui.label(
        egui::RichText::new(format!("{} · syntax changes · read only", data.language))
            .color(t.text_dim),
    );
    ui.label(
        egui::RichText::new("Use the patch to stage changes and to see formatting edits.")
            .small()
            .color(t.text_faint),
    );
    if data.unchanged {
        ui.add_space(12.0);
        ui.label("No syntax changes. Formatting can still differ in the patch.");
        return;
    }
    let char_w = ui
        .painter()
        .layout_no_wrap("M".to_owned(), theme::mono(12.0), t.text)
        .size()
        .x;
    egui::ScrollArea::both().id_salt("syntax_diff").show_rows(
        ui,
        HEIGHT,
        data.rows.len() + 1,
        |ui, range| {
            let width = ui.available_width().max(500.0);
            let half = (width / 2.0).max(data.longest as f32 * char_w + GUTTER + 12.0);
            for index in range {
                if index == 0 {
                    let (rect, _) =
                        ui.allocate_exact_size(vec2(half * 2.0, HEIGHT), Sense::hover());
                    for (column, label) in ["Before", "After"].into_iter().enumerate() {
                        ui.painter().text(
                            pos2(rect.left() + half * column as f32 + GUTTER, rect.center().y),
                            Align2::LEFT_CENTER,
                            label,
                            theme::mono(12.0),
                            t.text_dim,
                        );
                    }
                    continue;
                }
                match &data.rows[index - 1] {
                    Row::Heading(n) => {
                        let (rect, _) = ui.allocate_exact_size(vec2(width, HEIGHT), Sense::hover());
                        ui.painter()
                            .rect_filled(rect, 0.0, t.accent.gamma_multiply(0.1));
                        ui.painter().text(
                            rect.left_center() + vec2(8.0, 0.0),
                            Align2::LEFT_CENTER,
                            format!("Syntax change {n}"),
                            theme::mono(11.0),
                            t.accent,
                        );
                    }
                    Row::Pair(old, new) => {
                        // Size from the source lines, so the horizontal scrollbar can reach a
                        // long expression. Rows are virtualized; only visible lines are laid out.
                        let jobs = [job(old.as_ref(), true, &t), job(new.as_ref(), false, &t)];
                        let galleys = jobs.map(|j| ui.fonts_mut(|f| f.layout_job(j)));
                        let (rect, _) =
                            ui.allocate_exact_size(vec2(half * 2.0, HEIGHT), Sense::hover());
                        for (i, (line, galley)) in [old.as_ref(), new.as_ref()]
                            .into_iter()
                            .zip(galleys)
                            .enumerate()
                        {
                            let x = rect.left() + half * i as f32;
                            if let Some(line) = line {
                                ui.painter().text(
                                    pos2(x + GUTTER - 10.0, rect.center().y),
                                    Align2::RIGHT_CENTER,
                                    line.number.to_string(),
                                    theme::mono(11.0),
                                    t.text_faint,
                                );
                                ui.painter().galley(
                                    pos2(x + GUTTER, rect.top() + 2.0),
                                    galley,
                                    t.text,
                                );
                            }
                        }
                    }
                }
            }
        },
    );
}

fn job(line: Option<&Line>, old: bool, t: &Tokens) -> LayoutJob {
    let mut job = LayoutJob::default();
    let Some(line) = line else { return job };
    let mut start = 0;
    let mut append = |text: &str, changed: bool| {
        let mut format = TextFormat {
            font_id: theme::mono(12.0),
            color: t.text_dim,
            ..Default::default()
        };
        if changed {
            format.color = if old { t.danger } else { t.success };
            format.background = if old { t.diff_del_bg } else { t.diff_add_bg };
        }
        // Tabs are displayed as four spaces after byte ranges have been checked/sliced.
        job.append(&text.replace('\t', "    ").replace('\r', ""), 0.0, format);
    };
    for range in &line.changed {
        append(&line.text[start..range.start], false);
        append(&line.text[range.clone()], true);
        start = range.end;
    }
    append(&line.text[start..], false);
    job
}
