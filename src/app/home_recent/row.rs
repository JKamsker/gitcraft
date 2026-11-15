//! Recent-repository row layout, status painting, and explicit row actions.

use super::*;

pub(super) enum RowClick {
    Open,
    Forget,
}

/// The room left between a name and the time it was opened, `room` wide, shared: the name
/// (`name` wide), the dot of uncommitted changes and the branch (`status` wide) all as they
/// are when they fit; else the branch keeps at least about a third, the name the rest. Returns
/// the widths the name and the branch may take.
pub(in crate::app) fn share_room(room: f32, name: f32, dot: f32, status: f32) -> (f32, f32) {
    const GAP: f32 = 10.0;
    let room = room.max(0.0);
    let want = name + dot + GAP + status;
    if want <= room {
        return (name, status);
    }
    let status_min = (room * 0.35).min(status);
    let name_max = (room - dot - GAP - status_min).max(0.0).min(name);
    let status_max = (room - dot - GAP - name_max).clamp(0.0, status);
    (name_max, status_max)
}

/// `text` on one line, cut with "…" to fit `max` wide.
pub(in crate::app) fn elided(
    ui: &Ui,
    text: String,
    font: egui::FontId,
    color: egui::Color32,
    max: f32,
) -> Arc<Galley> {
    let mut job =
        egui::text::LayoutJob::single_section(text, egui::TextFormat::simple(font, color));
    job.wrap = egui::text::TextWrapping {
        max_width: max.max(1.0),
        max_rows: 1,
        break_anywhere: true,
        overflow_character: Some('…'),
    };
    ui.fonts_mut(|f| f.layout_job(job))
}

/// A recent repository's row. Returns what was clicked, and whether the mouse is over it.
pub(super) fn recent_row(ui: &mut Ui, path: &Path, info: &RowInfo) -> (Option<RowClick>, bool) {
    let t = Tokens::get(ui.ctx());
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 40.0), Sense::click());
    crate::repo::widgets::a11y(&resp, egui::WidgetType::Button, || {
        crate::remote::display(path)
    });
    let hovered = resp.hovered();
    if hovered || info.highlighted {
        ui.painter().rect_filled(rect, t.radius, t.hover);
    }
    if info.highlighted {
        let bar = Rect::from_min_size(rect.min + vec2(0.0, 8.0), vec2(3.0, rect.height() - 16.0));
        ui.painter().rect_filled(bar, 1.5, t.accent);
    }
    if resp.has_focus() {
        ui.painter().rect_stroke(
            rect,
            t.radius,
            Stroke::new(1.0, t.accent),
            egui::StrokeKind::Inside,
        );
    }
    let y = rect.center().y;
    // One on another machine says which.
    icons::paint(
        ui,
        Rect::from_center_size(pos2(rect.left() + 20.0, y), vec2(18.0, 18.0)),
        if crate::remote::is_remote(path) {
            "server"
        } else {
            "folder-git-2"
        },
        16.0,
        t.icon,
    );
    // Leave room for the remove button.
    let left = rect.left() + 40.0;
    let right = rect.right() - 36.0;
    let p = ui.painter_at(rect.with_max_x(right));
    let opened_left = info.opened.as_ref().map_or(right, |opened| {
        p.text(
            pos2(right, y - 8.0),
            Align2::RIGHT_CENTER,
            opened,
            egui::FontId::proportional(11.0),
            t.text_faint,
        )
        .left()
            - 12.0
    });
    first_line(ui, &p, path, info.status, pos2(left, y - 8.0), opened_left);
    let shown = elided(
        ui,
        crate::remote::display(path),
        egui::FontId::proportional(11.0),
        t.text_faint,
        right - left,
    );
    p.galley(
        pos2(left, y + 8.0 - shown.size().y / 2.0),
        shown,
        t.text_faint,
    );
    let mut forget = false;
    let forget_id = resp.id.with("forget");
    let keyboard = crate::a11y::keyboard_on(&resp) || ui.memory(|m| m.has_focus(forget_id));
    if hovered || keyboard || ui.rect_contains_pointer(rect) {
        let x_rect = Rect::from_center_size(pos2(rect.right() - 18.0, y), vec2(20.0, 20.0));
        let x = ui
            .interact(x_rect, forget_id, Sense::click())
            .on_hover_text(crate::i18n::text("interaction-home-remove"));
        crate::repo::widgets::a11y(
            &x,
            egui::WidgetType::Button,
            crate::i18n::text("interaction-home-remove"),
        );
        if x.hovered() {
            ui.painter().rect_filled(x_rect, t.radius_sm, t.pressed);
        }
        icons::paint(ui, x_rect, "x", 12.0, t.text_faint);
        forget = x.clicked();
    }
    let clicked = if forget {
        Some(RowClick::Forget)
    } else if resp.clicked() {
        Some(RowClick::Open)
    } else {
        None
    };
    (clicked, hovered)
}

/// The name, the dot of uncommitted changes, then the branch (or what is wrong), from `at` to
/// `max_x`: each cut with "…" rather than overlapping what follows.
pub(in crate::app) fn first_line(
    ui: &Ui,
    p: &egui::Painter,
    path: &Path,
    status: Option<&RecentStatus>,
    at: egui::Pos2,
    max_x: f32,
) {
    let t = Tokens::get(ui.ctx());
    let name_font = theme::medium(12.5);
    let status_font = egui::FontId::proportional(11.0);
    let full_name = elided(
        ui,
        repo_name(path),
        name_font.clone(),
        t.text,
        f32::INFINITY,
    );
    let (label, color, icon) = match status {
        Some(s) if s.is_problem() => (s.display_label(), t.warning, "circle-alert"),
        Some(s) => (s.display_label(), t.text_dim, "git-branch"),
        None => (String::new(), t.text_dim, ""),
    };
    let dirty = status.is_some_and(RecentStatus::dirty);
    const ICON: f32 = 15.0;
    let full_status = elided(ui, label.clone(), status_font.clone(), color, f32::INFINITY);
    let status_w = if label.is_empty() {
        0.0
    } else {
        ICON + full_status.size().x
    };
    let dot_w = if dirty { 14.0 } else { 0.0 };
    let (name_w, status_room) = share_room(max_x - at.x, full_name.size().x, dot_w, status_w);
    let name = elided(ui, repo_name(path), name_font, t.text, name_w);
    let name_right = at.x + name.size().x;
    p.galley(pos2(at.x, at.y - name.size().y / 2.0), name, t.text);
    let mut x = name_right;
    if dirty {
        let dot = pos2(x + 8.0, at.y);
        p.circle_filled(dot, 3.5, t.warning);
        let hit = Rect::from_center_size(dot, vec2(12.0, 12.0));
        ui.interact(hit, egui::Id::new(("recent-dirty", path)), Sense::hover())
            .on_hover_text(crate::i18n::text("interaction-home-dirty"));
        x += dot_w;
    }
    let Some(status) = status else { return };
    if label.is_empty() || status_room < ICON + 12.0 {
        return;
    }
    x += 10.0;
    icons::paint(
        ui,
        Rect::from_center_size(pos2(x + 6.0, at.y), vec2(12.0, 12.0)),
        icon,
        11.0,
        color,
    );
    let text = elided(ui, label, status_font, color, status_room - ICON);
    let text_rect = Rect::from_min_size(pos2(x + ICON, at.y - text.size().y / 2.0), text.size());
    p.galley(text_rect.min, text, color);
    if let Some(why) = status.reason() {
        ui.interact(
            text_rect,
            egui::Id::new(("recent-why", path)),
            Sense::hover(),
        )
        .on_hover_text(why);
    }
}
