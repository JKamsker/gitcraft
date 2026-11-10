//! Development-container window and consent presentation.

use super::*;

impl RepoTab {
    pub(in crate::repo) fn devcontainer_window(
        &mut self,
        ctx: &egui::Context,
        acts: &mut Vec<Action>,
    ) {
        let state = &mut self.devcontainer;
        if let Some(pending) = &state.pending {
            let stale = pending.context != connections::current() || pending.scope.0.is_cancelled();
            if stale {
                pending.scope.0.cancel();
            }
            match pending.rx.try_recv() {
                Ok(reply) => {
                    state.result_presentation = None;
                    let mutation = pending.mutation;
                    state.pending = None;
                    if mutation {
                        self.busy_op = None;
                    }
                    state.trust = false;
                    state.rebuild = false;
                    if stale {
                        state.config = None;
                        state.binding = None;
                        state.checked = false;
                        state.remember_result(LocalResult::ProfileChanged);
                    } else {
                        match reply {
                            Ok(Reply::Config(config)) => {
                                if config.is_some() {
                                    state.open = true;
                                }
                                state.config = config;
                                state.remember_result(if state.config.is_some() {
                                    LocalResult::Detected
                                } else {
                                    LocalResult::Missing
                                });
                            }
                            Ok(Reply::Started(binding)) => {
                                state.remember_result(LocalResult::Started);
                                state.binding = Some(binding);
                            }
                            Ok(Reply::Editor(result)) => state.result = result,
                            Ok(Reply::Terminal(ready)) => {
                                if let Some(term) = ready
                                    .slot
                                    .lock()
                                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                                    .take()
                                {
                                    self.terminals.opened(Ok(term));
                                    state.remember_result(LocalResult::TerminalOpened);
                                } else {
                                    state.remember_result(LocalResult::TerminalExpired);
                                }
                            }
                            Err(error) => {
                                state.remember_result(LocalResult::Failed { detail: error });
                            }
                        }
                    }
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    if pending.mutation {
                        self.busy_op = None;
                    }
                    state.pending = None;
                    state.remember_result(LocalResult::WorkerEnded);
                }
                Err(mpsc::TryRecvError::Empty) => {
                    ctx.request_repaint_after(std::time::Duration::from_millis(100))
                }
            }
        }
        let state = &mut self.devcontainer;
        if state.pending.is_none()
            && state
                .config
                .as_ref()
                .is_some_and(|c| c.context != connections::current())
        {
            state.checked = false;
            state.config = None;
            state.binding = None;
            state.trust = false;
        }
        if !state.checked && state.pending.is_none() {
            acts.push(Action::Devcontainer(Act::Read));
        }
        if !state.open {
            return;
        }
        let mut open = true;
        egui::Window::new(crate::i18n::text(
            "repo-workflows-devcontainer-devcontainer-window-b64ae277",
        ))
        .id(egui::Id::new(("devcontainer", &self.path)))
        .open(&mut open)
        .default_width(730.0)
        .vscroll(true)
        .show(ctx, |ui| {
            ui.label(crate::i18n::text(
                "repo-workflows-devcontainer-devcontainer-window-3c898437",
            ));
            if state.pending.is_some() {
                ui.add(crate::spinner::Spinner::new());
                if ui
                    .button(crate::i18n::text(
                        "repo-workflows-devcontainer-devcontainer-window-b64a0d4f",
                    ))
                    .clicked()
                {
                    acts.push(Action::Devcontainer(Act::Cancel));
                }
            }
            state.result_ui(ui);
            ui.add_enabled_ui(state.pending.is_none() && self.busy_op.is_none(), |ui| {
                if ui
                    .button(crate::i18n::text(
                        "repo-workflows-devcontainer-devcontainer-window-437e57b5",
                    ))
                    .clicked()
                {
                    acts.push(Action::Devcontainer(Act::Read));
                }
                let Some(config) = &state.config else {
                    return;
                };
                ui.monospace(crate::i18n::message(
                    "repo-workflows-devcontainer-devcontainer-window-3675738c",
                    &[
                        (
                            "config-path-display-1",
                            config.path.display().to_string().into(),
                        ),
                        ("config-label-2", (config.label()).clone().into()),
                        (
                            "model-hash-config-source-as-byte-3",
                            (model::hash(config.source.as_bytes())).clone().into(),
                        ),
                    ],
                ));
                egui::CollapsingHeader::new(crate::i18n::text(
                    "repo-workflows-devcontainer-devcontainer-window-df4a39c8",
                ))
                .id_salt("devcontainer-source")
                .show(ui, |ui| {
                    ui.add(egui::Label::new(&config.source).selectable(true));
                });
                ui.label(crate::i18n::text(
                    "repo-workflows-devcontainer-devcontainer-window-3a6ea0e2",
                ));
                ui.checkbox(
                    &mut state.trust,
                    crate::i18n::text("repo-workflows-devcontainer-devcontainer-window-c1f95bd8"),
                );
                if ui
                    .add_enabled(
                        state.trust && !state.binding.as_ref().is_some_and(|b| b.config != *config),
                        egui::Button::new(crate::i18n::text(
                            "repo-workflows-devcontainer-devcontainer-window-05be654b",
                        )),
                    )
                    .clicked()
                {
                    acts.push(Action::Devcontainer(Act::Start));
                }
                if let Some(binding) = &state.binding {
                    ui.separator();
                    ui.monospace(crate::i18n::message(
                        "repo-workflows-devcontainer-devcontainer-window-5d2a90a9",
                        &[
                            ("binding-id-1", (binding.id).clone().into()),
                            (
                                "binding-engine-context-2",
                                (binding.engine.context).clone().into(),
                            ),
                            ("binding-engine-id-3", (binding.engine.id).clone().into()),
                            ("binding-folder-4", (binding.folder).clone().into()),
                            ("binding-user-5", (binding.user).clone().into()),
                        ],
                    ));
                    ui.label(crate::i18n::message(
                        "repo-workflows-devcontainer-devcontainer-window-473f0412",
                        &[(
                            "model-hash-binding-config-source-1",
                            (model::hash(binding.config.source.as_bytes()))
                                .clone()
                                .into(),
                        )],
                    ));
                    if binding.config != *config {
                        ui.label(crate::i18n::text(
                            "repo-workflows-devcontainer-devcontainer-window-6e69284d",
                        ));
                    }
                    ui.label(crate::i18n::text(
                        "repo-workflows-devcontainer-devcontainer-window-dfcb3d90",
                    ));
                    ui.horizontal(|ui| {
                        if ui
                            .button(crate::i18n::text(
                                "repo-workflows-devcontainer-devcontainer-window-2d5a4455",
                            ))
                            .clicked()
                        {
                            acts.push(Action::Devcontainer(Act::Terminal));
                        }
                        if ui
                            .button(crate::i18n::text(
                                "repo-workflows-devcontainer-devcontainer-window-4227dbe9",
                            ))
                            .clicked()
                        {
                            acts.push(Action::Devcontainer(Act::Editor));
                        }
                    });
                    ui.checkbox(
                        &mut state.rebuild,
                        crate::i18n::text(
                            "repo-workflows-devcontainer-devcontainer-window-8f14be4f",
                        ),
                    );
                    if ui
                        .add_enabled(
                            state.trust && state.rebuild,
                            egui::Button::new(crate::i18n::text(
                                "repo-workflows-devcontainer-devcontainer-window-20441b4a",
                            )),
                        )
                        .clicked()
                    {
                        acts.push(Action::Devcontainer(Act::Rebuild));
                    }
                }
            });
        });
        self.devcontainer.open = open;
    }
}
