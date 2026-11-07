use super::{Act, Data, State};
use crate::{git::repository_tools::routes::Route, repo::Action};
fn label(route: &Route, direction: &str) -> String {
    if route.remote.is_empty() {
        return format!("{direction}: no route configured");
    }
    let counts = route
        .counts
        .map_or("counts unknown (fetch first)".into(), |(a, b)| {
            format!("{a} ahead · {b} behind")
        });
    format!("{direction} {}/{} · {counts}", route.remote, route.branch)
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
        .selected_text("Known branches")
        .show_ui(ui, |ui| {
            for (_, name) in choices.iter().filter(|(r, _)| r == remote) {
                ui.selectable_value(branch, name.clone(), name);
            }
        });
}
pub(super) fn show(ui: &mut egui::Ui, state: &mut State, acts: &mut Vec<Action>) {
    ui.heading("Separate pull and push routes");
    ui.label("Each count compares this local branch to the last fetched ref for that route. Missing or excluded tracking refs show unknown. Changing a push branch affects gitcraft's ordinary branch pushes; Git itself still uses its native push.default/refspec rules.");
    let Some(result) = &mut state.result else {
        ui.spinner();
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
        "I reviewed the pull and push destinations before saving",
    );
    egui::ScrollArea::vertical()
        .id_salt("branch_routes")
        .max_height(560.0)
        .show(ui, |ui| {
            let mut save = None;
            for b in &mut data.branches {
                ui.push_id(&b.name, |ui| {
                    ui.strong(&b.name);
                    ui.label(label(&b.pull, "Pull from"));
                    ui.label(label(&b.push, "Push to"));
                    egui::CollapsingHeader::new("Change routes…")
                        .default_open(data.selected == b.name)
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.label("Pull:");
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
                                ui.label("Push:");
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
                                    egui::Button::new("Save both routes"),
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
    ui.heading("Force push to the push route");
    let Some(result) = &state.result else {
        ui.spinner();
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
    ui.label(format!(
        "Replace {} on {} ({}) with {} at {}?",
        p.destination,
        p.remote,
        crate::git::redact(&p.url, None),
        p.branch,
        p.commit
    ));
    ui.label(format!("The destination is currently at {}. This can remove collaborators' commits. The exact URL and expected destination commit are pinned, and a lease rejects concurrent pushes.",p.remote_commit));
    ui.checkbox(
        &mut state.acknowledged,
        "I reviewed the destination and accept replacing its history",
    );
    if ui
        .add_enabled(
            state.acknowledged,
            egui::Button::new("Force push with explicit lease"),
        )
        .clicked()
    {
        acts.push(Action::RepositoryTools(Act::ForceConfirmed(Box::new(
            p.clone(),
        ))));
    }
}
