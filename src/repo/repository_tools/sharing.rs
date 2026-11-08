use super::{Act, Action, Data, State};
pub(super) fn show(ui: &mut egui::Ui, state: &mut State, acts: &mut Vec<Action>) {
    ui.heading("Shared and borrowed objects");
    let Some(result) = state.result.as_ref() else {
        ui.add(crate::spinner::Spinner::new());
        return;
    };
    let model = match result {
        Ok(Data::Sharing(v)) => v,
        Err(e) => {
            ui.colored_label(egui::Color32::RED, e);
            return;
        }
        _ => return,
    };
    ui.label(format!("Git directory: {}", model.git));
    ui.label(format!("Shared common directory: {}", model.common));
    ui.label(format!(
        "Object store: {}",
        crate::remote::display(&model.objects)
    ));
    ui.label("Worktrees using this common directory:");
    ui.monospace(&model.topology.worktrees);
    ui.separator();
    egui::ScrollArea::vertical()
        .max_height(290.0)
        .show(ui, |ui| {
            for store in &model.topology.stores {
                ui.group(|ui| {
                    ui.monospace(crate::remote::display(&store.path));
                    let borrowers: Vec<_> = model
                        .topology
                        .edges
                        .iter()
                        .filter(|(_, donor)| donor == &store.path)
                        .map(|(borrower, _)| crate::remote::display(borrower))
                        .collect();
                    if borrowers.is_empty() {
                        ui.label("This repository's primary store");
                    } else {
                        for borrower in borrowers {
                            ui.label(format!("Lends objects to {borrower}"));
                        }
                    }
                    if !store.available {
                        ui.colored_label(egui::Color32::RED, "Missing donor directory");
                    }
                    if let Some(e) = &store.error {
                        ui.colored_label(egui::Color32::RED, e);
                    }
                });
            }
            for warning in &model.topology.warnings {
                ui.colored_label(egui::Color32::YELLOW, warning);
            }
        });
    if model.topology.promisor {
        ui.label("Partial clone: some objects may require its promisor remote.");
    }
    ui.separator();
    ui.label("Make independent copies a complete pack for refs, original ancestry, reflogs and index entries, verifies it in a separate repository, then removes the alternate link. Donors remain unchanged. The original alternate metadata is retained as a recovery backup. Staging is limited to 128 MiB per pack; larger repositories can use git repack -a before preparing again.");
    if let Some(why) = &model.blocked {
        ui.label(why);
    }
    ui.checkbox(
        &mut state.acknowledged,
        "I stopped other programs writing to this repository and have a backup",
    );
    if ui
        .add_enabled(
            model.plan.is_some() && state.acknowledged,
            egui::Button::new("Make this clone independent"),
        )
        .clicked()
        && let Some(plan) = &model.plan
    {
        acts.push(Action::RepositoryTools(Act::MakeIndependent(Box::new(
            plan.clone(),
        ))));
    }
    if ui.button("Refresh topology and prepare again").clicked() {
        acts.push(Action::RepositoryTools(Act::Sharing));
    }
}
