//! Deferred replay review, conflict navigation, and candidate comparison actions.

use super::*;

pub(in crate::repo::repository_tools) fn show(
    ui: &mut egui::Ui,
    state: &mut State,
    acts: &mut Vec<Action>,
) {
    ui.heading(crate::i18n::text(
        "repo-workflows-repository_tools-show-7d86e238",
    ));
    ui.label(crate::i18n::text(
        "repo-workflows-repository_tools-show-c757c39b",
    ));
    if ui
        .button(crate::i18n::text(
            "repo-workflows-repository_tools-show-7c52fd3d",
        ))
        .clicked()
    {
        acts.push(Action::RepositoryTools(Act::Deferred));
    }
    let Some(result) = &state.result else {
        ui.add(crate::spinner::Spinner::new());
        return;
    };
    let data = match result {
        Ok(d) => d,
        Err(e) => {
            ui.colored_label(crate::theme::Tokens::get(ui.ctx()).danger, e);
            return;
        }
    };
    if let Data::DeferredStack(view, specs) = data {
        ui.label(crate::i18n::text(
            "repo-workflows-repository_tools-show-412ec52f",
        ));
        for p in specs {
            ui.monospace(crate::i18n::message(
                "repo-workflows-repository_tools-show-b2f82ce9",
                &[
                    ("p-branch-1", (p.branch).clone().into()),
                    ("p-expected-2", (p.expected).clone().into()),
                    ("p-old-base-3", (p.old_base).clone().into()),
                    ("p-onto-4", (p.onto).clone().into()),
                ],
            ));
        }
        if ui
            .button(crate::i18n::text(
                "repo-workflows-repository_tools-show-1919101c",
            ))
            .clicked()
        {
            acts.push(Action::RepositoryTools(Act::StartDeferred(
                Box::new(view.clone()),
                "Isolated restack".into(),
                specs.clone(),
            )));
        }
        return;
    }
    let Data::Deferred(view) = data else {
        return;
    };
    ui.horizontal(|ui| {
        ui.label(crate::i18n::text(
            "repo-workflows-repository_tools-show-4829ff52",
        ));
        egui::ComboBox::from_id_salt("deferred-source")
            .selected_text(if state.query.is_empty() {
                &view.branch
            } else {
                &state.query
            })
            .show_ui(ui, |ui| {
                for (name, _) in &view.branches {
                    ui.selectable_value(&mut state.query, name.clone(), name);
                }
            });
    });
    ui.horizontal(|ui| {
        ui.label(crate::i18n::text(
            "repo-workflows-repository_tools-show-80c567ae",
        ));
        ui.text_edit_singleline(&mut state.filter);
    });
    ui.horizontal(|ui| {
        ui.label(crate::i18n::text(
            "repo-workflows-repository_tools-show-0d37e057",
        ));
        ui.text_edit_singleline(&mut state.parent);
    });
    if ui
        .button(crate::i18n::text(
            "repo-workflows-repository_tools-show-0256e5a0",
        ))
        .clicked()
    {
        let name = if state.query.is_empty() {
            view.branch.clone()
        } else {
            state.query.clone()
        };
        if let Some((_, old)) = view.branches.iter().find(|(n, _)| *n == name) {
            acts.push(Action::RepositoryTools(Act::StartDeferred(
                Box::new(view.clone()),
                format!("Rebase {name}"),
                vec![logic::Spec {
                    branch: name,
                    expected: old.clone(),
                    old_base: state.filter.clone(),
                    onto: state.parent.clone(),
                    parent: String::new(),
                }],
            )));
        }
    }
    ui.checkbox(
        &mut state.acknowledged,
        crate::i18n::text("repo-workflows-repository_tools-show-3a7402e9"),
    );
    egui::ScrollArea::vertical()
        .max_height(430.0)
        .show(ui, |ui| {
            for job in &view.jobs.jobs {
                ui.separator();
                ui.strong(crate::i18n::message(
                    "repo-workflows-deferred-replay-progress",
                    &[
                        (
                            "title",
                            (crate::repo::messages::display_operation(&job.title)).into(),
                        ),
                        ("done", (job.cursor).into()),
                        ("total", (job.steps.len()).into()),
                        ("closed", (if job.cancelled { "yes" } else { "no" }).into()),
                    ],
                ));
                ui.label(crate::repo::messages::display_status(&job.notice));
                if job.busy {
                    ui.colored_label(
                        crate::theme::Tokens::get(ui.ctx()).warning,
                        crate::i18n::text("repo-workflows-repository_tools-show-aa785909"),
                    );
                }
                for s in &job.steps {
                    ui.monospace(format!(
                        "{}: {} → {}{}",
                        s.branch,
                        s.old,
                        s.result.as_deref().unwrap_or(crate::i18n::text(
                            "repo-workflows-repository_tools-show-6bdc8428"
                        )),
                        if s.accepted { " · accepted" } else { "" }
                    ));
                    ui.label(crate::i18n::message(
                        "repo-workflows-repository_tools-show-616296e8",
                        &[
                            ("s-branch-1", (s.branch).clone().into()),
                            ("s-old-base-2", (s.old_base).clone().into()),
                            ("s-onto-3", (s.onto).clone().into()),
                        ],
                    ));
                    if s.made && ui.link(s.worktree.display().to_string()).clicked() {
                        acts.push(Action::OpenWorktree(s.worktree.clone()));
                    }
                    for p in &s.conflicts {
                        ui.colored_label(
                            crate::theme::Tokens::get(ui.ctx()).danger,
                            crate::i18n::message(
                                "repo-workflows-repository_tools-show-8daa08a5",
                                &[("p", (p).clone().into())],
                            ),
                        );
                    }
                    if let Some(new) = &s.result
                        && ui
                            .button(crate::i18n::message(
                                "repo-workflows-repository_tools-show-19af9596",
                                &[("s-branch-1", (s.branch).clone().into())],
                            ))
                            .clicked()
                    {
                        acts.push(candidate_comparison(&s.old, new, &s.branch));
                    }
                }
                ui.horizontal_wrapped(|ui| {
                    if !job.cancelled
                        && job.cursor < job.steps.len()
                        && ui
                            .button(crate::i18n::text(
                                "repo-workflows-repository_tools-show-98a12458",
                            ))
                            .clicked()
                    {
                        acts.push(Action::RepositoryTools(Act::ContinueDeferred(
                            Box::new(view.clone()),
                            job.id.clone(),
                        )));
                    }
                    if !job.cancelled
                        && job.cursor == job.steps.len()
                        && ui
                            .add_enabled(
                                state.acknowledged && !job.busy,
                                egui::Button::new(crate::i18n::text(
                                    "repo-workflows-repository_tools-show-ba765b08",
                                )),
                            )
                            .clicked()
                    {
                        acts.push(Action::RepositoryTools(Act::AcceptDeferred(
                            Box::new(view.clone()),
                            job.id.clone(),
                            false,
                        )));
                    }
                    if job.steps.iter().any(|s| s.accepted)
                        && ui
                            .add_enabled(
                                state.acknowledged && !job.busy,
                                egui::Button::new(crate::i18n::text(
                                    "repo-workflows-repository_tools-show-440174a7",
                                )),
                            )
                            .clicked()
                    {
                        acts.push(Action::RepositoryTools(Act::AcceptDeferred(
                            Box::new(view.clone()),
                            job.id.clone(),
                            true,
                        )));
                    }
                    if ui
                        .button(crate::i18n::text(
                            "repo-workflows-repository_tools-show-5471de9c",
                        ))
                        .clicked()
                    {
                        acts.push(Action::RepositoryTools(Act::ReconcileDeferred(
                            Box::new(view.clone()),
                            job.id.clone(),
                        )));
                    }
                    if !job.cancelled
                        && ui
                            .button(crate::i18n::text(
                                "repo-workflows-repository_tools-show-33463eb4",
                            ))
                            .clicked()
                    {
                        acts.push(Action::RepositoryTools(Act::CloseDeferred(
                            Box::new(view.clone()),
                            job.id.clone(),
                        )));
                    }
                });
            }
        });
}

pub(super) fn candidate_comparison(original: &str, candidate: &str, branch: &str) -> Action {
    Action::Compare {
        base: original.to_owned(),
        base_label: format!("original {branch}"),
        target: Some(candidate.to_owned()),
        target_label: format!("candidate {branch}"),
        captions: [
            Some(crate::repo::state::CompareCaption::OriginalBranch {
                branch: branch.into(),
            }),
            Some(crate::repo::state::CompareCaption::CandidateBranch {
                branch: branch.into(),
            }),
        ],
        since_fork: false,
    }
}
