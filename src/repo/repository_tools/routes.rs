use super::{Act, Data, State};
use crate::{git::repository_tools::routes::Route, repo::Action};
fn label(route: &Route, direction: &str) -> String {
    if route.remote.is_empty() {
        return crate::i18n::message(
            "repo-workflows-repository_tools-label-87dc28eb",
            &[("direction", (direction).into())],
        );
    }
    let counts = route.counts.map_or(
        crate::i18n::text("repo-workflows-repository_tools-label-2d5fbffb").into(),
        |(a, b)| {
            crate::i18n::message(
                "repo-workflows-repository_tools-label-f25a8952",
                &[("a", (a).into()), ("b", (b).into())],
            )
        },
    );
    crate::i18n::message(
        "repo-workflows-repository_tools-label-137ebba9",
        &[
            ("direction", (direction).into()),
            ("route-remote-1", (route.remote).clone().into()),
            ("route-branch-2", (route.branch).clone().into()),
            ("counts", (counts).clone().into()),
        ],
    )
}
fn picker(
    ui: &mut egui::Ui,
    id: &str,
    remotes: &[String],
    choices: &[(String, String)],
    remote: &mut String,
    branch: &mut String,
) {
    egui::ComboBox::from_id_salt((id, "remote"))
        .selected_text(remote.as_str())
        .show_ui(ui, |ui| {
            for name in remotes {
                ui.selectable_value(remote, name.clone(), name);
            }
        });
    ui.text_edit_singleline(branch);
    egui::ComboBox::from_id_salt((id, "branch"))
        .selected_text(crate::i18n::text(
            "repo-workflows-repository_tools-picker-f523255b",
        ))
        .show_ui(ui, |ui| {
            for (_, name) in choices.iter().filter(|(r, _)| r == remote) {
                ui.selectable_value(branch, name.clone(), name);
            }
        });
}
pub(super) fn show(ui: &mut egui::Ui, state: &mut State, acts: &mut Vec<Action>) {
    ui.heading(crate::i18n::text(
        "repo-workflows-repository_tools-show-5dc87aef",
    ));
    ui.label(crate::i18n::text(
        "repo-workflows-repository_tools-show-5dcd35a3",
    ));
    let Some(result) = &mut state.result else {
        ui.add(crate::spinner::Spinner::new());
        return;
    };
    let data = match result {
        Ok(Data::Routes(v)) => v,
        Ok(_) => return,
        Err(e) => {
            ui.label(e.as_str());
            return;
        }
    };
    ui.checkbox(
        &mut state.acknowledged,
        crate::i18n::text("repo-workflows-repository_tools-show-b1249b5a"),
    );
    egui::ScrollArea::vertical()
        .id_salt("branch_routes")
        .max_height(560.0)
        .show(ui, |ui| {
            let mut save = None;
            for b in &mut data.branches {
                ui.push_id(&b.name, |ui| {
                    ui.strong(&b.name);
                    ui.label(label(
                        &b.pull,
                        crate::i18n::text("repo-workflows-repository_tools-show-089eb3ef"),
                    ));
                    ui.label(label(
                        &b.push,
                        crate::i18n::text("repo-workflows-repository_tools-show-a503aa66"),
                    ));
                    egui::CollapsingHeader::new(crate::i18n::text(
                        "repo-workflows-repository_tools-show-8fe4dcef",
                    ))
                    .id_salt("branch-route-edit")
                    .default_open(data.selected == b.name)
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(crate::i18n::text(
                                "repo-workflows-repository_tools-show-b87ce680",
                            ));
                            picker(
                                ui,
                                "pull",
                                &data.remotes,
                                &data.choices,
                                &mut b.pull_remote,
                                &mut b.pull_branch,
                            );
                        });
                        ui.horizontal(|ui| {
                            ui.label(crate::i18n::text(
                                "repo-workflows-repository_tools-show-9fbeaee4",
                            ));
                            picker(
                                ui,
                                "push",
                                &data.remotes,
                                &data.choices,
                                &mut b.push_remote,
                                &mut b.push_branch,
                            );
                        });
                        if ui
                            .add_enabled(
                                state.acknowledged,
                                egui::Button::new(crate::i18n::text(
                                    "repo-workflows-repository_tools-show-c2c3785a",
                                )),
                            )
                            .clicked()
                        {
                            save = Some(b.name.clone());
                        }
                    });
                    ui.separator();
                });
            }
            if let Some(name) = save {
                acts.push(Action::RepositoryTools(Act::SaveRoutes(
                    Box::new(data.clone()),
                    name,
                )));
            }
        });
}

pub(super) fn force(ui: &mut egui::Ui, state: &mut State, acts: &mut Vec<Action>) {
    ui.heading(crate::i18n::text(
        "repo-workflows-repository_tools-force-01549c1f",
    ));
    let Some(result) = &state.result else {
        ui.add(crate::spinner::Spinner::new());
        return;
    };
    let p = match result {
        Ok(Data::Force(v)) => v,
        Ok(_) => return,
        Err(e) => {
            ui.label(e);
            return;
        }
    };
    ui.label(crate::i18n::message(
        "repo-workflows-repository_tools-force-884b8ad2",
        &[
            ("p-destination-1", (p.destination).clone().into()),
            ("p-remote-2", (p.remote).clone().into()),
            (
                "crate-git-redact-p-url-none-3",
                (crate::git::redact(&p.url, None)).clone().into(),
            ),
            ("p-branch-4", (p.branch).clone().into()),
            ("p-commit-5", (p.commit).clone().into()),
        ],
    ));
    ui.label(crate::i18n::message(
        "repo-workflows-repository_tools-force-15d2a4c4",
        &[("p-remote-commit-1", (p.remote_commit).clone().into())],
    ));
    ui.checkbox(
        &mut state.acknowledged,
        crate::i18n::text("repo-workflows-repository_tools-force-1d53000c"),
    );
    if ui
        .add_enabled(
            state.acknowledged,
            egui::Button::new(crate::i18n::text(
                "repo-workflows-repository_tools-force-b7f756d6",
            )),
        )
        .clicked()
    {
        acts.push(Action::RepositoryTools(Act::ForceConfirmed(Box::new(
            p.clone(),
        ))));
    }
}
