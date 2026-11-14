//! Full-tree recovery is separate from ZIP archives: no quota eviction or automatic purge.
use std::path::PathBuf;

use egui::Ui;

use super::window::Asks;
use crate::git::worktree_cleanup::archive::Store;
use crate::repo::RepoTab;

#[derive(Default)]
pub(super) struct Retained {
    items: Option<Result<Vec<(PathBuf, String)>, String>>,
    armed: Option<PathBuf>,
    running: bool,
    said: Option<String>,
}

pub(super) fn show(ui: &mut Ui, retained: &mut Retained, asks: &mut Asks) {
    ui.separator();
    ui.label(crate::i18n::text("worktree-cleanup-recovery-title"));
    ui.label(crate::i18n::text("worktree-cleanup-recovery-hint"));
    if let Some(said) = &retained.said {
        ui.label(said);
    }
    let items = match &retained.items {
        Some(Ok(items)) => items,
        Some(Err(e)) => {
            ui.label(e);
            return;
        }
        None => {
            ui.add(crate::spinner::Spinner::new());
            return;
        }
    };
    if items.is_empty() {
        ui.label(crate::i18n::text("worktree-cleanup-recovery-empty"));
    }
    for (directory, label) in items {
        ui.horizontal_wrapped(|ui| {
            ui.label(label)
                .on_hover_text(directory.join("tree").display().to_string());
            ui.add_enabled_ui(!retained.running, |ui| {
                if ui
                    .small_button(crate::i18n::text("worktree-cleanup-recovery-restore"))
                    .clicked()
                {
                    asks.retained = Some((directory.clone(), false));
                }
                let armed = retained.armed.as_ref() == Some(directory);
                let text = crate::i18n::text(if armed {
                    "worktree-cleanup-purge-confirm"
                } else {
                    "worktree-cleanup-purge"
                });
                if ui.small_button(text).clicked() {
                    if armed {
                        asks.retained = Some((directory.clone(), true));
                    } else {
                        retained.armed = Some(directory.clone());
                    }
                }
            });
        });
    }
}

impl RepoTab {
    pub(super) fn read_retained(&mut self, ctx: &egui::Context) {
        let repo = self.path.clone();
        self.then(
            ctx,
            move || Store::recoveries(&repo),
            |tab, items, _| {
                if let Some(window) = &mut tab.worktree_cleanup.window {
                    window.retained.items = Some(items);
                }
            },
        );
    }

    pub(super) fn retained_action(&mut self, directory: PathBuf, purge: bool, ctx: &egui::Context) {
        let Some(window) = &mut self.worktree_cleanup.window else {
            return;
        };
        if window.running || window.retained.running {
            return;
        }
        window.retained.running = true;
        let repo = self.path.clone();
        self.then(
            ctx,
            move || {
                if purge {
                    Store::purge_recovery(&repo, &directory)
                } else {
                    Store::restore_recovery(&repo, &directory)
                }
            },
            |tab, result, ctx| {
                if let Some(window) = &mut tab.worktree_cleanup.window {
                    window.retained.running = false;
                    window.retained.armed = None;
                    window.retained.said = Some(match result {
                        Ok(()) => crate::i18n::text("worktree-cleanup-recovery-done").to_owned(),
                        Err(e) => e,
                    });
                }
                tab.reload(ctx);
                tab.read_retained(ctx);
                tab.read_worktrees(ctx);
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui_kittest::{Harness, kittest::Queryable};

    #[test]
    fn purge_requires_its_own_second_confirmation() {
        let directory = PathBuf::from("/synthetic/recovery/tree-one");
        let retained = Retained {
            items: Some(Ok(vec![(directory.clone(), "worktree".into())])),
            ..Default::default()
        };
        let mut h = Harness::builder().build_ui_state(
            |ui, state: &mut (Retained, Option<(PathBuf, bool)>)| {
                let mut asks = Asks::default();
                show(ui, &mut state.0, &mut asks);
                if asks.retained.is_some() {
                    state.1 = asks.retained;
                }
            },
            (retained, None),
        );
        h.get_by_label("Purge…").click();
        h.step();
        h.step();
        assert!(h.state().1.is_none());
        h.get_by_label("Permanently purge all files?").click();
        h.step();
        h.step();
        assert_eq!(h.state().1.as_ref(), Some(&(directory, true)));
    }
}
