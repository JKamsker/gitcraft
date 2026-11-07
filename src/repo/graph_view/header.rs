//! The strip above the graph: the names of its columns, and its menu.

use egui::{Rect, Response, Stroke, Ui, pos2};

use crate::repo::columns::Cols;
use crate::settings::{self, Settings};
use crate::theme::{self, Tokens};

/// The names of the columns, in the strip `header` above the graph; `graph` for the graph's.
pub(super) fn column_headers(ui: &Ui, t: &Tokens, header: Rect, cols: &Cols, graph: &str) {
    let p = ui.painter();
    p.rect_filled(header, 0.0, t.chrome);
    p.line_segment(
        [header.left_bottom(), header.right_bottom()],
        Stroke::new(1.0, t.separator),
    );
    let y = header.center().y;
    let font = theme::semibold(10.5);
    use crate::repo::columns::Bar;
    let mut labels = vec![
        (cols.cell(header, Bar::Refs), "BRANCH / TAG"),
        (cols.cell(header, Bar::Graph), graph),
        (cols.description(header), "DESCRIPTION"),
    ];
    for (bar, label) in [
        (Bar::Hash, "COMMIT"),
        (Bar::Author, "AUTHOR"),
        (Bar::When, "WHEN"),
        (Bar::Date, "DATE"),
    ] {
        if cols.width(bar) > 0.0 {
            labels.push((cols.cell(header, bar), label));
        }
    }
    for (cell, label) in labels {
        let clip = Cols::padded(cell).intersect(ui.clip_rect());
        let mut job =
            egui::text::LayoutJob::simple_singleline(label.to_owned(), font.clone(), t.text_faint);
        job.wrap = egui::text::TextWrapping::truncate_at_width(clip.width().max(0.0));
        let galley = ui.fonts_mut(|f| f.layout_job(job));
        p.with_clip_rect(clip).galley(
            pos2(
                cell.left() + crate::repo::columns::CELL_PAD,
                y - galley.size().y / 2.0,
            ),
            galley,
            t.text_faint,
        );
    }
}

/// The header's right-click menu: which columns show, a compact graph column, and the widths
/// back as they were at first.
pub(super) fn header_menu(resp: &Response, settings: &Settings) {
    egui::Popup::context_menu(resp)
        .style(theme::menu_style)
        .show(|ui| {
            let mut next = settings.clone();
            ui.checkbox(&mut next.show_hash, "Commit");
            ui.checkbox(&mut next.show_author, "Author");
            ui.checkbox(&mut next.show_when, "When");
            ui.checkbox(&mut next.show_date, "Date");
            ui.checkbox(&mut next.show_test_lane, "Tests")
                .on_hover_text(crate::repo::TEST_LANE_TIP);
            ui.separator();
            let mut fit = next.fit_visible_refs();
            if ui
                .checkbox(&mut fit, "Fit visible branch / tag pills")
                .changed()
            {
                next.auto_refs_width = fit;
                if fit {
                    next.refs_width = settings::DEFAULT_REFS_WIDTH;
                }
            }
            let mut compact = next.compact_graph;
            if ui
                .checkbox(&mut compact, "Compact graph column")
                .on_hover_text(settings::COMPACT_TIP)
                .changed()
            {
                next.set_compact_graph(compact);
            }
            if ui
                .add_enabled(
                    next.graph_width > 0.0,
                    egui::Button::new("Fit graph column to its lanes"),
                )
                .on_hover_text("As double-clicking its border does")
                .clicked()
            {
                next.graph_width = 0.0;
            }
            ui.separator();
            if ui
                .button("Reset columns")
                .on_hover_text("Every column back to its first width")
                .clicked()
            {
                next.reset_columns();
            }
            if next != *settings {
                next.set(ui.ctx());
            }
        });
}
