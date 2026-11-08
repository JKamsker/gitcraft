use super::{Act, Data, State};
use crate::repo::Action;
pub(super) fn show(ui: &mut egui::Ui, state: &mut State, acts: &mut Vec<Action>) {
    ui.heading(crate::i18n::text(
        "repo-workflows-repository_tools-show-51768487",
    ));
    let Some(result) = &state.result else {
        ui.add(crate::spinner::Spinner::new());
        return;
    };
    let c = match result {
        Ok(Data::SubmoduleConflict(c)) => c,
        Ok(_) => return,
        Err(e) => {
            ui.label(e);
            return;
        }
    };
    ui.label(crate::i18n::message(
        "repo-workflows-repository_tools-show-3acc29f8",
        &[
            ("c-path-1", (c.path).clone().into()),
            (
                "c-base-as-deref-unwrap-or-none-2",
                (c.base
                    .as_deref()
                    .unwrap_or_else(|| crate::i18n::text("repo-workflows-paint-no-base")))
                .clone()
                .into(),
            ),
            ("c-ours-3", (c.ours).clone().into()),
            ("c-theirs-4", (c.theirs).clone().into()),
        ],
    ));
    ui.label(crate::i18n::text(
        "repo-workflows-repository_tools-show-b26a038e",
    ));
    ui.horizontal(|ui| {
        if ui
            .button(crate::i18n::text(
                "repo-workflows-repository_tools-show-46aafc00",
            ))
            .clicked()
        {
            acts.push(Action::OpenTab(c.child.clone()));
        }
        if ui
            .button(crate::i18n::text(
                "repo-workflows-repository_tools-show-bbcce2f9",
            ))
            .clicked()
        {
            acts.push(Action::RepositoryTools(Act::SubmoduleConflict(
                c.path.clone(),
            )));
        }
    });
    egui::ScrollArea::both()
        .id_salt("submodule_conflict_graph")
        .max_height(290.0)
        .show(ui, |ui| {
            ui.monospace(&c.graph);
        });
    ui.label(crate::i18n::text(
        "repo-workflows-repository_tools-show-44e05a73",
    ));
    if c.candidates_limited {
        ui.label(crate::i18n::text(
            "repo-workflows-repository_tools-show-6c892519",
        ));
    }
    for candidate in &c.candidates {
        if ui
            .selectable_label(
                state.parent == candidate.oid,
                format!("{} · {}", candidate.label, candidate.oid),
            )
            .clicked()
        {
            state.parent = candidate.oid.clone();
        }
    }
    ui.label(crate::i18n::message(
        "repo-workflows-repository_tools-show-0e95ad58",
        &[("c-head-1", (c.head).clone().into())],
    ));
    ui.text_edit_singleline(&mut state.parent);
    if let Some(why) = &c.recording_blocker {
        ui.label(why);
    }
    ui.checkbox(
        &mut state.acknowledged,
        crate::i18n::text("repo-workflows-repository_tools-show-edc5ba83"),
    );
    if ui
        .add_enabled(
            c.recording_blocker.is_none() && state.acknowledged && !state.parent.is_empty(),
            egui::Button::new(crate::i18n::text(
                "repo-workflows-repository_tools-show-b7b34c26",
            )),
        )
        .clicked()
    {
        acts.push(Action::RepositoryTools(Act::ResolveSubmodule(
            Box::new(c.clone()),
            state.parent.trim().into(),
        )));
    }
}
