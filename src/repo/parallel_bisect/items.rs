//! Where a parallel bisect is offered: a commit's menu, the command palette and the bisect
//! banner.

use egui::Ui;

use super::ParallelAct;
use crate::repo::{Action, RepoTab};

/// Why a parallel bisect can't start while one runs.
pub(super) const RUNNING: &str = "repo-workflows-parallel_bisect--1df5e96c";

impl RepoTab {
    /// A commit's entry: bisect in parallel from it (the marks of the bisect going on, if any).
    pub(in crate::repo) fn parallel_menu_items(
        &self,
        ui: &mut Ui,
        hash: &str,
        acts: &mut Vec<Action>,
    ) {
        let running = self.parallel.running();
        let resp = crate::menu_item::MenuItem::new(crate::i18n::text(
            "repo-workflows-parallel_bisect-parallel-menu-items-be3adae3",
        ))
        .icon("command")
        .refused(running.then_some(crate::i18n::text(RUNNING)))
        .show(ui)
        .on_hover_text(crate::i18n::text(
            "repo-workflows-parallel_bisect-parallel-menu-items-7109f605",
        ))
        .on_disabled_hover_text(crate::i18n::text(RUNNING));
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
                crate::i18n::text("repo-workflows-parallel_bisect-parallel-palette-items-060e9182")
                    .to_owned(),
                "bisect".to_owned(),
                Action::ParallelBisect(ParallelAct::Cancel),
            )];
        }
        if self.refs.head.is_empty() {
            return Vec::new();
        }
        let head = self.refs.head.clone();
        vec![(
            crate::i18n::text("repo-workflows-parallel_bisect-parallel-palette-items-a69bdd78")
                .to_owned(),
            "bisect".to_owned(),
            self.parallel_dialog_for(&head),
        )]
    }

    /// The bisect banner's button: its marks, tested in parallel.
    pub(in crate::repo) fn parallel_banner_button(&self, ui: &mut Ui, acts: &mut Vec<Action>) {
        let ready = self.bisect.state().is_some_and(|b| b.progress.is_some());
        let running = self.parallel.running();
        let resp = ui
            .add_enabled(
                ready && !running,
                egui::Button::new(crate::i18n::text(
                    "repo-workflows-parallel_bisect-parallel-banner-button-2fe0d401",
                )),
            )
            .on_hover_text(crate::i18n::text(
                "repo-workflows-parallel_bisect-parallel-banner-button-0afdf9b5",
            ))
            .on_disabled_hover_text(if running {
                crate::i18n::text(RUNNING)
            } else {
                crate::i18n::text("repo-workflows-parallel_bisect-parallel-banner-button-c3efc1be")
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
