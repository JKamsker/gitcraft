use super::{Act, Data, State};
use crate::repo::Action;
pub(super) fn show(ui: &mut egui::Ui, state: &mut State, acts: &mut Vec<Action>) {
    ui.heading("Submodule conflict graph");
    let Some(result) = &state.result else {
        ui.spinner();
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
    ui.label(format!(
        "{} · base {} · ours {} · theirs {}",
        c.path,
        c.base.as_deref().unwrap_or("none"),
        c.ours,
        c.theirs
    ));
    ui.label("Pick an existing commit containing both sides, or open the submodule, merge there, then refresh this graph. Recording changes only the parent index.");
    ui.horizontal(|ui| {
        if ui.button("Open submodule to merge").clicked() {
            acts.push(Action::OpenTab(c.child.clone()));
        }
        if ui.button("Refresh after merge").clicked() {
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
    ui.label("Known commits containing both sides (you can also paste any full commit ID):");
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
    ui.label(format!("Submodule checkout: {}", c.head));
    ui.text_edit_singleline(&mut state.parent);
    ui.checkbox(
        &mut state.acknowledged,
        "I reviewed the selected commit and want to record this parent resolution",
    );
    if ui
        .add_enabled(
            state.acknowledged && !state.parent.is_empty(),
            egui::Button::new("Record chosen submodule commit in parent"),
        )
        .clicked()
    {
        acts.push(Action::RepositoryTools(Act::ResolveSubmodule(
            Box::new(c.clone()),
            state.parent.trim().into(),
        )));
    }
}
