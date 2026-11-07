//! Reachable repository internals dialogs; stale background answers never replace a new view.
use super::{Action, Dialog, RepoTab};
use crate::git::repository_tools::all_refs;
use std::sync::atomic::{AtomicU64, Ordering};
static REQUESTS: AtomicU64 = AtomicU64::new(1);

pub(crate) enum Act {
    AllRefs(String),
}
pub(crate) struct State {
    id: u64,
    query: String,
    result: Option<Result<all_refs::Browser, String>>,
    filter: String,
}
impl RepoTab {
    pub(super) fn repository_tools_act(&mut self, act: Act, ctx: &egui::Context) {
        let Act::AllRefs(query) = act;
        let id = REQUESTS.fetch_add(1, Ordering::Relaxed);
        self.dialog = Some(Dialog::RepositoryTools(Box::new(State {
            id,
            query: query.clone(),
            result: None,
            filter: String::new(),
        })));
        let path = self.path.clone();
        let expected_path = path.clone();
        let profile = crate::git::profile_config();
        self.then_or(
            ctx,
            false,
            Err,
            move || all_refs::read(&path, &query),
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
        Ok(v) => v,
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
