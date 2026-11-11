//! Presentation responsibilities for storyboard.
use super::*;

pub(super) fn display_phase(phase: &str) -> &str {
    match phase {
        "Old failure" => crate::i18n::text("repo-workflows-review-label-old-failure-af0d68"),
        "Invariant" => crate::i18n::text("repo-workflows-review-label-invariant-59d137"),
        "Fix" => crate::i18n::text("repo-workflows-review-label-fix-21f159"),
        "Result" => crate::i18n::text("repo-workflows-review-label-result-6e7d50"),
        _ => phase,
    }
}
pub(super) fn display_verdict(verdict: &str) -> &str {
    match verdict {
        "good" => crate::i18n::text("repo-workflows-review-label-good-770e60"),
        "bad" => crate::i18n::text("repo-workflows-review-label-bad-2f05d4"),
        _ => verdict,
    }
}
pub(super) fn display_test_command(command: &str) -> String {
    const CONFIGURED: &str =
        "Configured when pinned (execution command not recorded by test lane): ";
    match command.strip_prefix(CONFIGURED) {
        Some(command) => crate::i18n::message(
            "repo-workflows-review-raw-configured-when-pinned-execution-command-not-r-ddb1ee",
            &[("command", command.into())],
        ),
        None => command.to_owned(),
    }
}
pub(in crate::repo) fn show(
    ui: &mut egui::Ui,
    state: &mut Storyboard,
    d: &PrDetails,
    f: &PrFiles,
    remotes: &[git::Remote],
    acts: &mut Vec<Action>,
) {
    egui::CollapsingHeader::new(crate::i18n::text("repo-workflows-review-label-review-storyboard-f917d1")).id_salt("repo-workflows-review-label-review-storyboard-f917d1").show(ui, |ui| {
        if ui.button(crate::i18n::text("repo-workflows-review-label-load-storyboard-df2f79")).clicked() {
            acts.push(Action::Story(StoryAct::Load));
        }
        egui::ComboBox::from_id_salt("story-sharing-remote").selected_text(if state.remote.is_empty(){crate::i18n::text("repo-workflows-review-label-choose-storyboard-remote-bd2c8d")}else{&state.remote}).show_ui(ui,|ui|{
            for remote in remotes {ui.selectable_value(&mut state.remote,remote.name.clone(),&remote.name);}
        });
        ui.weak(crate::i18n::text("repo-workflows-review-label-sharing-publishes-the-cards-and-their-pinned-s-4f1959"));
        if ui.button(crate::i18n::text("repo-workflows-review-label-fetch-shared-author-storyboard-ec4246")).clicked(){acts.push(Action::Story(StoryAct::Fetch{remote:state.remote.clone(),head:f.to.clone()}));}
        if state.imported {ui.label(crate::i18n::text("repo-workflows-review-label-showing-imported-storyboard-load-the-local-sto-cc81d0"));}
        let own = d
            .extras
            .viewer
            .as_deref()
            .is_some_and(|v| v.eq_ignore_ascii_case(&d.author));
        if own && ui.button(crate::i18n::text("repo-workflows-review-label-publish-my-local-storyboard-3ee6c4")).clicked(){acts.push(Action::Story(StoryAct::Publish{remote:state.remote.clone(),head:f.to.clone()}));}
        let own=own && !state.imported;
        if let Some(rows) = &state.rows {
            match rows {
                Err(e) => {
                    ui.label(crate::repo::messages::display_status(e));
                }
                Ok(rows) => {
                    for (i, row) in rows.iter().enumerate() {
                        ui.group(|ui| {
                            ui.strong(format!("{}. {}", i + 1, display_phase(&row.card.phase)));
                            ui.label(&row.card.text);
                            ui.weak(crate::i18n::message("repo-workflows-review-message-pinned-base-head-96c6e6", &[("base", (crate::repo::short(&row.card.base)).into()), ("head", (crate::repo::short(&row.card.head)).into())]));
                            if let Some(stale) = &row.stale {
                                ui.colored_label(egui::Color32::YELLOW, crate::i18n::message("repo-workflows-review-message-stale-detail-264fc0", &[("detail", (crate::repo::messages::display_status(stale)).into())]));
                            }
                            ui.horizontal_wrapped(|ui| {
                                if let Some((commit, path, line)) = &row.target
                                    && ui
                                        .button(if *line > 0 {
                                            crate::i18n::text("repo-workflows-review-label-open-pinned-hunk-beb93d")
                                        } else {
                                            crate::i18n::text("repo-workflows-review-label-open-file-4190c0")
                                        })
                                        .clicked()
                                {
                                    open(acts, commit, path, *line);
                                }
                                match &row.card.anchor {
                                    Anchor::Code {
                                        commit, path, line, ..
                                    } => {
                                        if ui.button(crate::i18n::text("repo-workflows-review-label-open-original-revision-e14b40")).clicked() {
                                            open(acts, commit, path, *line);
                                        }
                                    }
                                    Anchor::Test {
                                        tree,
                                        command,
                                        verdict,
                                        ..
                                    } => {
                                        ui.monospace(crate::i18n::message("repo-workflows-review-message-observed-verdict-tree-tree-command-a654d0", &[("verdict", (display_verdict(verdict)).into()), ("tree", (tree.as_str()).into()), ("command", (display_test_command(command)).into())]));
                                    }
                                }
                                if own {
                                    for (up, label) in [(true, "↑"), (false, "↓")] {
                                        if ui.small_button(label).clicked() {
                                            acts.push(Action::Story(StoryAct::Move {
                                                index: i,
                                                up,
                                                head: f.to.clone(),
                                            }));
                                        }
                                    }
                                    if ui.small_button(crate::i18n::text("repo-workflows-review_workflow-show-c3812fc4")).clicked() {
                                        acts.push(Action::Story(StoryAct::Remove {
                                            index: i,
                                            head: f.to.clone(),
                                        }));
                                    }
                                }
                            });
                        });
                    }
                }
            }
        }
        if own {
            let form = &mut state.form;
            egui::ComboBox::from_id_salt("story-phase")
                .selected_text([crate::i18n::text("repo-workflows-review-label-old-failure-af0d68"), crate::i18n::text("repo-workflows-review-label-invariant-59d137"), crate::i18n::text("repo-workflows-review-label-fix-21f159"), crate::i18n::text("repo-workflows-review-label-result-6e7d50")][form.phase])
                .show_ui(ui, |ui| {
                    for (i, name) in [crate::i18n::text("repo-workflows-review-label-old-failure-af0d68"), crate::i18n::text("repo-workflows-review-label-invariant-59d137"), crate::i18n::text("repo-workflows-review-label-fix-21f159"), crate::i18n::text("repo-workflows-review-label-result-6e7d50")]
                        .iter()
                        .enumerate()
                    {
                        ui.selectable_value(&mut form.phase, i, *name);
                    }
                });
            ui.add(egui::TextEdit::multiline(&mut form.text).hint_text(crate::i18n::text("repo-workflows-review-label-explain-this-step-7f09ed")));
            ui.checkbox(&mut form.test, crate::i18n::text("repo-workflows-review-label-pin-a-test-lane-result-at-this-revision-6f1f6f"));
            ui.add(
                egui::TextEdit::singleline(&mut form.commit)
                    .hint_text(crate::i18n::text("repo-workflows-review-label-commit-empty-current-pr-head-6e6c30")),
            );
            if !form.test {
                ui.add(
                    egui::TextEdit::singleline(&mut form.path)
                        .hint_text(crate::i18n::text("repo-workflows-review-label-repository-relative-path-3f3f0a")),
                );
                ui.add(
                    egui::TextEdit::singleline(&mut form.line)
                        .hint_text(crate::i18n::text("repo-workflows-review-label-line-empty-entire-file-afe710")),
                );
            }
            if ui.button(crate::i18n::text("repo-workflows-review-label-add-revision-pinned-card-634c2d")).clicked() {
                acts.push(Action::Story(StoryAct::Add {
                    form: form.clone(),
                    head: f.to.clone(),
                }));
            }
        }
        ui.weak(crate::i18n::text("repo-workflows-review-label-stored-in-this-clone-cards-never-execute-comma-4cb802"));
    });
}
pub(super) fn open(acts: &mut Vec<Action>, commit: &str, path: &str, line: u64) {
    if line > 0 {
        acts.push(Action::Story(StoryAct::OpenCode {
            commit: commit.into(),
            path: path.into(),
            line,
        }));
    } else {
        acts.push(Action::FileView {
            path: path.into(),
            rev: Some(commit.into()),
            tab: FileTab::File,
        });
    }
}
