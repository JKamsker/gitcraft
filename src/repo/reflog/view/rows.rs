//! Reflog row geometry, timestamp/kind painting, and scan status messages.

use super::*;

/// Where a row's columns end, from the right; those there is no room for are `None`.
pub(super) struct Columns {
    date_end: Option<f32>,
    pub(super) ago_end: f32,
    pub(super) move_end: Option<f32>,
    /// Where the entry's text has to stop.
    pub(super) text_end: f32,
}

impl Columns {
    pub(super) fn of(rect: Rect) -> Self {
        let right = rect.right() - 12.0;
        let date_end = (rect.width() >= ROOM_FOR_DATE).then_some(right);
        let ago_end = if date_end.is_some() {
            right - DATE_W
        } else {
            right
        };
        let move_end = (rect.width() >= ROOM_FOR_MOVE).then_some(ago_end - AGO_W);
        let text_end = match move_end {
            Some(end) => end - MOVE_W,
            None => ago_end - AGO_W,
        };
        Self {
            date_end,
            ago_end,
            move_end,
            text_end,
        }
    }
}

/// How long ago, and the date when there is room.
pub(super) fn times(
    p: &egui::Painter,
    t: &Tokens,
    cols: &Columns,
    mid: f32,
    time: i64,
    date: &str,
    now: i64,
) {
    p.text(
        pos2(cols.ago_end, mid),
        Align2::RIGHT_CENTER,
        git::relative_time(time, now),
        FontId::proportional(11.5),
        t.text_dim,
    );
    if let Some(end) = cols.date_end {
        p.text(
            pos2(end, mid),
            Align2::RIGHT_CENTER,
            date,
            theme::mono(11.0),
            t.text_faint,
        );
    }
}

pub(super) fn paint_row_back(ui: &Ui, rect: Rect, selected: bool, hovered: bool) {
    let t = Tokens::get(ui.ctx());
    if selected {
        ui.painter().rect_filled(rect, 0.0, t.row_selected);
    } else if hovered {
        ui.painter().rect_filled(rect, 0.0, t.hover);
    }
}

/// The kind of an entry, as a tinted label starting at `left`.
pub(super) fn kind_chip(p: &egui::Painter, t: &Tokens, left: Pos2, kind: ReflogKind) {
    let color = kind_color(t, kind);
    let rect = Rect::from_min_size(left - vec2(0.0, 9.0), vec2(KIND_W - 8.0, 18.0));
    p.rect_filled(rect, CornerRadius::same(9), color.gamma_multiply(0.18));
    p.text(
        rect.center(),
        Align2::CENTER_CENTER,
        kind.label(),
        theme::medium(11.0),
        color,
    );
}

/// What stands out: moves that leave commits behind (reset, rebase) in the warning colour.
fn kind_color(t: &Tokens, kind: ReflogKind) -> Color32 {
    match kind {
        ReflogKind::Reset | ReflogKind::Rebase => t.warning,
        ReflogKind::Initial
        | ReflogKind::Commit
        | ReflogKind::Amend
        | ReflogKind::CherryPick
        | ReflogKind::Revert
        | ReflogKind::Am => t.success,
        ReflogKind::Merge
        | ReflogKind::Pull
        | ReflogKind::Fetch
        | ReflogKind::Push
        | ReflogKind::Clone => t.accent,
        ReflogKind::Checkout | ReflogKind::Branch => t.text_dim,
        ReflogKind::Other => t.text_faint,
    }
}

/// An entry's hover: its name, what moved it, when and from where to where.
pub(super) fn entry_tip(e: &ReflogEntry, now: i64) -> String {
    let message = if e.message.detail.is_empty() {
        e.message.action.clone()
    } else {
        format!("{}: {}", e.message.action, e.message.detail)
    };
    let moved = match &e.old {
        Some(old) => format!("{old} → {}", e.new),
        None => e.new.clone(),
    };
    format!(
        "{}\n{message}\n{} ({})\n{moved}",
        e.selector,
        e.date,
        git::relative_time(e.time, now)
    )
}

pub(super) fn loading_row(ui: &mut Ui) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), ROW_H), Sense::hover());
    spinner::Spinner::new()
        .size(14.0)
        .paint_at(ui, Rect::from_center_size(rect.center(), vec2(14.0, 14.0)));
}

/// A line of text in the middle of the list's place.
pub(super) fn message(ui: &mut Ui, text: &str, color: Color32) {
    ui.add_space(24.0);
    ui.vertical_centered(|ui| {
        ui.set_max_width(520.0);
        ui.label(egui::RichText::new(text).color(color));
    });
}

/// The scan running: how long it has, and Cancel.
pub(super) fn scanning(ui: &mut Ui, t: &Tokens, secs: u64, acts: &mut Vec<Action>) {
    ui.add_space(24.0);
    ui.vertical_centered(|ui| {
        ui.add(spinner::Spinner::new().size(20.0));
        ui.add_space(10.0);
        ui.label(
            egui::RichText::new(crate::i18n::message(
                "repo-workflows-reflog-scanning-7ed409a6",
                &[("secs", (secs).into())],
            ))
            .color(t.text_dim),
        );
        ui.add_space(8.0);
        if ui
            .button(crate::i18n::text("repo-workflows-reflog-scanning-19766ed6"))
            .clicked()
        {
            acts.push(Action::Reflog(ReflogAct::CancelScan));
        }
    });
    ui.ctx()
        .request_repaint_after(std::time::Duration::from_secs(1));
}

/// Before a scan (or after one was cancelled): what it does, and the button that starts it.
pub(super) fn not_scanned(ui: &mut Ui, t: &Tokens, acts: &mut Vec<Action>) {
    ui.add_space(24.0);
    ui.vertical_centered(|ui| {
        ui.set_max_width(520.0);
        ui.label(
            egui::RichText::new(crate::i18n::text(
                "repo-workflows-reflog-not-scanned-e1899125",
            ))
            .color(t.text_dim),
        );
        ui.add_space(10.0);
        if widgets::primary_button(
            ui,
            crate::i18n::text("repo-workflows-reflog-not-scanned-faab01a6"),
            true,
            0.0,
        )
        .clicked()
        {
            acts.push(Action::Reflog(ReflogAct::Scan));
        }
    });
}
