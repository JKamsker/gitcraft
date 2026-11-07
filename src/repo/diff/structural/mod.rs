//! Opt-in syntax comparison through Difftastic. The patch remains the source for staging.

mod model;
mod reader;
#[cfg(test)]
mod tests;
mod tool;
mod view;

pub(in crate::repo) use model::Structural;
pub(super) use view::show;

use crate::repo::{Action, Msg, RepoTab, spawn};

#[derive(Default)]
pub(in crate::repo) struct State {
    pub enabled: bool,
    pub result: Option<Result<Structural, String>>,
    load: u64,
    running: Option<(u64, u64)>,
    reload: bool,
}

impl State {
    /// Finish only our outstanding job. A refresh during it schedules one new reading,
    /// coalescing rapid working-tree changes rather than starting many difft processes.
    pub fn arrived(
        &mut self,
        ids: (u64, u64),
        current: u64,
        result: Result<Structural, String>,
    ) -> bool {
        if self.running != Some(ids) {
            return false;
        }
        self.running = None;
        if ids.0 == current && !self.reload {
            self.result = Some(result);
        }
        let reload = self.reload && self.enabled;
        self.reload = false;
        reload
    }
}

impl RepoTab {
    pub(in crate::repo) fn show_diff_structural(&mut self, on: bool, ctx: &egui::Context) {
        let Some(view) = &mut self.diff else { return };
        view.structural.enabled = on;
        view.chosen.clear();
        view.anchor = None;
        view.text_sel = None;
        if on {
            self.diff_rich = None;
            self.diff_as_file = false;
            self.diff_rendered = false;
            self.load_diff_structural(ctx);
        }
    }

    pub(in crate::repo) fn structural_arrived(
        &mut self,
        ids: (u64, u64),
        result: Result<Structural, String>,
        ctx: &egui::Context,
    ) {
        if self.structural_running != Some(ids) {
            return;
        }
        self.structural_running = None;
        let pending = std::mem::take(&mut self.structural_pending);
        let reload = self
            .diff
            .as_mut()
            .is_some_and(|view| view.structural.arrived(ids, view.request, result));
        if pending || reload {
            self.load_diff_structural(ctx);
        }
    }

    pub(in crate::repo) fn load_diff_structural(&mut self, ctx: &egui::Context) {
        let Some(view) = self.diff.as_mut().filter(|v| v.structural.enabled) else {
            return;
        };
        if self.structural_running.is_some() {
            self.structural_pending = true;
            view.structural.reload = true;
            view.structural.result = None;
            return;
        }
        view.structural.load += 1;
        view.structural.result = None;
        let ids = (view.request, view.structural.load);
        view.structural.running = Some(ids);
        self.structural_running = Some(ids);
        self.structural_pending = false;
        view.structural.reload = false;
        let (repo, key) = (self.path.clone(), view.key.clone());
        spawn(&self.tx, ctx, move || {
            let result = reader::load(&repo, &key);
            Msg::DiffStructural(ids, Box::new(result))
        });
    }
}

/// A reading-mode control, separate from staging and the patch's presentation options.
pub(super) fn toggle(ui: &mut egui::Ui, on: bool, acts: &mut Vec<Action>) {
    if ui
        .selectable_label(on, "Syntax")
        .on_hover_text("Compare syntax with Difftastic (difft), installed locally. SSH/WSL use the patch. Use the patch to stage changes.")
        .clicked()
    {
        acts.push(Action::DiffStructural(!on));
    }
}
