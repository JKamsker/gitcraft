//! Local-branch badges and row painting with stable menu identities.

use super::*;

/// The badge a branch's row shows for the first of `reasons` that has one: Merged, else Stale.
pub(super) fn badge(reasons: &[Reason]) -> Option<Badge> {
    if reasons.iter().any(Reason::is_merged) {
        Some(Badge::Merged)
    } else if reasons.iter().any(|r| matches!(r, Reason::Stale(_))) {
        Some(Badge::Stale)
    } else {
        None
    }
}

/// `Merged into main` from `merged into main`.
pub(super) fn capitalise(text: &str) -> String {
    let mut chars = text.chars();
    chars.next().map_or_else(String::new, |first| {
        first.to_uppercase().chain(chars).collect()
    })
}

/// A branch, `full` its whole ref: icon (a check for the checked-out one), name, ahead/behind
/// counts; and what clicking its build status asked for.
pub(super) fn branch_row(
    ui: &mut Ui,
    full: &str,
    row: LeafRow<'_>,
    is_head: bool,
    ahead: u32,
    behind: u32,
    marks: Marks,
) -> (Response, Option<Action>) {
    let Marks {
        gone,
        badge,
        pinned,
        in_worktree,
        has_pr,
        ci,
        hidden,
    } = marks;
    let t = Tokens::get(ui.ctx());
    // Known by the branch, not its place: a fetch or a branch made in the terminal shifts the
    // rows, and a menu left open (Merge, Rebase, Check out…) must stay this branch's.
    let (rect, resp) = widgets::keyed_row(ui, ROW_H, ("branch-row", full));
    widgets::a11y(&resp, egui::WidgetType::Button, || {
        let branch = labels::branch(labels::ref_name(full), is_head, ahead as _, behind as _);
        let notes = [
            (gone, crate::i18n::text("repo-core-3416")),
            (in_worktree, crate::i18n::text("repo-core-3417")),
            (has_pr, crate::i18n::text("repo-core-3418")),
            (hidden, crate::i18n::text("repo-core-3419")),
        ];
        labels::with_notes(&branch, &notes)
    });
    let painter = ui.painter_at(rect);
    if is_head {
        painter.rect_filled(rect, 0.0, t.accent.gamma_multiply(0.14));
    }
    if resp.hovered() {
        painter.rect_filled(rect, 0.0, t.hover);
    }
    let y = rect.center().y;
    let icon_rect =
        Rect::from_center_size(pos2(rect.left() + 34.0 + row.indent, y), vec2(16.0, 16.0));
    if is_head {
        icons::paint(ui, icon_rect, "check", 14.0, t.accent);
    } else if pinned {
        icons::paint(ui, icon_rect, "pin", 13.0, t.text_dim);
    } else {
        icons::paint(ui, icon_rect, "git-branch", 13.0, t.text_faint);
    }

    let mut right = rect.right() - 8.0;
    let small = theme::mono(11.0);
    if has_pr {
        let ir = Rect::from_center_size(pos2(right - 6.0, y), vec2(12.0, 12.0));
        icons::paint(ui, ir, "git-pull-request", 12.0, t.success);
        right = ir.left() - 6.0;
    }
    if in_worktree {
        let ir = Rect::from_center_size(pos2(right - 6.0, y), vec2(12.0, 12.0));
        icons::paint(ui, ir, "folder-tree", 12.0, t.text_dim);
        right = ir.left() - 6.0;
    }
    if gone {
        let r = painter.text(
            pos2(right, y),
            Align2::RIGHT_CENTER,
            crate::i18n::text("repo-core-3421"),
            theme::medium(10.5),
            t.text_faint,
        );
        right = r.left() - 6.0;
    }
    if let Some(badge) = badge {
        let (word, color) = match badge {
            Badge::Merged => (crate::i18n::text("repo-core-3422"), t.success),
            Badge::Stale => (crate::i18n::text("repo-core-3423"), t.warning),
        };
        let r = painter.text(
            pos2(right, y),
            Align2::RIGHT_CENTER,
            word,
            theme::medium(10.5),
            color,
        );
        right = r.left() - 6.0;
    }
    let mut ci_act = None;
    if let Some((sha, status)) = ci {
        let center = pos2(right - ci_status::DOT_R, y);
        ci_act = ci_status::dot(ui, resp.id.with("ci"), center, sha, status);
        right = center.x - ci_status::DOT_R - 6.0;
    }
    for (n, icon) in [(behind, "arrow-down"), (ahead, "arrow-up")] {
        if n > 0 {
            let r = painter.text(
                pos2(right, y),
                Align2::RIGHT_CENTER,
                n.to_string(),
                small.clone(),
                t.text_dim,
            );
            let ir = Rect::from_center_size(pos2(r.left() - 7.0, y), vec2(12.0, 12.0));
            icons::paint(ui, ir, icon, 11.0, t.text_dim);
            right = ir.left() - 6.0;
        }
    }
    let font = if is_head {
        theme::semibold(12.5)
    } else {
        egui::FontId::proportional(12.5)
    };
    painter
        .with_clip_rect(Rect::from_min_max(
            pos2(rect.left(), rect.top()),
            pos2(right, rect.bottom()),
        ))
        .text(
            pos2(rect.left() + 48.0 + row.indent, y),
            Align2::LEFT_CENTER,
            row.name,
            font,
            if hidden { t.text_faint } else { t.text },
        );
    (resp, ci_act)
}
