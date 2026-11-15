//! Bounded PR row text: badges and identity take priority over the independently elided route.
use super::*;

pub(super) fn paint(
    tab: &RepoTab,
    painter: &egui::Painter,
    rect: Rect,
    right: f32,
    pr: &PullRequest,
) -> Option<String> {
    let t = Tokens::get(painter.ctx());
    let left = rect.left() + 48.0;
    let y = rect.center().y;
    let (badges_left, badges) =
        crate::repo::forks::paint_badges(tab, painter, pos2(right, y - 7.0), pr.number, &pr.sha);
    line(
        painter,
        &format!("#{} {}", pr.number, pr.title),
        pos2(left, y - 7.0),
        badges_left - left - 8.0,
        12.5,
        t.text,
    );

    let status = if pr.draft {
        crate::i18n::text("repo-core-3450")
    } else {
        ""
    };
    let status_galley = painter.layout_no_wrap(
        status.into(),
        egui::FontId::proportional(11.0),
        t.text_faint,
    );
    let author_right = line(
        painter,
        &pr.author,
        pos2(left, y + 8.0),
        right - left - status_galley.size().x,
        11.0,
        t.text_faint,
    );
    let route_left = author_right + status_galley.size().x + 8.0;
    painter.galley(
        pos2(author_right, y + 8.0 - status_galley.size().y / 2.0),
        status_galley,
        t.text_faint,
    );
    line(
        painter,
        &route(pr, tab.hosting.as_ref()),
        pos2(route_left, y + 8.0),
        right - route_left,
        11.0,
        t.text_faint,
    );
    badges
}

/// Paint one elided line within its own width, returning its right edge.
fn line(
    p: &egui::Painter,
    text: &str,
    center_left: egui::Pos2,
    width: f32,
    size: f32,
    color: egui::Color32,
) -> f32 {
    if width <= 0.0 {
        return center_left.x;
    }
    let mut job = egui::text::LayoutJob::simple_singleline(
        text.into(),
        egui::FontId::proportional(size),
        color,
    );
    job.wrap = egui::text::TextWrapping::truncate_at_width(width);
    let galley = p.layout_job(job);
    let right = center_left.x + galley.size().x;
    p.galley(
        pos2(center_left.x, center_left.y - galley.size().y / 2.0),
        galley,
        color,
    );
    right
}

/// Repository identity comes from the forge, never from the PR author.
fn route(pr: &PullRequest, hosting: Option<&Hosting>) -> String {
    let base = hosting.map_or("unknown", |h| h.path.as_str());
    let head = pr
        .head_repo
        .as_deref()
        .unwrap_or(if pr.from_fork { "unknown" } else { base });
    format!("{head}:{} → {base}:{}", pr.branch, pr.base)
}

pub(super) fn tooltip(pr: &PullRequest, hosting: Option<&Hosting>, moved: bool) -> String {
    format!(
        "{}{} · {} · {}\n{}",
        if moved {
            crate::i18n::text("repo-core-3449")
        } else {
            ""
        },
        pr.author,
        crate::i18n::text(if pr.draft {
            "repo-workflows-pr_view-header-ebf12ef4"
        } else {
            "repo-workflows-pr_view-header-ed077f3d"
        }),
        route(pr, hosting),
        pr.url
    )
}

#[cfg(test)]
mod tests;
