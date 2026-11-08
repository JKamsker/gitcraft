use super::{Act, Data, State};
use crate::{git, repo::Action};
pub(super) fn show(ui: &mut egui::Ui, state: &mut State, acts: &mut Vec<Action>) {
    ui.heading(crate::i18n::text(
        "repo-workflows-repository_tools-show-107f87fc",
    ));
    ui.label(crate::i18n::text(
        "repo-workflows-repository_tools-show-19cca4e2",
    ));
    let Some(result) = &mut state.result else {
        ui.add(crate::spinner::Spinner::new());
        return;
    };
    let p = match result {
        Ok(Data::Dependencies(p)) => p,
        Ok(_) => return,
        Err(e) => {
            ui.label(e.as_str());
            return;
        }
    };
    ui.label(crate::i18n::message(
        "repo-workflows-repository_tools-show-b85186ac",
        &[
            ("p-branch-1", (p.branch).clone().into()),
            ("p-commit-2", (p.commit).clone().into()),
            ("p-parent-remote-3", (p.parent_remote).clone().into()),
            ("p-parent-branch-4", (p.parent_branch).clone().into()),
            (
                "git-redact-p-parent-url-none-5",
                (git::redact(&p.parent_url, None)).clone().into(),
            ),
        ],
    ));
    egui::ScrollArea::vertical()
        .id_salt("dependency_publication")
        .max_height(470.0)
        .show(ui, |ui| {
            for n in &mut p.dependencies {
                ui.push_id(&n.path, |ui| {
                    ui.separator();
                    ui.strong(&n.path);
                    ui.horizontal(|ui| {
                        ui.label(crate::i18n::text(
                            "repo-workflows-repository_tools-show-fcb0aec1",
                        ));
                        egui::ComboBox::from_id_salt("dependency_remote")
                            .selected_text(n.remote.as_str())
                            .show_ui(ui, |ui| {
                                for remote in &n.remotes {
                                    if ui
                                        .selectable_value(&mut n.remote, remote.clone(), remote)
                                        .changed()
                                    {
                                        p.checked = false;
                                    }
                                }
                            });
                        if ui
                            .button(crate::i18n::text(
                                "repo-workflows-repository_tools-show-1560b7d2",
                            ))
                            .clicked()
                        {
                            acts.push(Action::OpenTab(n.repo.clone()));
                        }
                    });
                    for oid in &n.required {
                        ui.monospace(crate::i18n::message(
                            "repo-workflows-repository_tools-show-31a04bc4",
                            &[("oid", (oid).clone().into())],
                        ));
                    }
                    if !n.fetch_url.is_empty() {
                        ui.label(crate::i18n::message(
                            "repo-workflows-repository_tools-show-3f2768b0",
                            &[
                                (
                                    "git-redact-n-fetch-url-none-1",
                                    (git::redact(&n.fetch_url, None)).clone().into(),
                                ),
                                (
                                    "if-n-published-already-published-2",
                                    (if n.published {
                                        crate::i18n::text(
                                            "repo-workflows-repository_tools-show-1fad1933",
                                        )
                                    } else {
                                        crate::i18n::text(
                                            "repo-workflows-repository_tools-show-88db1c04",
                                        )
                                    })
                                    .into(),
                                ),
                            ],
                        ));
                        ui.label(crate::i18n::message(
                            "repo-workflows-repository_tools-show-49346e37",
                            &[
                                ("n-source-1", (n.source).clone().into()),
                                (
                                    "git-redact-n-push-url-none-2",
                                    (git::redact(&n.push_url, None)).clone().into(),
                                ),
                            ],
                        ));
                    }
                    ui.horizontal(|ui| {
                        ui.label(crate::i18n::text(
                            "repo-workflows-repository_tools-show-798b0df6",
                        ));
                        if ui.text_edit_singleline(&mut n.target).changed() {
                            p.checked = false;
                        }
                    });
                    if let Some(problem) = &n.problem {
                        ui.colored_label(crate::theme::Tokens::get(ui.ctx()).danger, problem);
                    }
                    if let Some(result) = &n.result {
                        match result {
                            Ok(s) => {
                                ui.label(s);
                            }
                            Err(e) => {
                                ui.colored_label(
                                    crate::theme::Tokens::get(ui.ctx()).danger,
                                    git::redact(e, None),
                                );
                            }
                        }
                    }
                });
            }
            if let Some(result) = &p.parent_result {
                ui.separator();
                match result {
                    Ok(s) => {
                        ui.label(crate::i18n::message(
                            "repo-workflows-repository_tools-show-ccbcb47e",
                            &[("git-redact-s-none-1", (git::redact(s, None)).clone().into())],
                        ));
                    }
                    Err(e) => {
                        ui.colored_label(
                            crate::theme::Tokens::get(ui.ctx()).danger,
                            git::redact(e, None),
                        );
                    }
                }
            }
        });
    if ui
        .button(crate::i18n::text(
            "repo-workflows-repository_tools-show-e9e7ecb3",
        ))
        .clicked()
    {
        acts.push(Action::RepositoryTools(Act::CheckDependencies(Box::new(
            p.clone(),
        ))));
    }
    ui.checkbox(
        &mut state.acknowledged,
        crate::i18n::text("repo-workflows-repository_tools-show-8f3b56b3"),
    );
    if ui
        .add_enabled(
            p.checked && p.dependencies.iter().all(|n| n.problem.is_none()) && state.acknowledged,
            egui::Button::new(crate::i18n::text(
                "repo-workflows-repository_tools-show-118694a9",
            )),
        )
        .clicked()
    {
        acts.push(Action::RepositoryTools(Act::PublishDependencies(Box::new(
            p.clone(),
        ))));
    }
}
