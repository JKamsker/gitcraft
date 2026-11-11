//! Presentation responsibilities for radar.
use super::*;

pub(in crate::repo::review_workflow) fn show(
    ui: &mut egui::Ui,
    s: &mut State,
    acts: &mut Vec<Action>,
) {
    ui.heading(crate::i18n::text(
        "repo-workflows-review-label-fix-survival-radar-6fc26d",
    ));
    ui.label(crate::i18n::text(
        "repo-workflows-review-label-patch-identity-and-tested-behavior-are-indepen-012e83",
    ));
    ui.horizontal(|ui| {
        for (label, act) in [
            (
                crate::i18n::text("repo-workflows-review-label-load-pinned-fixes-83d54a"),
                Act::Load,
            ),
            (
                crate::i18n::text("repo-workflows-review-label-refresh-branch-observations-36659c"),
                Act::Check,
            ),
        ] {
            if ui.add_enabled(!s.busy, egui::Button::new(label)).clicked() {
                acts.push(Action::ReviewTools(ToolsAct::Radar(act)));
            }
        }
        if s.busy
            && ui
                .button(crate::i18n::text(
                    "repo-workflows-review-label-stop-regression-checks-72f2f9",
                ))
                .clicked()
        {
            acts.push(Action::ReviewTools(ToolsAct::Radar(Act::Stop)));
        }
    });
    for (i, pin) in s.pins.iter().enumerate() {
        ui.horizontal_wrapped(|ui| {
            ui.label(format!(
                "{} · {} · {}",
                pin.name,
                crate::repo::short(&pin.fix),
                pin.command
            ));
            if ui
                .add_enabled(
                    !s.busy && s.form.trusted,
                    egui::Button::new(crate::i18n::text(
                        "repo-workflows-review-label-test-target-behaviors-a08ec4",
                    )),
                )
                .clicked()
            {
                acts.push(Action::ReviewTools(ToolsAct::Radar(Act::Test {
                    index: i,
                    trusted: s.form.trusted,
                })));
            }
            if ui
                .small_button(crate::i18n::text(
                    "repo-workflows-review-label-unpin-ee3c71",
                ))
                .clicked()
            {
                acts.push(Action::ReviewTools(ToolsAct::Radar(Act::Remove(i))));
            }
        });
    }
    if let Some(rows) = &s.rows {
        match rows {
            Err(e) => {
                ui.label(crate::repo::messages::display_status(e));
            }
            Ok(rows) => {
                for row in rows {
                    ui.group(|ui| {
                        ui.strong(crate::i18n::message("repo-workflows-review-message-name-target-at-commit-0a888f", &[("name", (row.name.as_str()).into()), ("target", (row.target.as_str()).into()), ("commit", (crate::repo::short(&row.tip)).into())]));
                        match &row.patch {
                            Patch::Same(commit) => {
                                ui.label(crate::i18n::message("repo-workflows-review-message-same-patch-found-commit-b7c6ec", &[("commit", (commit.as_str()).into())]));
                            }
                            Patch::Adapted(commit) => {
                                ui.label(crate::i18n::message("repo-workflows-review-message-explicit-adaptation-linked-commit-7b568f", &[("commit", (commit.as_str()).into())]));
                                if ui.button(crate::i18n::text("repo-workflows-review-label-compare-adaptation-with-original-61cb5a")).clicked() {
                                    acts.push(adapted_fix_comparison(&row.fix, commit));
                                }
                            }
                            Patch::Reverted(commit) => {
                                ui.colored_label(
                                    egui::Color32::YELLOW,
                                    crate::i18n::message("repo-workflows-review-message-amber-later-reverse-patch-at-commit-fa4964", &[("commit", (commit.as_str()).into())]),
                                );
                            }
                            Patch::NotChecked => {
                                ui.label(crate::i18n::text("repo-workflows-review-label-patch-not-checked-no-match-in-inspected-histor-5787a6"));
                            }
                        }
                        ui.weak(if row.complete { crate::i18n::message("repo-workflows-review-message-count-one-one-nonmerge-commit-inspected-other-615d5c", &[("count", (row.coverage).into())]) } else { crate::i18n::message("repo-workflows-review-message-count-one-one-nonmerge-commit-inspected-other-85a6b5", &[("count", (row.coverage).into())]) });
                        if let Some(test) = &row.test {
                            contracts::show_observation(ui, test);
                        } else {
                            ui.label(crate::i18n::text("repo-workflows-review-label-regression-behavior-not-checked-for-this-tree-efab0a"));
                        }
                    });
                }
            }
        }
    }
    ui.separator();
    ui.add(
        egui::TextEdit::singleline(&mut s.form.name).hint_text(crate::i18n::text(
            "repo-workflows-review-label-fix-name-6f2e2c",
        )),
    );
    ui.add(
        egui::TextEdit::singleline(&mut s.form.fix).hint_text(crate::i18n::text(
            "repo-workflows-review-label-fix-commit-c192f6",
        )),
    );
    ui.label(crate::i18n::text(
        "repo-workflows-review-label-release-refs-optionally-release-adapted-commit-116d19",
    ));
    ui.text_edit_multiline(&mut s.form.targets);
    ui.add(
        egui::TextEdit::singleline(&mut s.form.command).hint_text(crate::i18n::text(
            "repo-workflows-review-label-fix-specific-regression-command-c0b3bc",
        )),
    );
    ui.checkbox(
        &mut s.form.trusted,
        crate::i18n::text(
            "repo-workflows-review-label-i-trust-running-these-regression-commands-and-3f8179",
        ),
    );
    if ui
        .add_enabled(
            !s.busy,
            egui::Button::new(crate::i18n::text(
                "repo-workflows-review-label-pin-this-fix-d28695",
            )),
        )
        .clicked()
    {
        acts.push(Action::ReviewTools(ToolsAct::Radar(Act::Pin(
            s.form.clone(),
        ))));
    }
}
