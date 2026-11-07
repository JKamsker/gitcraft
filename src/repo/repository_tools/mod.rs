//! Reachable repository internals dialogs; stale background answers never replace a new view.
use super::{Action, Dialog, RepoTab};
use crate::git::repository_tools::{all_refs, object};
use std::sync::atomic::{AtomicU64, Ordering};
static REQUESTS: AtomicU64 = AtomicU64::new(1);

pub(crate) enum Act {
    AllRefs(String),
    Object(String),
}
enum Data {
    Refs(all_refs::Browser),
    Object(object::Inspection),
}
pub(crate) struct State {
    id: u64,
    query: String,
    result: Option<Result<Data, String>>,
    filter: String,
    object: bool,
}
impl RepoTab {
    pub(super) fn repository_tools_act(&mut self, act: Act, ctx: &egui::Context) {
        let (query, is_object) = match act {
            Act::AllRefs(q) => (q, false),
            Act::Object(q) => (q, true),
        };
        let id = REQUESTS.fetch_add(1, Ordering::Relaxed);
        self.dialog = Some(Dialog::RepositoryTools(Box::new(State {
            id,
            query: query.clone(),
            result: None,
            filter: String::new(),
            object: is_object,
        })));
        let path = self.path.clone();
        let expected_path = path.clone();
        let profile = crate::git::profile_config();
        self.then_or(
            ctx,
            false,
            Err,
            move || {
                if is_object {
                    object::inspect(&path, &query, &|| {
                        !crate::tasks::cancellation().is_cancelled()
                    })
                    .map(Data::Object)
                } else {
                    all_refs::read(&path, &query).map(Data::Refs)
                }
            },
            move |tab, result, _| {
                if tab.path != expected_path || crate::git::profile_config() != profile {
                    return;
                }
                if let Some(Dialog::RepositoryTools(state)) = &mut tab.dialog
                    && state.id == id
                {
                    state.result = Some(result);
                }
            },
        );
    }
}

pub(super) fn show(ui: &mut egui::Ui, state: &mut State, acts: &mut Vec<Action>) {
    ui.set_width(850.0);
    if state.object {
        object_view(ui, state, acts);
        return;
    }
    ui.heading("All refs and namespace history");
    ui.label("Includes application, bisect, backup and namespace refs. Object IDs name the exact snapshot below.");
    ui.horizontal(|ui| {
        ui.label("History of ref / prefix (blank = all):");
        ui.text_edit_singleline(&mut state.query);
        if ui.button("Read").clicked() {
            acts.push(Action::RepositoryTools(Act::AllRefs(state.query.clone())));
        }
    });
    let Some(result) = &state.result else {
        ui.spinner();
        return;
    };
    let browser = match result {
        Ok(Data::Refs(v)) => v,
        Ok(_) => return,
        Err(e) => {
            ui.colored_label(crate::theme::Tokens::get(ui.ctx()).danger, e);
            return;
        }
    };
    if browser.root_fallback {
        ui.label("This Git cannot enumerate arbitrary root refs; established pseudorefs are included when present.");
    }
    ui.horizontal(|ui| {
        ui.label("Filter refs:");
        ui.text_edit_singleline(&mut state.filter);
    });
    egui::ScrollArea::vertical()
        .id_salt("all_refs")
        .max_height(270.0)
        .show(ui, |ui| {
            for r in browser
                .refs
                .iter()
                .filter(|r| r.name.contains(&state.filter))
            {
                ui.horizontal(|ui| {
                    if ui.selectable_label(false, &r.name).clicked() {
                        acts.push(Action::RepositoryTools(Act::AllRefs(r.name.clone())));
                    }
                    ui.monospace(&r.oid);
                    ui.label(&r.kind);
                    if !r.symbolic.is_empty() && ui.link(format!("→ {}", r.symbolic)).clicked() {
                        acts.push(Action::RepositoryTools(Act::AllRefs(r.symbolic.clone())));
                    }
                    if ui.small_button("Inspect object").clicked() {
                        acts.push(Action::RepositoryTools(Act::Object(r.oid.clone())));
                    }
                    if ui.small_button("Copy object").clicked() {
                        ui.ctx().copy_text(r.oid.clone());
                    }
                });
            }
        });
    ui.separator();
    ui.label("History (up to 200 commits)");
    egui::ScrollArea::both()
        .id_salt("namespace_history")
        .max_height(260.0)
        .show(ui, |ui| {
            ui.monospace(&browser.history);
        });
}

fn object_view(ui: &mut egui::Ui, state: &mut State, acts: &mut Vec<Action>) {
    ui.heading("How an object is stored");
    ui.label("Object ID or file version, for example HEAD:src/main.rs. Inspection never downloads missing objects. Git 2.45+ is required for this guarantee.");
    ui.horizontal(|ui| {
        ui.text_edit_singleline(&mut state.query);
        if ui.button("Inspect").clicked() {
            acts.push(Action::RepositoryTools(Act::Object(state.query.clone())));
        }
    });
    let Some(result) = &state.result else {
        ui.spinner();
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
    egui::ScrollArea::both().id_salt("object_inspector").max_height(600.0).show(ui, |ui| {
        ui.label(format!("{} · {} · resolved from {}", data.format, data.objects.display(), data.query));
        ui.label(format!("Git directory: {}\nShared directory: {}", data.git_dir, data.common_dir));
        if let Some(o) = &data.object {
            ui.monospace(&o.oid); ui.label(format!("{} · {} bytes expanded · {} bytes on disk", o.kind, o.size, o.disk));
            if let Some(base) = &o.base && ui.link(format!("Delta base: {base}")).clicked() { acts.push(Action::RepositoryTools(Act::Object(base.clone()))); }
        }
        if let Some(missing) = &data.missing { ui.label(missing); }
        for note in &data.notes { ui.label(note); }
        for representation in &data.representations {
            ui.separator(); ui.monospace(representation.path.display().to_string());
            if let Some(n) = representation.loose_bytes { ui.label(format!("Loose object: {n} compressed bytes")); }
            for (i, row) in representation.chain.iter().enumerate() {
                if ui.link(format!("{}: {} · {} · packed {} B · representation {} B · offset {} · depth {}", if i == 0 { "Object" } else { "Base" }, row.oid, row.kind, row.disk, row.size, row.offset, row.depth)).clicked() { acts.push(Action::RepositoryTools(Act::Object(row.oid.clone()))); }
            }
            if representation.chain.iter().any(|r| r.base.is_some()) { ui.label("For deltas, representation size describes the delta instruction stream; expanded object size is above."); }
        }
        ui.separator(); ui.label("Tree entries / linked objects");
        for entry in &data.entries {
            if ui.link(format!("{} · {} · {}", entry.name, entry.kind, entry.oid)).clicked() { acts.push(Action::RepositoryTools(Act::Object(entry.oid.clone()))); }
        }
    });
}
