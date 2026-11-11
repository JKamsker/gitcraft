//! Presentation responsibilities for choreography.
use super::*;

pub(in crate::repo::review_workflow) fn show(
    ui: &mut egui::Ui,
    s: &mut State,
    repo: &Path,
    acts: &mut Vec<Action>,
) {
    ui.heading(crate::i18n::text(
        "repo-workflows-review-label-release-choreography-across-repositories-ebc4ee",
    ));
    ui.label(crate::i18n::text(
        "repo-workflows-review-label-give-each-repository-deployed-optional-bridge-7a467e",
    ));
    let mut remove = None;
    for (i, component) in s.plan.components.iter_mut().enumerate() {
        ui.group(|ui| {
            ui.horizontal(|ui| {
                ui.label(crate::i18n::message(
                    "repo-workflows-review-message-component-number-2d0dbb",
                    &[("number", (i + 1).into())],
                ));
                if ui
                    .small_button(crate::i18n::text(
                        "repo-workflows-review_workflow-show-c3812fc4",
                    ))
                    .clicked()
                {
                    remove = Some(i);
                }
            });
            ui.add(
                egui::TextEdit::singleline(&mut component.name).hint_text(crate::i18n::text(
                    "repo-workflows-review-label-identifier-e-g-backend-593c47",
                )),
            );
            ui.add(
                egui::TextEdit::singleline(&mut component.repo).hint_text(crate::i18n::text(
                    "repo-workflows-review-label-repository-path-same-machine-as-this-board-1839cc",
                )),
            );
            ui.label(crate::i18n::text(
                "repo-workflows-review-label-ordered-stages-one-label-revision-per-line-fir-e2768b",
            ));
            ui.text_edit_multiline(&mut component.revisions);
        });
    }
    if let Some(i) = remove {
        s.plan.components.remove(i);
    }
    if ui
        .button(crate::i18n::text(
            "repo-workflows-review-label-add-repository-component-e7a69c",
        ))
        .clicked()
    {
        s.plan.components.push(Component {
            repo: repo.to_string_lossy().into(),
            ..Default::default()
        });
    }
    let names: Vec<_> = s.plan.components.iter().map(|c| c.name.clone()).collect();
    let mut remove = None;
    for (i, rule) in s.plan.rules.iter_mut().enumerate() {
        ui.group(|ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(crate::i18n::text("repo-workflows-review-label-when-cf9c7a"));
                egui::ComboBox::from_id_salt(("rule-dependent", i))
                    .selected_text(&rule.dependent)
                    .show_ui(ui, |ui| {
                        for n in &names {
                            ui.selectable_value(&mut rule.dependent, n.clone(), n);
                        }
                    });
                ui.label(crate::i18n::text("repo-workflows-review-label-reaches-stage-b0463b"));
                ui.add(egui::DragValue::new(&mut rule.at).range(1..=3));
                ui.label(crate::i18n::text("repo-workflows-review-label-require-c4d0cf"));
                egui::ComboBox::from_id_salt(("rule-prerequisite", i))
                    .selected_text(&rule.prerequisite)
                    .show_ui(ui, |ui| {
                        for n in &names {
                            ui.selectable_value(&mut rule.prerequisite, n.clone(), n);
                        }
                    });
                ui.label(crate::i18n::text("repo-workflows-review-label-at-least-stage-aeda37"));
                ui.add(egui::DragValue::new(&mut rule.min).range(1..=3));
                if ui.small_button(crate::i18n::text("repo-workflows-review-label-remove-rule-3fab73")).clicked() {
                    remove = Some(i);
                }
            });
            ui.add(
                egui::TextEdit::singleline(&mut rule.reason)
                    .hint_text(crate::i18n::text("repo-workflows-review-label-e-g-backend-supports-both-protocols-before-sdk-b4f54d")),
            );
        });
    }
    if let Some(i) = remove {
        s.plan.rules.remove(i);
    }
    if ui
        .button(crate::i18n::text(
            "repo-workflows-review-label-add-ordering-compatibility-rule-e45b80",
        ))
        .clicked()
    {
        s.plan.rules.push(Rule {
            at: 1,
            min: 1,
            ..Default::default()
        });
    }
    ui.add(
        egui::TextEdit::multiline(&mut s.plan.command).hint_text(crate::i18n::text(
            "repo-workflows-review-label-compatibility-command-4b1ecc",
        )),
    );
    ui.weak(crate::i18n::text(
        "repo-workflows-review-label-the-command-receives-gitcraft-component-name-s-a6ca30",
    ));
    ui.checkbox(
        &mut s.plan.trusted,
        crate::i18n::text(
            "repo-workflows-review-label-i-trust-this-compatibility-command-and-the-rep-46eb9c",
        ),
    );
    ui.horizontal_wrapped(|ui| {
        for (label, act) in [
            (
                crate::i18n::text("repo-workflows-review-label-save-board-3bbd88"),
                Act::Save(s.plan.clone()),
            ),
            (
                crate::i18n::text("repo-workflows-review-label-load-board-and-evidence-28df38"),
                Act::Load,
            ),
            (
                crate::i18n::text(
                    "repo-workflows-review-label-plan-order-without-running-tests-47f369",
                ),
                Act::Plan(s.plan.clone()),
            ),
            (
                crate::i18n::text(
                    "repo-workflows-review-label-rehearse-every-permitted-combination-16276e",
                ),
                Act::Rehearse(s.plan.clone()),
            ),
        ] {
            if ui.add_enabled(!s.busy, egui::Button::new(label)).clicked() {
                acts.push(Action::ReviewTools(ToolsAct::Choreography(act)));
            }
        }
        if s.busy
            && ui
                .button(crate::i18n::text(
                    "repo-workflows-review-label-stop-rehearsal-1464d4",
                ))
                .clicked()
        {
            acts.push(Action::ReviewTools(ToolsAct::Choreography(Act::Stop)));
        }
    });
    if let Some(e) = &s.error {
        ui.label(crate::repo::messages::display_status(e));
    }
    if let Some(r) = &s.report {
        ui.label(if r.tested {
            crate::i18n::text(
                "repo-workflows-review-label-observed-compatibility-under-your-recorded-com-8785b1",
            )
        } else {
            crate::i18n::text(
                "repo-workflows-review-label-candidate-plan-only-no-compatibility-test-exec-afa4f9",
            )
        });
        if r.stopped {
            ui.label(crate::i18n::text(
                "repo-workflows-review-label-stopped-untested-combinations-cannot-establish-627b12",
            ));
        }
        if let Some(route) = &r.route {
            for pair in route.windows(2) {
                if let Some(i) = pair[0].iter().zip(&pair[1]).position(|(a, b)| a != b) {
                    let stage = &r.sources[i].stages[pair[1][i]];
                    ui.label(format!(
                        "{} → {} ({})",
                        r.sources[i].name,
                        display_stage(stage),
                        crate::repo::short(&stage.commit)
                    ));
                }
            }
        } else {
            ui.label(crate::i18n::text(
                "repo-workflows-review-label-no-permitted-passing-release-order-was-establi-244ee9",
            ));
        }
        egui::CollapsingHeader::new(crate::i18n::message("repo-workflows-review-message-count-one-one-combination-and-its-evidence-oth-e7b196", &[("count", (r.states.len()).into())]))
        .id_salt("review-release-combination-evidence")
        .show(ui, |ui| {
            for combination in &r.states {
                ui.group(|ui| {
                    ui.monospace(
                        r.sources
                            .iter()
                            .zip(&combination.stages)
                            .map(|(source, &i)| {
                                format!(
                                    "{}={} ({})",
                                    source.name,
                                    display_stage(&source.stages[i]),
                                    crate::repo::short(&source.stages[i].commit)
                                )
                            })
                            .collect::<Vec<_>>()
                            .join(" · "),
                    );
                    if !combination.allowed {
                        ui.label(crate::i18n::text("repo-workflows-review-label-excluded-by-your-declared-ordering-rules-2c5680"));
                    } else if let Some(error) = &combination.error {
                        ui.label(crate::repo::messages::display_status(error));
                    } else if let Some(observed) = &combination.observed {
                        contracts::show_observation(ui, observed);
                    } else {
                        ui.label(crate::i18n::text("repo-workflows-review-label-not-tested-51e3a0"));
                    }
                });
            }
        });
        for e in &r.cleanup {
            ui.label(crate::i18n::message(
                "repo-workflows-review-message-scratch-cleanup-needs-attention-detail-1b274a",
                &[("detail", (crate::repo::messages::display_status(e)).into())],
            ));
        }
    }
}
