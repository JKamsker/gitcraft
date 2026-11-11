//! Object inspection, navigation, and guarded export controls.

use super::*;

pub(super) fn object_view(ui: &mut egui::Ui, state: &mut State, acts: &mut Vec<Action>) {
    ui.heading(crate::i18n::text(
        "repo-workflows-repository_tools-object-view-8efd32c4",
    ));
    ui.label(crate::i18n::text(
        "repo-workflows-repository_tools-object-view-3e15593a",
    ));
    ui.horizontal(|ui| {
        ui.text_edit_singleline(&mut state.query);
        if ui
            .button(crate::i18n::text(
                "repo-workflows-repository_tools-object-view-e0723a86",
            ))
            .clicked()
        {
            acts.push(Action::RepositoryTools(Act::Object(state.query.clone())));
        }
    });
    let Some(result) = &state.result else {
        ui.add(crate::spinner::Spinner::new());
        return;
    };
    let data = match result {
        Ok(Data::Object(v)) => v,
        Ok(_) => return,
        Err(e) => {
            ui.colored_label(crate::theme::Tokens::get(ui.ctx()).danger, e);
            return;
        }
    };
    egui::ScrollArea::both()
        .id_salt("object_inspector")
        .max_height(600.0)
        .show(ui, |ui| {
            ui.label(crate::i18n::message(
                "repo-workflows-repository_tools-object-view-9147a920",
                &[
                    ("data-format-1", (data.format).clone().into()),
                    (
                        "data-objects-display-2",
                        data.objects.display().to_string().into(),
                    ),
                    ("data-query-3", (data.query).clone().into()),
                ],
            ));
            ui.label(crate::i18n::message(
                "repo-workflows-repository_tools-object-view-ddc47a7a",
                &[
                    ("data-git-dir-1", (data.git_dir).clone().into()),
                    ("data-common-dir-2", (data.common_dir).clone().into()),
                ],
            ));
            if let Some(o) = &data.object {
                ui.monospace(&o.oid);
                ui.label(crate::i18n::message(
                    "repo-workflows-repository_tools-object-view-44de072f",
                    &[
                        ("o-kind-1", (o.kind).clone().into()),
                        ("o-size-2", (o.size).into()),
                        ("o-disk-3", (o.disk).into()),
                    ],
                ));
                if let Some(base) = &o.base
                    && ui
                        .link(crate::i18n::message(
                            "repo-workflows-repository_tools-object-view-b39d687c",
                            &[("base", (base).clone().into())],
                        ))
                        .clicked()
                {
                    acts.push(Action::RepositoryTools(Act::Object(base.clone())));
                }
            }
            if let Some(missing) = &data.missing {
                ui.label(missing);
            }
            for note in &data.notes {
                ui.label(note);
            }
            for representation in &data.representations {
                ui.separator();
                ui.monospace(representation.path.display().to_string());
                if let Some(n) = representation.loose_bytes {
                    ui.label(crate::i18n::message(
                        "repo-workflows-repository_tools-object-view-14a9c8ce",
                        &[("n", (n).into())],
                    ));
                }
                for (i, row) in representation.chain.iter().enumerate() {
                    if ui
                        .link(crate::i18n::message(
                            "repo-workflows-repository_tools-object-view-f4de0102",
                            &[
                                (
                                    "if-i-0-object-else-base-1",
                                    (if i == 0 {
                                        crate::i18n::text(
                                            "repo-workflows-repository_tools-object-view-62a6da87",
                                        )
                                    } else {
                                        crate::i18n::text(
                                            "repo-workflows-repository_tools-object-view-7b47361a",
                                        )
                                    })
                                    .into(),
                                ),
                                ("row-oid-2", (row.oid).clone().into()),
                                ("row-kind-3", (row.kind).clone().into()),
                                ("row-disk-4", (row.disk).into()),
                                ("row-size-5", (row.size).into()),
                                ("row-offset-6", (row.offset).into()),
                                ("row-depth-7", (row.depth).into()),
                            ],
                        ))
                        .clicked()
                    {
                        acts.push(Action::RepositoryTools(Act::Object(row.oid.clone())));
                    }
                }
                if representation.chain.iter().any(|r| r.base.is_some()) {
                    ui.label(crate::i18n::text(
                        "repo-workflows-repository_tools-object-view-9d4281bc",
                    ));
                }
            }
            ui.separator();
            ui.label(crate::i18n::text(
                "repo-workflows-repository_tools-object-view-56daa2be",
            ));
            for entry in &data.entries {
                if ui
                    .link(format!("{} · {} · {}", entry.name, entry.kind, entry.oid))
                    .clicked()
                {
                    acts.push(Action::RepositoryTools(Act::Object(entry.oid.clone())));
                }
            }
        });
}
