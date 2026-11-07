use super::{Act, Data, State};
use crate::{git, repo::Action};
pub(super) fn show(ui: &mut egui::Ui, state: &mut State, acts: &mut Vec<Action>) {
    ui.heading("Publish to several hosts");
    ui.label("These pushes are not atomic. A successful host stays published if another host fails. Each destination receives the exact commit shown; retry sends only failed destinations.");
    let Some(result) = &mut state.result else {
        ui.spinner();
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
        ui.label("Group:");
        ui.text_edit_singleline(&mut m.group);
        egui::ComboBox::from_id_salt("publication_groups")
            .selected_text("Saved groups")
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
        if ui.button("Save named group").clicked() {
            acts.push(Action::RepositoryTools(Act::SaveGroup(Box::new(m.clone()))));
        }
        if ui.button("Prepare exact commit and destinations").clicked() {
            acts.push(Action::RepositoryTools(Act::PreparePublication(Box::new(
                m.clone(),
            ))));
        }
    });
    if let Some(p) = &m.plan {
        ui.separator();
        ui.label(format!(
            "Group {} · {} → refs/heads/{}",
            p.group, p.commit, p.branch
        ));
        egui::ScrollArea::vertical()
            .id_salt("publication_results")
            .max_height(290.0)
            .show(ui, |ui| {
                for d in &p.destinations {
                    ui.label(format!("{} · {}", d.remote, git::redact(&d.url, None)));
                    match &d.result {
                        None => {
                            ui.label("Ready");
                        }
                        Some(Ok(output)) => {
                            ui.label(format!("Published: {}", git::redact(output, None)));
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
            "I reviewed every destination and accept partial publication",
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
                    "Publish group"
                } else {
                    "Retry failed destinations only"
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
