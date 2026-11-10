//! Presentation responsibilities for contracts.
use super::*;

pub(in crate::repo::review_workflow) fn show(
    ui: &mut egui::Ui,
    s: &mut State,
    repo: &Path,
    branches: &[String],
    acts: &mut Vec<Action>,
) {
    ui.heading(crate::i18n::text(
        "repo-workflows-review-label-backport-by-contract-247835",
    ));
    ui.label(crate::i18n::text(
        "repo-workflows-review-label-drag-a-graph-commit-onto-a-release-below-or-en-775ed9",
    ));
    egui::CollapsingHeader::new(crate::i18n::text(
        "repo-workflows-review-label-drop-a-fix-onto-a-release-c1a96f",
    ))
    .id_salt("repo-workflows-review-label-drop-a-fix-onto-a-release-c1a96f")
    .show(ui, |ui| {
        for branch in branches {
            let response = ui.group(|ui| ui.label(branch)).response;
            if let Some(payload) = response.dnd_release_payload::<Payload>()
                && payload.repository() == repo
                && let DragItem::Commit(commit) = &payload.item
            {
                s.plan.fix = commit.hash.clone();
                s.plan.targets = branch.clone();
            }
        }
    });
    ui.add(
        egui::TextEdit::singleline(&mut s.plan.fix).hint_text(crate::i18n::text(
            "repo-workflows-review-label-fix-commit-c192f6",
        )),
    );
    ui.label(crate::i18n::text(
        "repo-workflows-review-label-targets-one-per-line-optional-target-adaptatio-074e6e",
    ));
    ui.text_edit_multiline(&mut s.plan.targets);
    ui.add(
        egui::TextEdit::multiline(&mut s.plan.command).hint_text(crate::i18n::text(
            "repo-workflows-review-label-regression-command-e-g-cargo-test-token-lifeti-75b901",
        )),
    );
    ui.checkbox(
        &mut s.plan.trusted,
        crate::i18n::text(
            "repo-workflows-review-label-i-trust-this-command-and-the-repository-code-i-fcc4fd",
        ),
    );
    ui.horizontal(|ui| {
        if ui
            .add_enabled(
                !s.busy && s.plan.trusted,
                egui::Button::new(crate::i18n::text(
                    "repo-workflows-review-label-rehearse-backports-and-regression-test-bf2331",
                )),
            )
            .clicked()
        {
            acts.push(Action::ReviewTools(ToolsAct::Contract(Act::Run(
                s.plan.clone(),
            ))));
        }
        if ui
            .button(crate::i18n::text(
                "repo-workflows-review-label-load-preserved-results-23b59f",
            ))
            .clicked()
        {
            acts.push(Action::ReviewTools(ToolsAct::Contract(Act::Load)));
        }
        if s.busy
            && ui
                .button(crate::i18n::text(
                    "repo-workflows-review-label-stop-rehearsal-1464d4",
                ))
                .clicked()
        {
            acts.push(Action::ReviewTools(ToolsAct::Contract(Act::Stop)));
        }
    });
    if let Some(rows) = &s.rows {
        match rows {
            Err(e) => {
                ui.label(crate::repo::messages::display_status(e));
            }
            Ok(rows) => {
                for row in rows {
                    ui.group(|ui| {
                        ui.strong(crate::i18n::message("repo-workflows-review-message-target-at-commit-9e4831", &[("target", (row.target.as_str()).into()), ("commit", (crate::repo::short(&row.tip)).into())]));
                        ui.label(if let Some(tree) = &row.applied {
                            crate::i18n::message("repo-workflows-review-message-patch-applied-staged-tree-tree-6e0863", &[("tree", (tree.as_str()).into())])
                        } else {
                            crate::i18n::text("repo-workflows-review-label-patch-did-not-apply-a1c90f").into()
                        });
                        if let Some(e) = &row.error {
                            ui.label(crate::repo::messages::display_status(e));
                        }
                        if let Some(test) = &row.test {
                            show_observation(ui, test);
                        } else {
                            ui.label(crate::i18n::text("repo-workflows-review-label-behavior-not-tested-9f5ef2"));
                        }
                        if let Some((original, adapted)) = &row.adaptation {
                            egui::CollapsingHeader::new(crate::i18n::text("repo-workflows-review-label-adaptation-next-to-original-df4874")).id_salt("repo-workflows-review-label-adaptation-next-to-original-df4874").show(
                                ui,
                                |ui| {
                                    ui.columns(2, |cols| {
                                        cols[0].label(crate::i18n::text("repo-workflows-review-label-original-fix-27d835"));
                                        cols[0].monospace(original);
                                        cols[1].label(crate::i18n::text("repo-workflows-review-label-adaptation-760fad"));
                                        cols[1].monospace(adapted);
                                    });
                                },
                            );
                        }
                        if let Some(e) = &row.cleanup {
                            ui.label(crate::i18n::message("repo-workflows-review-message-scratch-cleanup-requires-attention-detail-0f29d4", &[("detail", (crate::repo::messages::display_status(e)).into())]));
                        }
                    });
                }
            }
        }
    }
}
pub(super) fn observation_summary(test: &Observation) -> String {
    if test.stopped {
        crate::i18n::message(
            "repo-workflows-review-message-test-stopped-command-command-machine-machine-t-03203f",
            &[
                ("command", (test.command.as_str()).into()),
                ("machine", (test.machine.as_str()).into()),
                ("tree", (test.tree.as_str()).into()),
            ],
        )
    } else if let Some(code) = test.code {
        crate::i18n::message(
            "repo-workflows-review-message-test-exit-code-command-command-machine-machine-96779e",
            &[
                ("code", (code).into()),
                ("command", (test.command.as_str()).into()),
                ("machine", (test.machine.as_str()).into()),
                ("tree", (test.tree.as_str()).into()),
            ],
        )
    } else {
        crate::i18n::message(
            "repo-workflows-review-message-test-exit-unknown-command-command-machine-mach-b250e6",
            &[
                ("command", (test.command.as_str()).into()),
                ("machine", (test.machine.as_str()).into()),
                ("tree", (test.tree.as_str()).into()),
            ],
        )
    }
}
pub(in crate::repo::review_workflow) fn show_observation(ui: &mut egui::Ui, test: &Observation) {
    ui.label(observation_summary(test));
    egui::CollapsingHeader::new(crate::i18n::text(
        "repo-workflows-review-label-observed-output-49acbe",
    ))
    .id_salt("repo-workflows-review-label-observed-output-49acbe")
    .id_salt((&test.tree, &test.command))
    .show(ui, |ui| {
        ui.monospace(test.lines.join("\n"));
    });
}
