//! Presentation responsibilities for reproducer.
use super::*;

pub(super) fn exit_code(code: Option<i32>) -> String {
    code.map(crate::i18n::number).unwrap_or_else(|| {
        crate::i18n::text("repo-workflows-review-label-unknown-b23a6a").to_owned()
    })
}
pub(super) fn show_report(ui: &mut egui::Ui, r: &Report) {
    ui.label(crate::i18n::message(
        "repo-workflows-review-message-pinned-path-line-base-head-b813c6",
        &[
            ("path", (r.case.path.as_str()).into()),
            ("line", (r.case.line.as_str()).into()),
            ("base", (crate::repo::short(&r.case.base)).into()),
            ("head", (crate::repo::short(&r.case.head)).into()),
        ],
    ));
    ui.label(crate::i18n::text(
        "repo-workflows-review-label-exit-code-differences-can-include-build-failur-324428",
    ));
    ui.columns(2, |cols| {
        cols[0].strong(crate::i18n::text("repo-workflows-review-label-base-7b4736"));
        contracts::show_observation(&mut cols[0], &r.base);
        cols[1].strong(crate::i18n::text("repo-workflows-review-label-head-9bb0aa"));
        contracts::show_observation(&mut cols[1], &r.head);
    });
    for error in &r.cleanup {
        ui.label(crate::i18n::message(
            "repo-workflows-review-message-cleanup-needs-attention-detail-3403ce",
            &[(
                "detail",
                (crate::repo::messages::display_status(error)).into(),
            )],
        ));
    }
}
pub(in crate::repo::review_workflow) fn show(
    ui: &mut egui::Ui,
    s: &mut State,
    acts: &mut Vec<Action>,
) {
    ui.heading(crate::i18n::text(
        "repo-workflows-review-label-ai-objection-concrete-reproducer-55420a",
    ));
    ui.label(crate::i18n::text(
        "repo-workflows-review-label-start-with-a-rust-integration-test-generation-6cf14f",
    ));
    ui.horizontal(|ui| {
        if ui
            .button(crate::i18n::text(
                "repo-workflows-review-label-use-open-diff-pr-revisions-9fc2d9",
            ))
            .clicked()
        {
            acts.push(Action::ReviewTools(ToolsAct::Reproducer(Act::UseSelection)));
        }
        if ui
            .button(crate::i18n::text(
                "repo-workflows-review-label-load-preserved-case-and-observations-2177f5",
            ))
            .clicked()
        {
            acts.push(Action::ReviewTools(ToolsAct::Reproducer(Act::Load)));
        }
    });
    let before = s.case.clone();
    let c = &mut s.case;
    for (text, hint) in [
        (
            &mut c.base,
            crate::i18n::text("repo-workflows-review-label-base-revision-7438b8"),
        ),
        (
            &mut c.head,
            crate::i18n::text("repo-workflows-review-label-head-revision-5887ea"),
        ),
        (
            &mut c.path,
            crate::i18n::text("repo-workflows-review-label-file-path-2fb6d3"),
        ),
        (
            &mut c.line,
            crate::i18n::text("repo-workflows-review-label-pinned-hunk-line-ba3843"),
        ),
    ] {
        ui.add(egui::TextEdit::singleline(text).hint_text(hint));
    }
    ui.add(
        egui::TextEdit::multiline(&mut c.objection).hint_text(crate::i18n::text(
            "repo-workflows-review-label-concrete-objection-to-reproduce-f74adb",
        )),
    );
    if ui
        .add_enabled(
            !s.busy,
            egui::Button::new(crate::i18n::text(
                "repo-workflows-review-label-show-me-generate-a-test-with-selected-ai-dd497a",
            )),
        )
        .clicked()
    {
        acts.push(Action::ReviewTools(ToolsAct::Reproducer(Act::Generate(
            c.clone(),
        ))));
    }
    ui.add(
        egui::TextEdit::multiline(&mut c.source)
            .code_editor()
            .desired_rows(12)
            .desired_width(f32::INFINITY)
            .hint_text(crate::i18n::text(
                "repo-workflows-review-label-rust-integration-test-source-editable-adba55",
            )),
    );
    if c.image.is_empty() {
        c.image = "rust:1.95.0".into();
    }
    ui.add(
        egui::TextEdit::singleline(&mut c.image).hint_text(crate::i18n::text(
            "repo-workflows-review-label-preloaded-rust-docker-image-or-digest-70313d",
        )),
    );
    ui.weak(crate::i18n::text(
        "repo-workflows-review-label-source-is-streamed-from-raw-git-trees-blobs-wi-96131a",
    ));
    ui.weak(crate::i18n::text(
        "repo-workflows-review-label-docker-must-be-available-on-the-repository-s-m-286675",
    ));
    let mut content = c.clone();
    content.approved = before.approved;
    if content != before {
        c.approved = false;
    }
    ui.checkbox(
        &mut c.approved,
        crate::i18n::text(
            "repo-workflows-review-label-i-inspected-this-source-and-explicitly-approve-d28ee6",
        ),
    );
    if ui
        .add_enabled(
            !s.busy && c.approved,
            egui::Button::new(crate::i18n::text(
                "repo-workflows-review-label-run-identical-test-on-base-and-head-in-sandbox-813afc",
            )),
        )
        .clicked()
    {
        acts.push(Action::ReviewTools(ToolsAct::Reproducer(Act::Run(
            c.clone(),
        ))));
    }
    if s.busy {
        ui.add(crate::spinner::Spinner::new());
        if ui
            .button(crate::i18n::text(
                "repo-workflows-review-label-stop-generation-sandbox-run-350bbf",
            ))
            .clicked()
        {
            acts.push(Action::ReviewTools(ToolsAct::Reproducer(Act::Stop)));
        }
    }
    if let Some(e) = &s.error {
        ui.label(crate::repo::messages::display_status(e));
    }
    if let Some(generated) = &s.generated {
        egui::CollapsingHeader::new(crate::i18n::text("repo-workflows-review-label-generated-result-for-the-earlier-form-ebc96e")).id_salt("repo-workflows-review-label-generated-result-for-the-earlier-form-ebc96e").show(ui, |ui| {
            ui.label(format!(
                "{}:{} · {} → {}",
                generated.path, generated.line, generated.base, generated.head
            ));
            ui.monospace(&generated.source);
            if ui
                .button(crate::i18n::text("repo-workflows-review-label-use-this-generated-case-approval-required-agai-ee18e0"))
                .clicked()
            {
                acts.push(Action::ReviewTools(ToolsAct::Reproducer(Act::UseGenerated)));
            }
        });
    }
    if let Some(r) = &s.report {
        show_report(ui, r);
        if ui
            .button(crate::i18n::text(
                "repo-workflows-review-label-open-pinned-comparison-85160d",
            ))
            .clicked()
        {
            acts.push(Action::OpenDiff(DiffKey::Range {
                from: r.case.base.clone(),
                to: Some(r.case.head.clone()),
                path: r.case.path.clone(),
            }));
        }
    }
}
