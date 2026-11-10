//! Commit row backgrounds, descriptions and trailing columns.

use super::*;

pub(super) fn row_background(
    painter: &Painter,
    t: &Tokens,
    rect: Rect,
    selected: bool,
    hovered: bool,
    is_match: bool,
    compared: bool,
) -> Color32 {
    // The row's own band, without the gap.
    let band = rect.shrink2(vec2(0.0, ROW_GAP / 2.0));
    let background = if selected {
        painter.rect_filled(band, 0.0, t.row_selected);
        t.row_selected
    } else if hovered {
        painter.rect_filled(band, 0.0, t.hover);
        t.hover
    } else if is_match {
        let tint = t.match_tint();
        painter.rect_filled(band, 0.0, tint);
        theme::palette::over(tint, t.dock)
    } else {
        t.dock
    };
    if is_match {
        painter.rect_filled(
            Rect::from_min_size(band.min, vec2(3.0, ROW_H)),
            0.0,
            t.warning,
        );
    }
    if compared {
        painter.rect_stroke(
            band.shrink(1.0),
            0.0,
            Stroke::new(1.0, t.accent),
            egui::StrokeKind::Inside,
        );
    }
    background
}

/// The summary, then the rest of the message faded, cut with an ellipsis.
/// `reserved`: room left at the end (for the build status, a merge's badge).
pub(super) fn description(
    ui: &Ui,
    painter: &Painter,
    t: &Tokens,
    row: &Row,
    (is_match, new_commit): (bool, bool),
    reserved: f32,
    prefix: f32,
) -> f32 {
    let (rect, mid) = (row.rect, row.mid());
    let mut x = row.cols.description(rect).left() + CELL_PAD + prefix;
    let text_right = row.cols.description(rect).right() - CELL_PAD - reserved;
    if new_commit && text_right - x > 70.0 {
        let marker = Rect::from_center_size(pos2(x + 13.0, mid), vec2(27.0, 17.0));
        painter.rect_filled(marker, 3.0, t.card);
        painter.rect_stroke(
            marker,
            3.0,
            Stroke::new(1.0, t.accent),
            egui::StrokeKind::Inside,
        );
        painter.text(
            marker.center(),
            Align2::CENTER_CENTER,
            crate::i18n::text("dev-integration-ui-new-marker"),
            FontId::proportional(10.0),
            t.text,
        );
        x += 34.0;
    }
    let font = if is_match {
        theme::medium(12.5)
    } else {
        FontId::proportional(12.5)
    };
    let text_clip = painter.with_clip_rect(Rect::from_min_max(
        pos2(x, rect.top()),
        pos2(text_right.max(x), rect.bottom()),
    ));
    let mut job =
        egui::text::LayoutJob::simple_singleline(row.commit.summary.clone(), font, t.text);
    job.wrap = egui::text::TextWrapping::truncate_at_width((text_right - x - 2.0).max(0.0));
    let galley = ui.fonts_mut(|f| f.layout_job(job));
    let summary = Rect::from_min_size(pos2(x + 2.0, mid - galley.size().y / 2.0), galley.size());
    text_clip.galley(summary.min, galley, t.text);
    let room = text_right - summary.right() - 12.0;
    let end = summary.right().min(text_right);
    if !row.commit.body.is_empty() && room > 24.0 {
        let mut job = egui::text::LayoutJob::single_section(
            row.commit.body.clone(),
            egui::TextFormat::simple(FontId::proportional(12.5), t.text_faint),
        );
        job.wrap = egui::text::TextWrapping::truncate_at_width(room);
        let galley = ui.fonts_mut(|f| f.layout_job(job));
        text_clip.galley(
            pos2(summary.right() + 10.0, mid - galley.size().y / 2.0),
            galley,
            t.text,
        );
    }
    end
}

