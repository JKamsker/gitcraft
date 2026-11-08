//! The strip above the graph: the names of its columns, and its menu.

use egui::{Align2, Rect, Response, Stroke, Ui, pos2};

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
    let right = header.right();
    let mut labels = vec![
        (header.left() + 10.0, "BRANCH / TAG"),
        (header.left() + cols.refs + 10.0, graph),
        (header.left() + cols.refs + cols.graph + 6.0, "DESCRIPTION"),
    ];
    if cols.hash > 0.0 {
        labels.push((right - cols.trailing(), crate::i18n::text("repo-core-0893")));
    }
    if cols.author > 0.0 {
        labels.push((right - cols.author - cols.when - cols.date, "AUTHOR"));
    }
    if cols.when > 0.0 {
        labels.push((right - cols.when - cols.date, "WHEN"));
    }
    if cols.date > 0.0 {
        labels.push((right - cols.date, "DATE"));
    }
    // Each label is clipped where the next column starts, so a narrow window
    // cuts DESCRIPTION short instead of running into AUTHOR.
    for (i, (x, label)) in labels.iter().enumerate() {
        let limit = labels
            .get(i + 1)
            .map_or(header.right(), |(next, _)| next - 8.0);
        let clip = Rect::from_min_max(pos2(*x, header.top()), pos2(limit.max(*x), header.bottom()));
        p.with_clip_rect(clip).text(
            pos2(*x, y),
            Align2::LEFT_CENTER,
            label,
            font.clone(),
            t.text_faint,
        );
    }
}

/// The header's right-click menu: which columns show, a compact graph column, and the widths
/// back as they were at first.
pub(super) fn header_menu(resp: &Response, settings: &Settings) {
    crate::menu_item::context(resp)
        .style(theme::menu_style)
        .show(|ui| {
            let mut next = settings.clone();
            ui.checkbox(&mut next.show_hash, crate::i18n::text("repo-core-0056"));
            ui.checkbox(&mut next.show_author, crate::i18n::text("repo-core-1671"));
            ui.checkbox(&mut next.show_when, "When");
            ui.checkbox(&mut next.show_date, "Date");
            ui.checkbox(&mut next.show_test_lane, crate::i18n::text("repo-core-1020"))
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
            if crate::menu_item::MenuItem::new("Fit graph column to its lanes")
                .icon("command")
                .refused(
                    (next.graph_width <= 0.0).then_some("The graph column already fits its lanes"),
                )
                .show(ui)
                .on_hover_text("As double-clicking its border does")
                .clicked()
            {
                next.graph_width = 0.0;
            }
            ui.separator();
            if crate::menu_item::MenuItem::new("Reset columns")
                .icon("undo-2")
                .show(ui)
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
