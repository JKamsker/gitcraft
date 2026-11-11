//! Presentation responsibilities for echo.
use super::*;

pub(in crate::repo::review_workflow) fn show(
    ui: &mut egui::Ui,
    s: &mut State,
    acts: &mut Vec<Action>,
) {
    ui.heading(crate::i18n::text(
        "repo-workflows-review-label-reviewer-echo-a4516c",
    ));
    ui.weak(crate::i18n::text(
        "repo-workflows-review-label-enabled-push-checks-conservatively-scan-all-li-fbe58a",
    ));
    ui.label(crate::i18n::text(
        "repo-workflows-review-label-opt-in-to-collect-your-own-published-comments-4cf4a1",
    ));
    let mut enabled = s.book.enabled;
    if ui
        .checkbox(
            &mut enabled,
            crate::i18n::text(
                "repo-workflows-review-label-enable-personal-reviewer-rules-before-push-ope-f97e09",
            ),
        )
        .changed()
    {
        let mut book = s.book.clone();
        book.enabled = enabled;
        acts.push(Action::ReviewTools(ToolsAct::Echo(Act::Save(book))));
    }
    ui.horizontal_wrapped(|ui| {
        for (label, act) in [
            (
                crate::i18n::text("repo-workflows-review-label-load-rulebook-b28937"),
                Act::Load,
            ),
            (
                crate::i18n::text(
                    "repo-workflows-review-label-import-my-comments-from-open-pr-9c5c90",
                ),
                Act::Import,
            ),
            (
                crate::i18n::text(
                    "repo-workflows-review-label-suggest-rules-with-selected-ai-7b58fa",
                ),
                Act::Infer,
            ),
            (
                crate::i18n::text("repo-workflows-review-label-check-outgoing-diff-5b59d4"),
                Act::Check,
            ),
        ] {
            if ui.add_enabled(!s.busy, egui::Button::new(label)).clicked() {
                acts.push(Action::ReviewTools(ToolsAct::Echo(act)));
            }
        }
    });
    ui.weak(crate::i18n::message(
        "repo-workflows-review-message-comments-one-one-cited-comment-other-comments-2a82bb",
        &[
            ("comments", (s.book.comments.len()).into()),
            ("rules", (s.book.rules.len()).into()),
        ],
    ));
    if s.busy {
        ui.add(crate::spinner::Spinner::new());
        if ui
            .button(crate::i18n::text(
                "repo-workflows-review-label-stop-ai-collection-job-0ee900",
            ))
            .clicked()
        {
            acts.push(Action::ReviewTools(ToolsAct::Echo(Act::Stop)));
        }
    }
    if let Some(e) = &s.error {
        ui.label(crate::repo::messages::display_status(e));
    }
    let mut edited = false;
    for rule in &mut s.book.rules {
        ui.group(|ui| {
            edited |= ui.checkbox(&mut rule.enabled, &rule.summary).changed();
            edited |= ui.text_edit_singleline(&mut rule.pattern).changed();
            if rule.source.starts_with("https://") || rule.source.starts_with("http://") {
                ui.hyperlink_to(
                    crate::i18n::text(
                        "repo-workflows-review-label-your-original-review-comment-e2c95e",
                    ),
                    &rule.source,
                );
            }
        });
    }
    if edited {
        acts.push(Action::ReviewTools(ToolsAct::Echo(Act::Save(
            s.book.clone(),
        ))));
    }
    for hit in &s.hits {
        ui.group(|ui| {
            ui.label(crate::i18n::message(
                "repo-workflows-review-message-you-d-flag-this-summary-path-line-f00dec",
                &[
                    ("summary", (hit.summary.as_str()).into()),
                    ("path", (hit.path.as_str()).into()),
                    ("line", (hit.line.to_string()).into()),
                ],
            ));
            ui.monospace(&hit.text);
            ui.hyperlink_to(
                crate::i18n::text("repo-workflows-review-label-your-comment-9fc732"),
                &hit.url,
            );
        });
    }
    if s.checking_push {
        ui.add(crate::spinner::Spinner::new());
        ui.label(crate::i18n::text(
            "repo-workflows-review-label-checking-pinned-push-sources-ea23aa",
        ));
    }
    if let Some(waiting) = &s.pending_push {
        ui.label(crate::i18n::message(
            "repo-workflows-review-message-pending-operation-operation-a0bcce",
            &[(
                "operation",
                (crate::repo::messages::display_operation(&waiting.op.label)).into(),
            )],
        ));
        if ui
            .button(crate::i18n::text(
                "repo-workflows-review-label-i-reviewed-these-observations-continue-this-op-25680b",
            ))
            .clicked()
        {
            acts.push(Action::ReviewTools(ToolsAct::Echo(Act::PushAnyway(
                waiting.stamp.clone(),
            ))));
        }
        if ui
            .button(crate::i18n::text(
                "repo-workflows-review-label-cancel-this-push-operation-cd2c32",
            ))
            .clicked()
        {
            acts.push(Action::ReviewTools(ToolsAct::Echo(Act::CancelPush)));
        }
    }
}
