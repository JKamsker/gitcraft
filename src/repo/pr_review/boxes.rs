//! The boxes a comment on the page or a review is written in.

use egui::{Align2, RichText, Sense, Ui, pos2, vec2};

use crate::icons;
use crate::pr_post::{self, Verdict};
use crate::pr_threads::PrExtras;
use crate::repo::Action;
use crate::theme::{self, Tokens};

use super::{Place, PrAct, PrDraft, error_label, pr_acts};

/// The box on the pull request's page.
pub(in crate::repo) fn comment_box(ui: &mut Ui, draft: &mut PrDraft, acts: &mut Vec<Action>) {
    ui.add_enabled(
        !draft.posting,
        egui::TextEdit::multiline(&mut draft.comment)
            .hint_text(crate::i18n::text(
                "repo-workflows-pr_review-comment-box-e5841068",
            ))
            .desired_width(f32::INFINITY)
            .desired_rows(3),
    );
    ui.add_space(4.0);
    error_label(ui, draft.error_at(Place::Comment));
    let can_send = !draft.comment.trim().is_empty() && !draft.posting;
    if ui
        .add_enabled(
            can_send,
            egui::Button::new(crate::i18n::text(
                "repo-workflows-pr_review-comment-box-44f5e3fb",
            )),
        )
        .clicked()
    {
        pr_acts(acts, PrAct::PostComment);
    }
}

/// The box under the files: the review being written.
pub(in crate::repo) fn review_box(
    ui: &mut Ui,
    draft: &mut PrDraft,
    extras: &PrExtras,
    own: bool,
    open: bool,
    acts: &mut Vec<Action>,
) {
    let t = Tokens::get(ui.ctx());
    ui.spacing_mut().item_spacing.y = 6.0;
    ui.label(
        RichText::new(crate::i18n::text(
            "repo-workflows-pr_review-review-box-182e4992",
        ))
        .font(theme::semibold(10.5))
        .color(t.text_faint),
    );
    super::native_discussions::show(ui, draft, extras, acts);
    if extras.review_features.pending {
        pending_list(ui, extras, draft.posting, acts);
    }
    ui.add_enabled(
        !draft.posting,
        egui::TextEdit::multiline(&mut draft.review)
            .hint_text(crate::i18n::text(
                "repo-workflows-pr_review-review-box-a0208868",
            ))
            .desired_width(f32::INFINITY)
            .desired_rows(2),
    );
    error_label(ui, draft.error_at(Place::Review));
    if own && open {
        ui.label(
            RichText::new(crate::i18n::text(
                "repo-workflows-pr_review-review-box-9b1f2050",
            ))
            .color(t.text_faint)
            .small(),
        );
    }
    ui.horizontal_wrapped(|ui| verdict_buttons(ui, draft, extras, own, open, acts));
}

/// A button for each verdict the review can go out with, and one to discard the pending one.
fn verdict_buttons(
    ui: &mut Ui,
    draft: &PrDraft,
    extras: &PrExtras,
    own: bool,
    open: bool,
    acts: &mut Vec<Action>,
) {
    let count = extras.pending_comments().count();
    for (verdict, label) in [
        (
            Verdict::Comment,
            crate::i18n::text("repo-workflows-pr_review-verdict-buttons-44f5e3fb"),
        ),
        (
            Verdict::Approve,
            crate::i18n::text("repo-workflows-pr_review-verdict-buttons-6007acbe"),
        ),
        (
            Verdict::RequestChanges,
            crate::i18n::text("repo-workflows-pr_review-verdict-buttons-cb5d9f98"),
        ),
    ] {
        if (verdict == Verdict::Approve && !extras.review_features.approve)
            || (verdict == Verdict::RequestChanges && !extras.review_features.request_changes)
        {
            continue;
        }
        // Whatever is pending can still go out, but a verdict comes too late.
        if !open && verdict != Verdict::Comment {
            continue;
        }
        let problem = pr_post::review_problem(verdict, &draft.review, count);
        let reviews_own = own && verdict != Verdict::Comment;
        let enabled = problem.is_none() && !reviews_own && !draft.posting;
        let button = ui.add_enabled(enabled, egui::Button::new(label));
        if button.clicked() {
            pr_acts(acts, PrAct::Submit(verdict));
        }
        if let Some(problem) = problem {
            button.on_disabled_hover_text(problem);
        }
    }
    if extras.pending_review.is_some()
        && ui
            .add_enabled(
                !draft.posting,
                egui::Button::new(crate::i18n::text(
                    "repo-workflows-pr_review-verdict-buttons-eb1a70e3",
                )),
            )
            .on_hover_text(crate::i18n::text(
                "repo-workflows-pr_review-verdict-buttons-6832518c",
            ))
            .clicked()
    {
        pr_acts(acts, PrAct::DiscardPending);
    }
    if draft.posting {
        ui.add(crate::spinner::Spinner::new());
    }
}

