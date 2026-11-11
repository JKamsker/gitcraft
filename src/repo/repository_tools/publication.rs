use super::{Act, Data, State};
use crate::{git, repo::Action};
pub(super) fn show(ui: &mut egui::Ui, state: &mut State, acts: &mut Vec<Action>) {
    ui.heading(crate::i18n::text(
        "repo-workflows-repository_tools-show-057248ff",
    ));
    ui.label(crate::i18n::text(
        "repo-workflows-repository_tools-show-d15b2630",
    ));
    let Some(result) = &mut state.result else {
        ui.add(crate::spinner::Spinner::new());
        return;
    };
    let m = match result {
        Ok(Data::Publication(v)) => v,
        Ok(_) => return,
        Err(e) => {
            ui.label(e.as_str());
            return;
        }
    };
    ui.horizontal(|ui| {
        ui.label(crate::i18n::text(
            "repo-workflows-repository_tools-show-aff83496",
        ));
        ui.text_edit_singleline(&mut m.group);
        egui::ComboBox::from_id_salt("publication_groups")
            .selected_text(crate::i18n::text(
                "repo-workflows-repository_tools-show-ae033504",
            ))
            .show_ui(ui, |ui| {
                for name in m.groups.keys() {
                    if ui.selectable_label(m.group == *name, name).clicked() {
                        acts.push(Action::RepositoryTools(Act::Publication(name.clone())));
                    }
                }
            });
    });
    for remote in &m.remotes {
        let mut selected = m.selected.contains(remote);
        if ui.checkbox(&mut selected, remote).changed() {
            if selected {
                m.selected.push(remote.clone());
            } else {
                m.selected.retain(|r| r != remote);
            }
            m.plan = None;
        }
    }
    ui.horizontal(|ui| {
        if ui
            .button(crate::i18n::text(
                "repo-workflows-repository_tools-show-7790f11b",
            ))
            .clicked()
        {
            acts.push(Action::RepositoryTools(Act::SaveGroup(Box::new(m.clone()))));
        }
        if ui
            .button(crate::i18n::text(
                "repo-workflows-repository_tools-show-f66ce8b9",
            ))
            .clicked()
        {
            acts.push(Action::RepositoryTools(Act::PreparePublication(Box::new(
                m.clone(),
            ))));
        }
    });
    if let Some(p) = &m.plan {
        ui.separator();
        ui.label(crate::i18n::message(
            "repo-workflows-repository_tools-show-5fb1b07d",
            &[
                ("p-group-1", (p.group).clone().into()),
                ("p-commit-2", (p.commit).clone().into()),
                ("p-branch-3", (p.branch).clone().into()),
            ],
        ));
        egui::ScrollArea::vertical()
            .id_salt("publication_results")
            .max_height(290.0)
            .show(ui, |ui| {
                for d in &p.destinations {
                    ui.label(format!("{} · {}", d.remote, git::redact(&d.url, None)));
                    match &d.result {
                        None => {
                            ui.label(crate::i18n::text(
                                "repo-workflows-repository_tools-show-5fa7aac5",
                            ));
                        }
                        Some(Ok(output)) => {
                            ui.label(crate::i18n::message(
                                "repo-workflows-repository_tools-show-8472b5d8",
                                &[(
                                    "git-redact-output-none-1",
                                    (git::redact(output, None)).clone().into(),
                                )],
                            ));
                        }
                        Some(Err(e)) => {
                            ui.colored_label(
                                crate::theme::Tokens::get(ui.ctx()).danger,
                                git::redact(e, None),
                            );
                        }
                    }
                }
            });
        ui.checkbox(
            &mut state.acknowledged,
            crate::i18n::text("repo-workflows-repository_tools-show-8edc0123"),
        );
        let fresh = p.destinations.iter().all(|d| d.result.is_none());
        let failed = p
            .destinations
            .iter()
            .any(|d| matches!(d.result, Some(Err(_))));
        if ui
            .add_enabled(
                state.acknowledged && (fresh || failed),
                egui::Button::new(if fresh {
                    crate::i18n::text("repo-workflows-repository_tools-show-3cc9a2c6")
                } else {
                    crate::i18n::text("repo-workflows-repository_tools-show-2c00ff77")
                }),
            )
            .clicked()
        {
            acts.push(Action::RepositoryTools(Act::PublishGroup(
                Box::new(m.clone()),
                !fresh,
            )));
        }
    }
}