/// The hash, author, when and date columns, those the settings show.
pub(super) fn trailing_columns(
    ui: &mut Ui,
    painter: &Painter,
    t: &Tokens,
    row: &Row,
    now: i64,
    lens: &crate::repo::authorship_view::Lens,
) {
    let (rect, mid, cols, commit) = (row.rect, row.mid(), row.cols, row.commit);
    let small = FontId::proportional(11.5);
    for (bar, text, font, color) in [
        (
            Bar::Hash,
            if commit.hash.starts_with('\0') {
                "—".to_owned()
            } else {
                commit.hash.get(..7).unwrap_or(&commit.hash).to_owned()
            },
            theme::mono(11.0),
            t.text_faint,
        ),
        (
            Bar::When,
            git::relative_time(commit.time, now),
            small.clone(),
            t.text_dim,
        ),
        (
            Bar::Date,
            crate::i18n::date_iso(&commit.date),
            theme::mono(11.0),
            t.text_faint,
        ),
    ] {
        if cols.width(bar) <= 0.0 {
            continue;
        }
        let cell = Cols::padded(cols.cell(rect, bar));
        truncated(
            ui,
            &painter.with_clip_rect(cell),
            pos2(cell.left(), mid),
            &text,
            font,
            color,
            cell.width(),
        );
    }
    if cols.author > 0.0 {
        let cell = Cols::padded(cols.cell(rect, Bar::Author));
        let clip = cell.intersect(ui.clip_rect());
        let mut author_ui = ui.new_child(egui::UiBuilder::new().max_rect(cell));
        author_ui.set_clip_rect(clip);
        let avatar = pos2(cell.left() + AVATAR_D / 2.0, mid);
        widgets::avatar(
            &author_ui,
            avatar,
            AVATAR_D / 2.0,
            &commit.author,
            &commit.email,
        );
        let mut x = cell.left() + AVATAR_D + 4.0;
        if let Some(a) = lens.of(commit) {
            x += crate::repo::authorship_view::robot(
                &author_ui,
                x,
                mid,
                &a,
                ui.id().with(("robot", &commit.hash)),
            );
        }
        truncated(
            ui,
            &painter.with_clip_rect(clip),
            pos2(x, mid),
            &commit.author,
            small,
            t.text_dim,
            (cell.right() - x).max(0.0),
        );
        // Keep the robot's more specific attribution tooltip over its own hit target.
        if ui.rect_contains_pointer(clip)
            && !ui.rect_contains_pointer(Rect::from_min_size(
                pos2(cell.left() + AVATAR_D + 4.0, rect.top()),
                vec2((x - cell.left() - AVATAR_D - 4.0).max(0.0), rect.height()),
            ))
        {
            ui.interact(
                clip,
                ui.id().with(("author-detail", &commit.hash)),
                Sense::hover(),
            )
            .on_hover_text(format!("{} <{}>", commit.author, commit.email));
        }
    }
}

pub(super) fn truncated(
    ui: &Ui,
    painter: &Painter,
    at: Pos2,
    text: &str,
    font: FontId,
    color: Color32,
    width: f32,
) {
    let mut job = egui::text::LayoutJob::simple_singleline(text.to_owned(), font, color);
    job.wrap = egui::text::TextWrapping::truncate_at_width(width.max(0.0));
    let galley = ui.fonts_mut(|f| f.layout_job(job));
    painter.galley(at - vec2(0.0, galley.size().y / 2.0), galley, color);
}

/// A small rounded count label; returns its width.
pub(in crate::repo) fn pill(
    painter: &Painter,
    t: &Tokens,
    left_center: Pos2,
    text: &str,
    color: Color32,
    background: Color32,
) -> f32 {
    let galley = painter.layout_no_wrap(text.to_owned(), theme::medium(11.0), color);
    let w = galley.size().x + 12.0;
    let r = Rect::from_min_size(pos2(left_center.x, left_center.y - 9.0), vec2(w, 18.0));
    painter.rect_filled(
        r,
        CornerRadius::same(9),
        t.text_tint(color, 0.18, background),
    );
    painter.galley(
        pos2(r.left() + 6.0, r.center().y - galley.size().y / 2.0),
        galley,
        color,
    );
    w
}

pub(super) fn faint_lane_label(ui: &Ui, painter: &Painter, t: &Tokens, row: &Row, label: &str) {
    let x = row.rect.left() + 8.0;
    let mut job = egui::text::LayoutJob::single_section(
        label.to_owned(),
        egui::TextFormat::simple(FontId::proportional(11.0), t.text_faint),
    );
    job.wrap = egui::text::TextWrapping::truncate_at_width((row.cols.refs - 24.0).max(0.0));
    let galley = ui.fonts_mut(|f| f.layout_job(job));
    let right = x + galley.size().x;
    let clip = Rect::from_min_max(row.rect.min, pos2(row.gx - 4.0, row.rect.bottom()));
    painter.with_clip_rect(clip).galley(
        pos2(x, row.mid() - galley.size().y / 2.0),
        galley,
        t.text_faint,
    );
    if let Some(node_x) = row.node_x {
        painter.line_segment(
            [
                pos2(right + 6.0, row.mid()),
                pos2(node_x.min(row.gx + row.cols.graph), row.mid()),
            ],
            Stroke::new(1.0, t.text_faint.gamma_multiply(0.35)),
        );
    }
}
