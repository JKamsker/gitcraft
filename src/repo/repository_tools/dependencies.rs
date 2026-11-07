use super::{Act, Data, State};
use crate::{git, repo::Action};
pub(super) fn show(ui: &mut egui::Ui, state: &mut State, acts: &mut Vec<Action>) {
    ui.heading("Publish submodule dependencies before their parent");
    ui.label("Check fetches heads/tags into temporary app-owned refs on each submodule. It checks the named fetch endpoint, including gitlinks in outgoing history. Nested dependencies publish first. These pushes are not atomic; failures stop their parent.");
    let Some(result) = &mut state.result else {
        ui.spinner();
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
    ui.label(format!(
        "Parent {} at {} → {}/{} ({})",
        p.branch,
        p.commit,
        p.parent_remote,
        p.parent_branch,
        git::redact(&p.parent_url, None)
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
                        ui.label("Check/publish remote:");
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
                        if ui.button("Open submodule").clicked() {
                            acts.push(Action::OpenTab(n.repo.clone()));
                        }
                    });
                    for oid in &n.required {
                        ui.monospace(format!("Required: {oid}"));
                    }
                    if !n.fetch_url.is_empty() {
                        ui.label(format!(
                            "Checked fetch URL: {} · {}",
                            git::redact(&n.fetch_url, None),
                            if n.published {
                                "already published"
                            } else {
                                "publication required"
                            }
                        ));
                        ui.label(format!(
                            "Exact source: {} · push URL: {}",
                            n.source,
                            git::redact(&n.push_url, None)
                        ));
                    }
                    ui.horizontal(|ui| {
                        ui.label("Destination branch:");
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
                        ui.label(format!("Parent published: {}", git::redact(s, None)));
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
        .button("Check every dependency and outgoing gitlink")
        .clicked()
    {
        acts.push(Action::RepositoryTools(Act::CheckDependencies(Box::new(
            p.clone(),
        ))));
    }
    ui.checkbox(
        &mut state.acknowledged,
        "I reviewed each named remote, exact commit and target branch",
    );
    if ui
        .add_enabled(
            p.checked && p.dependencies.iter().all(|n| n.problem.is_none()) && state.acknowledged,
            egui::Button::new("Push dependencies, then parent"),
        )
        .clicked()
    {
        acts.push(Action::RepositoryTools(Act::PublishDependencies(Box::new(
            p.clone(),
        ))));
    }
}
