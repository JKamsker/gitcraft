use super::{Act, Action, Data, State};
pub(super) fn show(ui: &mut egui::Ui, state: &mut State, acts: &mut Vec<Action>) {
    ui.heading(crate::i18n::text(
        "repo-workflows-repository_tools-show-07aa5dfe",
    ));
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
    ui.label(crate::i18n::message(
        "repo-workflows-repository_tools-show-1e4f897e",
        &[("model-git-1", (model.git).clone().into())],
    ));
    ui.label(crate::i18n::message(
        "repo-workflows-repository_tools-show-06b251d5",
        &[("model-common-1", (model.common).clone().into())],
    ));
    ui.label(crate::i18n::message(
        "repo-workflows-repository_tools-show-2d0adec7",
        &[(
            "crate-remote-display-model-objec-1",
            (crate::remote::display(&model.objects)).clone().into(),
        )],
    ));
    ui.label(crate::i18n::text(
        "repo-workflows-repository_tools-show-db839840",
    ));
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
                        ui.label(crate::i18n::text(
                            "repo-workflows-repository_tools-show-e0b832a0",
                        ));
                    } else {
                        for borrower in borrowers {
                            ui.label(crate::i18n::message(
                                "repo-workflows-repository_tools-show-864a00e5",
                                &[("borrower", (borrower).clone().into())],
                            ));
                        }
                    }
                    if !store.available {
                        ui.colored_label(
                            egui::Color32::RED,
                            crate::i18n::text("repo-workflows-repository_tools-show-11d962b1"),
                        );
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
        ui.label(crate::i18n::text(
            "repo-workflows-repository_tools-show-a1016f72",
        ));
    }
    ui.separator();
    ui.label(crate::i18n::text(
        "repo-workflows-repository_tools-show-6c1991e9",
    ));
    if let Some(why) = &model.blocked {
        ui.label(why);
    }
    ui.checkbox(
        &mut state.acknowledged,
        crate::i18n::text("repo-workflows-repository_tools-show-05e305a1"),
    );
    if ui
        .add_enabled(
            model.plan.is_some() && state.acknowledged,
            egui::Button::new(crate::i18n::text(
                "repo-workflows-repository_tools-show-301d9809",
            )),
        )
        .clicked()
        && let Some(plan) = &model.plan
    {
        acts.push(Action::RepositoryTools(Act::MakeIndependent(Box::new(
            plan.clone(),
        ))));
    }
    if ui
        .button(crate::i18n::text(
            "repo-workflows-repository_tools-show-ba8a03e2",
        ))
        .clicked()
    {
        acts.push(Action::RepositoryTools(Act::Sharing));
    }
}
