//! Commit message layout, author identity and text measurement.

use super::*;

pub(super) fn message_section(
    ui: &mut Ui,
    acts: &mut Vec<Action>,
    detail: &CommitDetail,
    reword: &mut Option<Reword>,
    editable: bool,
    room: f32,
) {
    let (subject, body) = detail
        .message
        .split_once('\n')
        .map(|(s, b)| (s, b.trim()))
        .unwrap_or((detail.message.as_str(), ""));
    let mut close = false;
    match reword.as_mut().filter(|r| r.hash == detail.hash) {
        Some(r) => {
            if let Some(save) = reword_editor(ui, r) {
                let message = r.message();
                if save && message != detail.message.trim() {
                    acts.push(Action::Reword {
                        hash: detail.hash.clone(),
                        message,
                    });
                }
                close = true;
            }
        }
        None => {
            let clicked = fitted_message(ui, detail, subject, body, editable, room);
            if clicked && editable {
                *reword = Some(Reword {
                    hash: detail.hash.clone(),
                    subject: subject.to_owned(),
                    body: body.to_owned(),
                    focus: true,
                    fill: false,
                });
            }
        }
    }
    if close {
        *reword = None;
    }
}

/// Description rows shown while folded: a longer one gets _Show more_ (#213).
pub(super) const PREVIEW_ROWS: usize = 4;

/// Whether long descriptions are unfolded: one choice for every commit, so going from commit to
/// commit keeps the layout.
fn unfolded_id() -> egui::Id {
    egui::Id::new("commit-message-unfolded")
}

/// `subject` and `body`, cut to `room` and, folded, to [`PREVIEW_ROWS`] description rows: the
/// description first, then the title down to its first row. A description cut that way gets
/// _Show more_ / _Show less_; what was cut also shows on hover. Returns whether it was
/// double-clicked.
pub(super) fn fitted_message(
    ui: &mut Ui,
    detail: &CommitDetail,
    subject: &str,
    body: &str,
    editable: bool,
    room: f32,
) -> bool {
    let t = Tokens::get(ui.ctx());
    let sense = if editable {
        Sense::click()
    } else {
        Sense::hover()
    };
    let gap = ui.spacing().item_spacing.y;
    let budget = room - gap;
    let title = RichText::new(subject).font(theme::semibold(15.0));
    let desc = RichText::new(body).color(t.text_dim);
    let (title_rows, title_h) = text_rows(ui, title.clone());
    let (body_rows, body_h) = if body.is_empty() {
        (0, 0.0)
    } else {
        text_rows(ui, desc.clone())
    };
    let fit = |room: f32, h: f32| (room / h).floor().max(0.0) as usize;
    let unfolded = ui
        .data(|d| d.get_temp::<bool>(unfolded_id()))
        .unwrap_or(false);
    // Folded, at most the preview; a description that fits it needs no expander.
    let foldable = body_rows > PREVIEW_ROWS;
    let (show_title, show_body) = if unfolded && foldable {
        (title_rows, body_rows)
    } else {
        let show_title = fit(budget, title_h).clamp(1, title_rows);
        let show_body = if show_title < title_rows {
            0
        } else {
            let room = budget - title_rows as f32 * title_h - gap - 2.0;
            fit(room, body_h).min(body_rows).min(PREVIEW_ROWS)
        };
        (show_title, show_body)
    };
    let hint = if editable {
        Some(crate::i18n::text("repo-core-0874"))
    } else if detail.in_head && detail.merges_after {
        Some(crate::i18n::text("repo-core-0875"))
    } else {
        None
    };
    // What was cut shows on hover.
    let full = (show_title < title_rows || show_body < body_rows).then(|| detail.message.trim());
    let tip = match (full, hint) {
        (Some(m), Some(h)) => Some(format!("{h}\n\n{m}")),
        (Some(m), None) => Some(m.to_owned()),
        (None, h) => h.map(str::to_owned),
    };
    let message = |ui: &mut Ui, text: RichText, rows: usize| {
        let job = clamped(ui, text, rows);
        let r = ui.add(
            egui::Label::new(job)
                .wrap()
                .sense(sense)
                .show_tooltip_when_elided(false),
        );
        match &tip {
            Some(tip) => r.on_hover_text(tip.as_str()),
            None => r,
        }
        .double_clicked()
    };
    let mut clicked = message(ui, title, show_title);
    if show_body > 0 {
        ui.add_space(2.0);
        clicked |= message(ui, desc, show_body);
    }
    if foldable {
        expander(ui, unfolded);
    }
    clicked
}

