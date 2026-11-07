use super::{Act, Data, State};
use crate::repo::Action;
fn lines(ui: &mut egui::Ui, title: &str, values: &mut Vec<String>) {
    ui.label(title);
    let mut text = values.join("\n");
    if ui
        .add(
            egui::TextEdit::multiline(&mut text)
                .desired_rows(2)
                .desired_width(f32::INFINITY),
        )
        .changed()
    {
        *values = text
            .lines()
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
            .collect();
    }
}
pub(super) fn show(ui: &mut egui::Ui, state: &mut State, acts: &mut Vec<Action>) {
    ui.heading("Remote URLs, refspecs and mirror policy");
    let Some(result) = &mut state.result else {
        ui.spinner();
        return;
    };
    let e = match result {
        Ok(Data::Remote(v)) => v,
        Ok(_) => return,
        Err(e) => {
            ui.label(e.as_str());
            return;
        }
    };
    ui.label(format!(
        "Remote: {} · all changes are repository-local",
        e.draft.name
    ));
    egui::ScrollArea::vertical().id_salt("remote_refspec_editor").max_height(570.0).show(ui, |ui| {
        lines(ui, "Fetch URLs (first is fetched; one per line)", &mut e.draft.fetch_urls);
        lines(ui, "Push URLs (blank inherits fetch URLs; Git pushes to every listed URL)", &mut e.draft.push_urls);
        lines(ui, "Fetch refspecs: +source:destination; ^source excludes a branch/prefix", &mut e.draft.fetch);
        lines(ui, "Push refspecs (blank uses push.default)", &mut e.draft.push);
        ui.checkbox(&mut e.draft.mirror, "Mirror pushes: force all refs and delete extra destination refs");
        ui.label("Fetch mirroring is expressed by +refs/*:refs/*. It writes directly into local namespaces, including branches. Push mirroring can overwrite or delete others' work. These previews never prune or push.");
        if ui.button("Preview mappings and prune deletions against fetch URL").clicked() { acts.push(Action::RepositoryTools(Act::RemotePreview(Box::new(e.clone())))); }
        if let Some(p) = &e.preview {
            if p.draft != e.draft { ui.label("Edits changed since preview; preview again before saving."); }
            for mapping in &p.mappings { ui.monospace(mapping); }
            ui.label(format!("Pruning with these mappings would delete {} local refs:", p.prune.len()));
            for name in &p.prune { ui.monospace(name); }
        }
        ui.checkbox(&mut state.acknowledged, "I reviewed these URLs, mappings and mirror consequences");
        let ready = state.acknowledged && e.preview.as_ref().is_some_and(|p| p.draft == e.draft);
        if ui.add_enabled(ready, egui::Button::new("Save remote configuration")).clicked() { acts.push(Action::RepositoryTools(Act::SaveRemote(Box::new(e.clone())))); }
    });
}
