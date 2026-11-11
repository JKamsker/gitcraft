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
        (
            cols.cell(header, Bar::Refs),
            crate::i18n::text("repo-core-2612"),
        ),
        (cols.cell(header, Bar::Graph), graph),
        (
            cols.description(header),
            crate::i18n::text("repo-core-2613"),
        ),
    ];
    for (bar, label) in [
        (Bar::Hash, crate::i18n::text("repo-core-0893")),
        (Bar::Author, crate::i18n::text("repo-core-2614")),
        (Bar::When, crate::i18n::text("repo-core-2615")),
        (Bar::Date, crate::i18n::text("repo-core-2616")),
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

/// The header's right-click menu: which columns show, a compact graph column, the history map's
/// size, and the widths back as they were at first.
pub(super) fn header_menu(resp: &Response, settings: &Settings) {
    crate::menu_item::context(resp)
        .style(theme::menu_style)
        .show(|ui| {
            let mut next = settings.clone();
            ui.checkbox(&mut next.show_hash, crate::i18n::text("repo-core-0056"));
            ui.checkbox(&mut next.show_author, crate::i18n::text("repo-core-1671"));
            ui.checkbox(&mut next.show_when, crate::i18n::text("repo-core-2617"));
            ui.checkbox(&mut next.show_date, crate::i18n::text("repo-core-2618"));
            ui.checkbox(
                &mut next.show_test_lane,
                crate::i18n::text("repo-core-1020"),
            )
            .on_hover_text(crate::i18n::text(crate::repo::TEST_LANE_TIP));
            ui.separator();
            let mut fit = next.fit_visible_refs();
            if ui
                .checkbox(&mut fit, crate::i18n::text("repo-core-2619"))
                .changed()
            {
                next.auto_refs_width = fit;
                if fit {
                    next.refs_width = settings::DEFAULT_REFS_WIDTH;
                }
            }
            let mut compact = next.compact_graph;
            if ui
                .checkbox(&mut compact, crate::i18n::text("repo-core-2620"))
                .on_hover_text(settings::compact_tip())
                .changed()
            {
                next.set_compact_graph(compact);
            }
            ui.menu_button(
                crate::i18n::text("interaction-settings-label-history-map"),
                |ui| settings::HistoryMap::menu(ui, &mut next.history_map),
            );
            if crate::menu_item::MenuItem::new(crate::i18n::text("repo-core-2621"))
                .icon("command")
                .refused((next.graph_width <= 0.0).then_some(crate::i18n::text("repo-core-2622")))
                .show(ui)
                .on_hover_text(crate::i18n::text("repo-core-2623"))
                .clicked()
            {
                next.graph_width = 0.0;
            }
            ui.separator();
            if crate::menu_item::MenuItem::new(crate::i18n::text("repo-core-2624"))
                .icon("undo-2")
                .show(ui)
                .on_hover_text(crate::i18n::text("repo-core-2625"))
                .clicked()
            {
                next.reset_columns();
            }
            if next != *settings {
                next.set(ui.ctx());
            }
        });
}
