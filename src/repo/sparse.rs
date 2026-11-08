//! Sparse checkout and one-operation download consent, scoped to this repository tab.
use super::{Action, Refresh, RepoTab, history_reader::Reader};
use crate::git::{self, SparseState};
use std::collections::BTreeSet;

#[derive(Default)]
pub(super) struct State {
    reader: Reader<SparseState>,
    before: Option<SparseState>,
    selected: BTreeSet<String>,
    all: bool,
    consent: bool,
    writing: bool,
    error: Option<String>,
    profile: Option<std::path::PathBuf>,
}
impl RepoTab {
    pub(super) fn open_sparse(&mut self) {
        if self.sparse.as_ref().is_some_and(|s| s.writing) {
            return;
        }
        let repo = self.path.clone();
        let mut state = State {
            profile: git::profile_config(),
            ..Default::default()
        };
        state.reader.queue(move |_| git::sparse_state(&repo));
        self.sparse = Some(state);
    }
    pub(super) fn sparse_window(&mut self, ctx: &egui::Context, acts: &mut Vec<Action>) {
        if self
            .sparse
            .as_ref()
            .is_some_and(|s| !s.writing && s.profile != git::profile_config())
        {
            self.sparse = None;
        }
        let Some(state) = &mut self.sparse else {
            return;
        };
        if let Some(result) = state.reader.poll(&self.tx, ctx) {
            match result {
                Ok(before) => {
                    state.all = !before.enabled;
                    state.selected = before.selected.clone();
                    state.before = Some(before);
                }
                Err(e) => state.error = Some(e),
            }
        }
        let mut open = true;
        let mut apply = false;
        let mut fetch = false;
        let idle = self.busy_op.is_none() && !state.writing && !state.reader.busy();
        egui::Window::new(crate::i18n::text("repo-core-3603"))
            .id(egui::Id::new(("sparse", &self.path)))
            .open(&mut open)
            .show(ctx, |ui| {
                ui.label(crate::i18n::text("repo-core-3605"));
                ui.label(crate::i18n::text("repo-core-3606"));
                ui.label(crate::i18n::text("repo-core-3607"));
                ui.add_enabled_ui(idle, |ui| {
                    ui.checkbox(&mut state.consent, crate::i18n::text("repo-core-3608"));
                    if let Some(before) = &state.before {
                        if before.enabled && !before.cone {
                            ui.label(crate::i18n::text("repo-core-3609"));
                        }
                        ui.checkbox(&mut state.all, crate::i18n::text("repo-core-3610"));
                        egui::ScrollArea::vertical()
                            .max_height(260.0)
                            .show(ui, |ui| {
                                ui.add_enabled_ui(!state.all, |ui| {
                                    for folder in &before.folders {
                                        let mut picked = state.selected.contains(folder);
                                        let depth = folder.bytes().filter(|b| *b == b'/').count();
                                        ui.horizontal(|ui| {
                                            ui.add_space((depth as f32 * 12.0).min(120.0));
                                            if ui.checkbox(&mut picked, folder).changed() {
                                                if picked {
                                                    state.selected.insert(folder.clone());
                                                } else {
                                                    state.selected.remove(folder);
                                                }
                                            }
                                        });
                                    }
                                });
                            });
                        if ui.button(crate::i18n::text("repo-core-3611")).clicked() {
                            apply = true;
                        }
                        if before.partial {
                            ui.separator();
                            ui.label(crate::i18n::text("repo-core-3612"));
                            if ui
                                .add_enabled(
                                    state.consent,
                                    egui::Button::new(crate::i18n::text("repo-core-3613")),
                                )
                                .clicked()
                            {
                                fetch = true;
                            }
                        }
                    }
                });
                if state.writing || state.reader.busy() {
                    ui.label(crate::i18n::text("repo-core-3614"));
                }
                if let Some(error) = &state.error {
                    ui.colored_label(crate::theme::Tokens::get(ctx).danger, error);
                }
            });
        if fetch {
            state.consent = false;
            acts.push(Action::Ops {
                label: crate::i18n::text("repo-core-3615").into(),
                ops: super::ops(&[&["fetch", "--refetch", "--no-filter", "--", "origin"]]),
                refresh: Refresh::All,
            });
        }
        if apply && let Some(before) = state.before.clone() {
            let folders = (!state.all).then(|| state.selected.iter().cloned().collect::<Vec<_>>());
            let consent = std::mem::take(&mut state.consent);
            let repo = self.path.clone();
            let credentials = self.credentials.clone();
            state.writing = true;
            state.error = None;
            self.busy_op = Some(crate::i18n::text("repo-core-3618").into());
            self.then_or(
                ctx,
                true,
                Err,
                move || {
                    git::set_sparse(
                        &repo,
                        &before,
                        folders.as_deref(),
                        consent,
                        credentials.as_ref(),
                    )
                },
                |tab, result, ctx| {
                    tab.busy_op = None;
                    match result {
                        Ok(()) => {
                            if let Some(state) = &mut tab.sparse {
                                state.writing = false;
                            }
                            tab.open_sparse();
                            tab.reload(ctx);
                        }
                        Err(e) => {
                            if let Some(state) = &mut tab.sparse {
                                state.writing = false;
                                state.error = Some(e);
                            }
                        }
                    }
                },
            );
        } else if !open && !state.writing {
            self.sparse = None;
        }
    }
}
