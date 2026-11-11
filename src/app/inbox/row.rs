//! Inbox row layout and accessibility.

use super::*;

pub(super) fn one_line(
    ui: &Ui,
    text: String,
    font: FontId,
    color: Color32,
    width: f32,
) -> Arc<Galley> {
    let mut job =
        egui::text::LayoutJob::single_section(text, egui::TextFormat::simple(font, color));
    job.wrap = egui::text::TextWrapping {
        max_width: width.max(0.0),
        max_rows: 1,
        break_anywhere: true,
        overflow_character: Some('…'),
    };
    ui.painter().layout_job(job)
}

/// One pull request: its number and title, its repository and author, and why it's here.
pub(super) fn inbox_row(
    ui: &mut Ui,
    item: &Item,
    reasons: &[Reason],
    t: &Tokens,
) -> egui::Response {
    let (_, rect) = ui.allocate_space(vec2(ui.available_width(), 40.0));
    let resp = ui.interact(
        rect,
        ui.id().with(("inbox-row", item.forge, &item.pr.url)),
        Sense::click(),
    );
    crate::a11y::a11y(&resp, egui::WidgetType::Button, || {
        let reasons = reasons
            .iter()
            .map(|reason| reason.label())
            .collect::<Vec<_>>()
            .join(", ");
        crate::i18n::message(
            "interaction-home-inbox-a11y",
            &[
                ("forge", item.forge.name().into()),
                ("noun", item.forge.noun_label().into()),
                ("number", item.pr.number.to_string().into()),
                ("title", item.pr.title.as_str().into()),
                ("author", item.pr.author.as_str().into()),
                ("project", item.pr.project.as_str().into()),
                ("reasons", reasons.into()),
            ],
        )
    });
    if resp.hovered() {
        ui.painter().rect_filled(rect, t.radius, t.hover);
    }
    let y = rect.center().y;
    icons::paint(
        ui,
        Rect::from_center_size(pos2(rect.left() + 20.0, y), vec2(18.0, 18.0)),
        "git-pull-request",
        16.0,
        t.icon,
    );
    // The reasons on the right, the first always and the others as far as half the row
    // goes; the title keeps what is left.
    let font = FontId::proportional(11.0);
    let labels: Vec<(Arc<Galley>, Color32)> = reasons
        .iter()
        .map(|reason| {
            let color = match reason {
                Reason::ReviewRequested => t.accent,
                Reason::ChecksFailing => t.danger,
                Reason::NewComments(_) => t.warning,
            };
            let galley = ui
                .painter()
                .layout_no_wrap(reason.label(), font.clone(), color);
            (galley, color)
        })
        .collect();
    let mut used = 0.0;
    let mut fit = 0;
    for (galley, _) in &labels {
        let w = galley.size().x + if fit > 0 { 12.0 } else { 0.0 };
        if fit > 0 && used + w > rect.width() * 0.5 {
            break;
        }
        used += w;
        fit += 1;
    }
    let mut x = rect.right() - 8.0;
    for (galley, color) in labels.into_iter().take(fit).rev() {
        x -= galley.size().x;
        let at = pos2(x, y - galley.size().y / 2.0);
        ui.painter().galley(at, galley, color);
        x -= 12.0;
    }
    let left = rect.left() + 40.0;
    let width = x - 4.0 - left;
    let title = one_line(
        ui,
        crate::i18n::message(
            if item.pr.draft {
                "interaction-home-pr-draft-title"
            } else {
                "interaction-home-pr-title"
            },
            &[
                ("number", item.pr.number.to_string().into()),
                ("title", item.pr.title.as_str().into()),
            ],
        ),
        theme::medium(12.5),
        t.text,
        width,
    );
    ui.painter()
        .galley(pos2(left, y - 8.0 - title.size().y / 2.0), title, t.text);
    let repo = match &item.repo {
        Some(path) => super::super::workspaces::repo_name(path),
        None => item.pr.project.clone(),
    };
    let sub = one_line(
        ui,
        format!("{repo} · {} · {}", item.forge.name(), item.pr.author),
        FontId::proportional(11.0),
        t.text_faint,
        width,
    );
    ui.painter()
        .galley(pos2(left, y + 8.0 - sub.size().y / 2.0), sub, t.text_faint);
    let how = if item.forge.has_pages() && item.repo.is_some() {
        crate::i18n::text("interaction-home-open-tab")
    } else {
        crate::i18n::text("interaction-home-open-browser")
    };
    resp.on_hover_text(format!("{}\n{how}", item.pr.url))
        .on_hover_cursor(egui::CursorIcon::PointingHand)
}
