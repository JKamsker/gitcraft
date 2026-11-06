//! Where a parallel bisect is offered: a commit's menu, the command palette and the bisect
//! banner.

use egui::Ui;

use super::ParallelAct;
use crate::repo::{Action, RepoTab};

/// Why a parallel bisect can't start while one runs.
pub(super) const RUNNING: &str = "A parallel bisect is running: wait for it, or cancel it";

impl RepoTab {
    /// A commit's entry: bisect in parallel from it (the marks of the bisect going on, if any).
    pub(in crate::repo) fn parallel_menu_items(
        &self,
        ui: &mut Ui,
        hash: &str,
        acts: &mut Vec<Action>,
    ) {
        let running = self.parallel.running();
        let resp = ui
            .add_enabled(!running, egui::Button::new("Bisect in parallel from here…"))
            .on_hover_text(
                "Tests several commits at once, each in a worktree of its own (here, in WSL or \
                 over SSH), to find the first bad commit; the checkout isn't touched",
            )
            .on_disabled_hover_text(RUNNING);
        if resp.clicked() {
            acts.push(self.parallel_dialog_for(hash));
        }
    }

    /// The dialog for a parallel bisect from `hash`: with the bisect's marks when it has both
    /// ends, else from that commit.
    fn parallel_dialog_for(&self, hash: &str) -> Action {
        self.parallel_from_bisect()
            .unwrap_or_else(|| self.parallel_from(hash))
    }

    /// What the command palette offers.
    pub(in crate::repo) fn parallel_palette_items(&self) -> Vec<(String, String, Action)> {
        if self.parallel.running() {
            return vec![(
                "Parallel bisect: cancel".to_owned(),
                "bisect".to_owned(),
                Action::ParallelBisect(ParallelAct::Cancel),
            )];
        }
        if self.refs.head.is_empty() {
            return Vec::new();
        }
        let head = self.refs.head.clone();
        vec![(
            "Bisect in parallel…".to_owned(),
            "bisect".to_owned(),
            self.parallel_dialog_for(&head),
        )]
    }

    /// The bisect banner's button: its marks, tested in parallel.
    pub(in crate::repo) fn parallel_banner_button(&self, ui: &mut Ui, acts: &mut Vec<Action>) {
        let ready = self.bisect.state().is_some_and(|b| b.progress.is_some());
        let running = self.parallel.running();
        let resp = ui
            .add_enabled(ready && !running, egui::Button::new("Run in parallel…"))
            .on_hover_text(
                "Tests several commits at once between these marks, each in a worktree of its \
                 own, here, in WSL or over SSH",
            )
            .on_disabled_hover_text(if running {
                RUNNING
            } else {
                "Mark a good and a bad commit first"
            });
        if resp.clicked()
            && let Some(action) = self.parallel_from_bisect()
        {
            acts.push(action);
        }
    }

    /// Clears, once per tab, what a parallel bisect that crashed left in the repository.
    pub(in crate::repo) fn clear_parallel_leftovers(&mut self) {
        if !std::mem::replace(&mut self.parallel.leftovers_cleared, true) {
            crate::parallel_bisect::clear_leftovers(self.path.clone());
        }
    }
}