/// The comments of the pending review, each with a button that takes it out; not while
/// something is sent, when the list may be about to change (the last one taken out throws the
/// review away, which a list behind the times can't tell).
fn pending_list(ui: &mut Ui, extras: &PrExtras, posting: bool, acts: &mut Vec<Action>) {
    let t = Tokens::get(ui.ctx());
    let waiting: Vec<_> = extras.pending_comments().collect();
    if waiting.is_empty() {
        return;
    }
    ui.label(
        RichText::new(crate::i18n::message(
            "repo-workflows-pr-pending-comment-count",
            &[("count", (waiting.len()).into())],
        ))
        .color(t.accent_fg)
        .small(),
    );
    for (thread, comment) in waiting {
        let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 22.0), Sense::hover());
        let y = rect.center().y;
        let x = egui::Rect::from_center_size(pos2(rect.right() - 9.0, y), vec2(18.0, 18.0));
        let p = ui.painter_at(rect.with_max_x(x.left() - 4.0));
        let line = thread.line.map_or(String::new(), |n| format!(":{n}"));
        p.text(
            pos2(rect.left(), y),
            Align2::LEFT_CENTER,
            format!("{}{line}  {}", thread.path, first_line(&comment.body)),
            egui::FontId::proportional(11.5),
            t.text_dim,
        );
        let sense = if posting {
            Sense::hover()
        } else {
            Sense::click()
        };
        let remove = ui
            .interact(x, ui.id().with(("pending-remove", &comment.id)), sense)
            .on_hover_text(crate::i18n::text(
                "repo-workflows-pr_review-pending-list-d74b2a22",
            ));
        if !posting {
            crate::repo::widgets::a11y(&remove, egui::WidgetType::Button, || {
                let what = first_line(&comment.body);
                crate::i18n::message(
                    "repo-workflows-pr_review-pending-list-761b2619",
                    &[
                        ("thread-path-1", (thread.path).clone().into()),
                        ("line", (line).clone().into()),
                        ("what", (what).clone().into()),
                    ],
                )
            });
        }
        if remove.hovered() && !posting {
            ui.painter().rect_filled(x, t.radius_sm, t.pressed);
        }
        icons::paint(ui, x, "x", 11.0, t.text_faint);
        if remove.clicked() && !posting {
            pr_acts(acts, PrAct::RemovePending(comment.id.clone()));
        }
    }
}

fn first_line(text: &str) -> &str {
    text.lines().next().unwrap_or("")
}

#[cfg(test)]
mod native_tests {
    use super::*;
    #[test]
    fn native_review_only_shows_supported_verdicts() {
        let ctx = egui::Context::default();
        crate::theme::install_fonts(&ctx);
        let extras = PrExtras {
            review_features: crate::pr_threads::ReviewFeatures {
                pending: false,
                request_changes: false,
                approve: true,
                apply_suggestions: false,
            },
            ..Default::default()
        };
        let mut draft = PrDraft {
            review: "Reviewed".into(),
            ..Default::default()
        };
        let out = ctx.run_ui(egui::RawInput::default(), |ui| {
            review_box(ui, &mut draft, &extras, false, true, &mut Vec::new())
        });
        let text = out
            .shapes
            .iter()
            .filter_map(|s| {
                if let egui::Shape::Text(t) = &s.shape {
                    Some(t.galley.text())
                } else {
                    None
                }
            })
            .collect::<Vec<_>>()
            .join("\n");
        assert!(text.contains("Approve"));
        assert!(text.contains("Comment"));
        assert!(!text.contains("Request changes"));
        assert!(!text.contains("Discard"));
        out.drop_without_applying_deltas();
    }
}
