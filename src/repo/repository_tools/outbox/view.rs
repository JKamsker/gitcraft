//! Queued publication review, explicit send consent, and localized summaries.

use super::*;

pub(in crate::repo::repository_tools) fn show(
    ui: &mut egui::Ui,
    state: &mut State,
    acts: &mut Vec<Action>,
) {
    ui.heading(crate::i18n::text(
        "repo-workflows-repository_tools-show-52b07a38",
    ));
    ui.label(crate::i18n::text(
        "repo-workflows-repository_tools-show-96a8a275",
    ));
    ui.label(crate::i18n::text(
        "repo-workflows-repository_tools-show-4d07d136",
    ));
    let Some(result) = &mut state.result else {
        ui.add(crate::spinner::Spinner::new());
        return;
    };
    let data = match result {
        Ok(data) => data,
        Err(e) => {
            ui.colored_label(crate::theme::Tokens::get(ui.ctx()).danger, e.as_str());
            if ui
                .button(crate::i18n::text(
                    "repo-workflows-repository_tools-show-a2869e42",
                ))
                .clicked()
            {
                acts.push(Action::RepositoryTools(Act::Outbox));
            }
            return;
        }
    };
    match data {
        Data::Outbox(view) => {
            ui.label(crate::i18n::message(
                "repo-workflows-repository_tools-show-1bd5bcfa",
                &[("view-branch-1", (view.branch).clone().into())],
            ));
            ui.checkbox(
                &mut state.compatible,
                crate::i18n::text("repo-workflows-repository_tools-show-202c8fbc"),
            );
            if ui
                .button(crate::i18n::text(
                    "repo-workflows-repository_tools-show-4c8748ea",
                ))
                .clicked()
            {
                acts.push(Action::RepositoryTools(Act::QueuePush(
                    Box::new(view.clone()),
                    state.compatible,
                )));
            }
            egui::ScrollArea::vertical()
                .max_height(430.0)
                .show(ui, |ui| {
                    for e in &view.queue.entries {
                        ui.separator();
                        ui.label(format!("{} → {} / {}", e.source, e.remote, e.destination));
                        ui.monospace(&e.commit);
                        ui.label(
                            e.consent.as_ref().map_or(
                                crate::i18n::text("repo-workflows-repository_tools-show-8cc484e4")
                                    .into(),
                                |consent| {
                                    crate::i18n::message(
                                        "repo-workflows-repository_tools-show-a2bc21d3",
                                        &[("consent-profile-1", (consent.profile).into())],
                                    )
                                },
                            ),
                        );
                        ui.label(outbox_entry_summary(
                            &e.status,
                            e.endpoints.len(),
                            e.autosend,
                        ));
                        if !e.receipt.is_empty() {
                            egui::CollapsingHeader::new(crate::i18n::text(
                                "repo-workflows-repository_tools-show-dad5a969",
                            ))
                            .id_salt(&e.id)
                            .show(ui, |ui| {
                                ui.label(&e.receipt);
                            });
                        }
                        if matches!(e.status, logic::Status::Paused(_) | logic::Status::Sending)
                            && ui
                                .button(crate::i18n::text(
                                    "repo-workflows-repository_tools-show-fccdc9ec",
                                ))
                                .clicked()
                        {
                            acts.push(Action::RepositoryTools(Act::ReviewOutbox(e.id.clone())));
                        }
                        ui.horizontal(|ui| {
                            if ui
                                .add_enabled(
                                    e.status == logic::Status::Ready,
                                    egui::Button::new(crate::i18n::text(
                                        "repo-workflows-repository_tools-show-222bd382",
                                    )),
                                )
                                .clicked()
                            {
                                acts.push(Action::RepositoryTools(Act::PrepareOutbox(
                                    e.id.clone(),
                                )));
                            }
                            if ui
                                .add_enabled(
                                    e.status != logic::Status::Sending,
                                    egui::Button::new(crate::i18n::text(
                                        "repo-workflows-repository_tools-show-d653a1c5",
                                    )),
                                )
                                .clicked()
                            {
                                acts.push(Action::RepositoryTools(Act::RemoveOutbox(
                                    Box::new(view.clone()),
                                    e.id.clone(),
                                )));
                            }
                        });
                    }
                });
        }
        Data::PreparedOutbox(p) => {
            if p.review {
                ui.colored_label(
                    crate::theme::Tokens::get(ui.ctx()).warning,
                    crate::i18n::text("repo-workflows-repository_tools-show-b9cc89c6"),
                );
            }
            ui.label(crate::i18n::message(
                "repo-workflows-repository_tools-show-a17b0445",
                &[
                    ("p-plan-head-1", (p.plan.head).clone().into()),
                    ("p-plan-remote-2", (p.plan.remote).clone().into()),
                    ("p-plan-destination-3", (p.plan.destination).clone().into()),
                ],
            ));
            ui.label(crate::i18n::message(
                "repo-workflows-repository_tools-show-99256c25",
                &[("p-consent-profile-1", (p.consent.profile).into())],
            ));
            if let Some((endpoint, account, username)) = &p.consent.principal {
                ui.label(crate::i18n::message(
                    "repo-workflows-repository_tools-show-8fb60b6e",
                    &[
                        (
                            "crate-git-redact-endpoint-none-1",
                            (crate::git::redact(endpoint, None)).clone().into(),
                        ),
                        ("account-2", (account).clone().into()),
                        ("username-3", (username).clone().into()),
                    ],
                ));
            }
            for target in &p.plan.targets {
                ui.label(target);
            }
            ui.label(crate::i18n::message(
                "repo-workflows-repository_tools-show-9b1cea76",
                &[("p-plan-commits-len-1", (p.plan.commits.len()).into())],
            ));
            egui::ScrollArea::vertical()
                .max_height(300.0)
                .show(ui, |ui| {
                    if p.plan.findings.is_empty() {
                        ui.label(crate::i18n::text(
                            "repo-workflows-repository_tools-show-80ef5663",
                        ));
                    }
                    for f in &p.plan.findings {
                        ui.label(format!(
                            "{:?}: {} {} {}",
                            f.kind, f.commit, f.path, f.detail
                        ));
                    }
                });
            ui.checkbox(
                &mut state.acknowledged,
                crate::i18n::text("repo-workflows-repository_tools-show-2e1d0813"),
            );
            if ui
                .add_enabled(
                    state.acknowledged,
                    egui::Button::new(crate::i18n::text(
                        "repo-workflows-repository_tools-show-78f32476",
                    )),
                )
                .clicked()
            {
                acts.push(Action::RepositoryTools(Act::SendOutbox(p.clone())));
            }
        }
        _ => {}
    }
}

pub(super) fn outbox_entry_summary(
    status: &logic::Status,
    endpoints: usize,
    automatic: bool,
) -> String {
    crate::i18n::message(
        "repo-workflows-outbox-entry-summary",
        &[
            (
                "status",
                (match status {
                    logic::Status::Ready => "ready",
                    logic::Status::Sending => "sending",
                    logic::Status::Sent => "sent",
                    logic::Status::Paused(_) => "paused",
                })
                .into(),
            ),
            (
                "detail",
                (match status {
                    logic::Status::Paused(detail) => {
                        crate::repo::activity::display_workflow_error(detail)
                    }
                    _ => String::new(),
                })
                .into(),
            ),
            ("endpoints", (endpoints).into()),
            ("automatic", (if automatic { "yes" } else { "no" }).into()),
        ],
    )
}
