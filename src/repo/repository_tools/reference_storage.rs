//! Ref-storage migration and replacement-history inspection controls.

use super::*;

pub(super) fn reftable_view(ui: &mut egui::Ui, state: &mut State, acts: &mut Vec<Action>) {
    ui.heading(crate::i18n::text(
        "repo-workflows-repository_tools-reftable-view-c2782e54",
    ));
    let Some(result) = &state.result else {
        ui.add(crate::spinner::Spinner::new());
        return;
    };
    let storage = match result {
        Ok(Data::Reftable(v)) => v,
        Ok(_) => return,
        Err(e) => {
            ui.label(e);
            return;
        }
    };
    ui.label(crate::i18n::message(
        "repo-workflows-repository_tools-reftable-view-4101df87",
        &[
            ("storage-git-1", (storage.git).clone().into()),
            ("storage-format-2", (storage.format).clone().into()),
        ],
    ));
    ui.label(crate::i18n::text(
        "repo-workflows-repository_tools-reftable-view-faca3ce1",
    ));
    ui.label(crate::i18n::text(
        "repo-workflows-repository_tools-reftable-view-891eab64",
    ));
    if let Some(why) = &storage.blocked {
        ui.label(why);
    }
    ui.checkbox(
        &mut state.acknowledged,
        crate::i18n::text("repo-workflows-repository_tools-reftable-view-87ed192f"),
    );
    ui.checkbox(
        &mut state.compatible,
        crate::i18n::text("repo-workflows-repository_tools-reftable-view-ea4f0afa"),
    );
    let target = if storage.format == "files" {
        "reftable"
    } else {
        "files"
    };
    if ui
        .add_enabled(
            storage.migrates && storage.blocked.is_none() && state.acknowledged && state.compatible,
            egui::Button::new(crate::i18n::message(
                "repo-workflows-repository_tools-reftable-view-d3ba0aeb",
                &[("target", (target).into())],
            )),
        )
        .clicked()
    {
        acts.push(Action::RepositoryTools(Act::Migrate(
            Box::new(storage.clone()),
            target.into(),
        )));
    }
}

pub(super) fn replacements_view(ui: &mut egui::Ui, state: &mut State, acts: &mut Vec<Action>) {
    ui.heading(crate::i18n::text(
        "repo-workflows-repository_tools-replacements-view-f5b874e5",
    ));
    let Some(result) = &state.result else {
        ui.add(crate::spinner::Spinner::new());
        return;
    };
    let data = match result {
        Ok(Data::Replacements(v)) => v,
        Ok(_) => return,
        Err(e) => {
            ui.label(e);
            return;
        }
    };
    ui.label(crate::i18n::text(
        "repo-workflows-repository_tools-replacements-view-fcc3aff3",
    ));
    ui.checkbox(
        &mut state.acknowledged,
        crate::i18n::text("repo-workflows-repository_tools-replacements-view-9c43f2c2"),
    );
    for r in &data.replacements {
        ui.horizontal(|ui| {
            if ui
                .link(crate::i18n::message(
                    "repo-workflows-repository_tools-replacements-view-33ea34c6",
                    &[
                        ("r-original-1", (r.original).clone().into()),
                        ("r-effective-2", (r.effective).clone().into()),
                    ],
                ))
                .clicked()
            {
                acts.push(Action::RepositoryTools(Act::Object(r.effective.clone())));
            }
            if ui
                .add_enabled(
                    state.acknowledged,
                    egui::Button::new(crate::i18n::text(
                        "repo-workflows-repository_tools-replacements-view-4953f897",
                    )),
                )
                .clicked()
            {
                acts.push(Action::RepositoryTools(Act::RemoveReplacement(
                    Box::new(data.clone()),
                    r.original.clone(),
                    r.effective.clone(),
                )));
            }
        });
        ui.label(crate::i18n::message(
            "repo-workflows-repository_tools-replacements-view-5fcfdd61",
            &[("r-parents-1", (r.parents).clone().into())],
        ));
    }
    if !data.legacy.is_empty() {
        ui.label(crate::i18n::text(
            "repo-workflows-repository_tools-replacements-view-7b8c9687",
        ));
        ui.monospace(&data.legacy);
        if ui
            .add_enabled(
                state.acknowledged,
                egui::Button::new(crate::i18n::text(
                    "repo-workflows-repository_tools-replacements-view-1714de4d",
                )),
            )
            .clicked()
        {
            acts.push(Action::RepositoryTools(Act::ConvertGrafts(Box::new(
                data.clone(),
            ))));
        }
    }
    ui.horizontal(|ui| {
        ui.selectable_value(
            &mut state.effective,
            true,
            crate::i18n::text("repo-workflows-repository_tools-replacements-view-0a69dfc6"),
        );
        ui.selectable_value(
            &mut state.effective,
            false,
            crate::i18n::text("repo-workflows-repository_tools-replacements-view-4cff5861"),
        );
    });
    egui::ScrollArea::both()
        .id_salt("replacement_history")
        .max_height(280.0)
        .show(ui, |ui| {
            ui.monospace(if state.effective {
                &data.effective
            } else {
                &data.original
            });
        });
    ui.separator();
    ui.label(crate::i18n::text(
        "repo-workflows-repository_tools-replacements-view-e51254f0",
    ));
    ui.horizontal(|ui| {
        ui.label(crate::i18n::text(
            "repo-workflows-repository_tools-replacements-view-84da95ea",
        ));
        ui.text_edit_singleline(&mut state.query);
    });
    ui.horizontal(|ui| {
        ui.label(crate::i18n::text(
            "repo-workflows-repository_tools-replacements-view-b022884c",
        ));
        ui.text_edit_singleline(&mut state.parent);
    });
    if ui
        .add_enabled(
            state.acknowledged && !state.query.is_empty() && !state.parent.is_empty(),
            egui::Button::new(crate::i18n::text(
                "repo-workflows-repository_tools-replacements-view-7916bff6",
            )),
        )
        .clicked()
    {
        acts.push(Action::RepositoryTools(Act::Graft(
            Box::new(data.clone()),
            state.query.trim().into(),
            state.parent.trim().into(),
        )));
    }
}
