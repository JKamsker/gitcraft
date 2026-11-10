//! Ref pill geometry, contrast, icons and label painting.

use super::*;

pub(super) fn more_count(painter: &Painter, t: &Tokens, r: Rect, more: usize, lane: Color32) {
    painter.rect_filled(
        r,
        CornerRadius::same(t.radius as u8),
        theme::palette::white_text_fill(
            lane.lerp_to_gamma(Color32::BLACK, 0.35)
                .lerp_to_gamma(Color32::BLACK, DIMMED),
        ),
    );
    painter.text(
        r.center(),
        Align2::CENTER_CENTER,
        format!("+{more}"),
        theme::medium(10.5),
        Color32::WHITE,
    );
}

/// What a pill carries besides its name and where the branch lives.
#[derive(Clone, Copy, Default)]
pub(super) struct Marks {
    /// The branch has an open pull request.
    pub(super) pr: bool,
    /// The build status of its tip.
    pub(super) ci: Option<CheckState>,
}

/// A ghost run's marks on a pill: a dashed outline where it would go, a line through one it
/// would delete.
pub(super) fn ghost_pill_marks(p: &Painter, t: &Tokens, pill: Rect, moved: bool, gone: bool) {
    if moved {
        let r = pill.expand(2.0);
        let corners = [
            r.left_top(),
            r.right_top(),
            r.right_bottom(),
            r.left_bottom(),
            r.left_top(),
        ];
        p.add(egui::Shape::dashed_line(
            &corners,
            Stroke::new(1.2, t.text),
            3.0,
            2.0,
        ));
    }
    if gone {
        let ends = [
            pos2(pill.left() + 2.0, pill.center().y),
            pos2(pill.right() - 2.0, pill.center().y),
        ];
        // A halo of the background under the line keeps it apart from the pill's colour.
        p.line_segment(ends, Stroke::new(4.0, t.dock));
        p.line_segment(ends, Stroke::new(2.0, t.danger));
    }
}

pub(super) fn pill_fill(t: &Tokens, badge: &Badge, lane: Color32) -> Color32 {
    let fill = match badge.kind {
        BadgeKind::Branch if badge.local => lane,
        // Only on a remote: a darker shade of the lane.
        BadgeKind::Branch | BadgeKind::PullRequest => lane.lerp_to_gamma(Color32::BLACK, 0.35),
        BadgeKind::Tag => Color32::from_rgb(104, 104, 112),
        BadgeKind::DetachedHead => t.warning,
    };
    // Every pill but the checked-out one is a darker shade of its colour.
    let fill = if badge.head {
        fill
    } else {
        fill.lerp_to_gamma(Color32::BLACK, DIMMED)
    };
    theme::palette::white_text_fill(fill)
}

/// Paints a pill with white text and icons, at most `max_w` wide; returns its rect.
pub(super) fn ref_pill(
    ui: &Ui,
    painter: &Painter,
    left_center: Pos2,
    badge: &Badge,
    lane: Color32,
    max_w: f32,
    marks: Marks,
) -> Rect {
    const ICON: f32 = 15.0;
    let t = Tokens::get(ui.ctx());
    let white = Color32::WHITE;
    let fill = pill_fill(&t, badge, lane);
    let (mut lead, mut trail) = pill_icons(badge, marks.pr);
    let font = if badge.head {
        theme::semibold(11.0)
    } else {
        theme::medium(11.0)
    };
    // At the column's minimum, the branch name takes precedence over its decorations. Keep
    // room for at least a short name such as `dev`; the tooltip still describes every mark.
    let name_w = painter
        .layout_no_wrap(badge.label.clone(), font.clone(), white)
        .size()
        .x;
    let short_w = painter
        .layout_no_wrap("dev".to_owned(), font.clone(), white)
        .size()
        .x;
    let icon_room = (max_w - 16.0 - name_w.min(short_w)).max(0.0);
    let mut ci = marks.ci;
    let mut dot_w = if ci.is_some() { 12.0 } else { 0.0 };
    while (lead.len() + trail.len()) as f32 * ICON + dot_w > icon_room {
        if trail.pop().is_some() {
            continue;
        }
        if ci.take().is_some() {
            dot_w = 0.0;
            continue;
        }
        if lead.pop().is_none() {
            break;
        }
    }
    let icons_w = (lead.len() + trail.len()) as f32 * ICON + dot_w;
    let text_room = (max_w - 16.0 - icons_w).max(16.0);
    let keep_end = crate::settings::Settings::branch_names_keep_end(ui.ctx());
    let label = truncate(painter, &badge.label, &font, text_room, keep_end);
    let galley = painter.layout_no_wrap(label, font, white);
    let w = (galley.size().x + 16.0 + icons_w).min(max_w);
    let r = Rect::from_min_size(pos2(left_center.x, left_center.y - 10.0), vec2(w, 20.0));
    painter.rect_filled(r, CornerRadius::same(t.radius as u8), fill);
    // Small manual widths can leave too little room for all marks. Text and icons must
    // stay inside the pill's actual hitbox; SVG images use the Ui's clip, not this painter's.
    let painter = &painter.with_clip_rect(painter.clip_rect().intersect(r));
    let mut x = r.left() + 7.0;
    for icon in lead {
        let icon_rect = Rect::from_center_size(pos2(x + 6.0, r.center().y), vec2(12.0, 12.0));
        if painter.clip_rect().contains_rect(icon_rect) {
            icons::paint(ui, icon_rect, icon, 11.0, white);
        }
        x += ICON;
    }
    let gw = galley.size().x;
    painter.galley(pos2(x, r.center().y - galley.size().y / 2.0), galley, white);
    x += gw + 1.0;
    for icon in trail {
        let icon_rect = Rect::from_center_size(pos2(x + 8.0, r.center().y), vec2(12.0, 12.0));
        if painter.clip_rect().contains_rect(icon_rect) {
            icons::paint(ui, icon_rect, icon, 11.0, white);
        }
        x += ICON;
    }
    if let Some(state) = ci {
        // On a ring of the pill's colour, so it reads on any lane.
        let center = pos2((x + 7.0).min(r.right() - 7.0), r.center().y);
        painter.circle_filled(center, ci_status::DOT_R + 1.5, Color32::WHITE);
        ci_status::paint_dot(painter, &t, center, ci_status::DOT_R, state);
    }
    r
}

/// The icons before and after a pill's label; `pr`: the branch has a pull request.
pub(super) fn pill_icons(badge: &Badge, pr: bool) -> (Vec<&'static str>, Vec<&'static str>) {
    let mut lead = Vec::new();
    let mut trail = Vec::new();
    match badge.kind {
        BadgeKind::Tag => lead.push("tag"),
        BadgeKind::DetachedHead => lead.push("check"),
        BadgeKind::PullRequest => trail.push("git-pull-request"),
        BadgeKind::Branch => {
            if badge.head {
                lead.push("check");
            }
            // Where the branch lives: this computer, a remote, or both.
            if badge.local {
                trail.push("laptop");
            }
            if !badge.remotes.is_empty() {
                trail.push("cloud");
            }
            if pr {
                trail.push("git-pull-request");
            }
        }
    }
    (lead, trail)
}

/// `text`, cut with an ellipsis to fit `width`.
pub(super) fn truncate(
    painter: &Painter,
    text: &str,
    font: &FontId,
    width: f32,
    keep_end: bool,
) -> String {
    let fits = |s: &str| {
        painter
            .layout_no_wrap(s.to_owned(), font.clone(), Color32::WHITE)
            .size()
            .x
            <= width
    };
    crate::settings::fit_branch_name(text, keep_end, fits)
}