/// _Show more_ / _Show less_ under a long description, a disclosure for screen readers.
fn expander(ui: &mut Ui, unfolded: bool) {
    let t = Tokens::get(ui.ctx());
    let (label, icon) = if unfolded {
        (crate::i18n::text("commit-details-show-less"), "chevron-up")
    } else {
        (
            crate::i18n::text("commit-details-show-more"),
            "chevron-down",
        )
    };
    let row = ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 2.0;
        let link = ui.add(
            egui::Label::new(RichText::new(label).color(t.accent).small()).sense(Sense::click()),
        );
        crate::icons::inline(ui, icon, 11.0, t.accent);
        link
    });
    let link = row.inner;
    crate::a11y::expanded(&link, label, unfolded);
    if link.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    if link.clicked() {
        ui.data_mut(|d| d.insert_temp(unfolded_id(), !unfolded));
    }
}

/// Author line with an initials avatar, and the committer when someone else.
pub(super) fn author_line(ui: &mut Ui, detail: &CommitDetail) {
    let t = Tokens::get(ui.ctx());
    ui.horizontal(|ui| {
        let (rect, _) = ui.allocate_exact_size(vec2(32.0, 32.0), Sense::hover());
        widgets::avatar(ui, rect.center(), 16.0, &detail.author, &detail.email);
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 2.0;
            ui.add(
                egui::Label::new(RichText::new(&detail.author).font(theme::medium(12.5)))
                    .truncate(),
            )
            .on_hover_text(format!("{} <{}>", detail.author, detail.email));
            ui.add(
                egui::Label::new(
                    RichText::new(&detail.author_date)
                        .color(t.text_faint)
                        .font(theme::mono(11.0)),
                )
                .truncate(),
            )
            .on_hover_text(&detail.author_date);
        });
    });
    if detail.committer != detail.author {
        let text = crate::i18n::message(
            "repo-core-0877",
            &[
                ("committer", detail.committer.to_string().into()),
                ("commit-date", detail.commit_date.to_string().into()),
            ],
        );
        ui.add(egui::Label::new(RichText::new(&text).color(t.text_faint).small()).truncate())
            .on_hover_text(text);
    }
}

/// The commit's hash, with a copy button, its parents, each selecting it when clicked, and
/// its `builds` on a forge (with their status if it's known).
/// How many rows `text` wraps to across the space left in `ui`, and how tall each is.
pub(super) fn text_rows(ui: &Ui, text: RichText) -> (usize, f32) {
    let galley = egui::WidgetText::from(text).into_galley(
        ui,
        Some(egui::TextWrapMode::Wrap),
        ui.available_width(),
        egui::FontSelection::Default,
    );
    let rows = galley.rows.len().max(1);
    (rows, galley.size().y / rows as f32)
}

/// `text` wrapped to at most `rows` rows, the last one ending in "…" when cut.
pub(super) fn clamped(ui: &Ui, text: RichText, rows: usize) -> egui::text::LayoutJob {
    let job = egui::WidgetText::from(text).into_layout_job(
        ui.style(),
        egui::FontSelection::Default,
        ui.text_valign(),
    );
    let mut job = std::sync::Arc::unwrap_or_clone(job);
    job.wrap.max_rows = rows;
    job
}
