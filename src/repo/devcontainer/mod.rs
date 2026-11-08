//! Per-worktree environments. Workers own CLI/PTY starts; UI owns selections and confirmation.
mod model;
mod run;
#[cfg(test)]
mod tests;
use super::{Action, RepoTab};
use crate::{connections, tasks};
use std::sync::{Arc, Mutex, mpsc};

pub(crate) enum Act {
    Open,
    Read,
    Start,
    Rebuild,
    Terminal,
    Editor,
    Cancel,
}
enum Reply {
    Config(Option<model::Config>),
    Started(model::Binding),
    Terminal(Ready),
    Editor(String),
}
struct Pending {
    rx: mpsc::Receiver<Result<Reply, String>>,
    scope: tasks::Scope,
    context: connections::Context,
    mutation: bool,
}
#[derive(Default)]
pub(super) struct State {
    pub(super) open: bool,
    checked: bool,
    config: Option<model::Config>,
    binding: Option<model::Binding>,
    trust: bool,
    rebuild: bool,
    pending: Option<Pending>,
    result: String,
    result_presentation: Option<(String, LocalResult)>,
}
/// The worker retains the terminal until UI acknowledgement. Stale/expired results are
/// disposed by that worker, so a rejected callback never kills/waits for a child on egui.
struct Ready {
    slot: Arc<Mutex<Option<crate::terminal::Terminal>>>,
    _ack: mpsc::Sender<()>,
}
fn offer(
    terminal: crate::terminal::Terminal,
    tx: mpsc::Sender<Result<Reply, String>>,
    ctx: egui::Context,
) {
    let slot = Arc::new(Mutex::new(Some(terminal)));
    let (ack, rx) = mpsc::channel();
    let _ = tx.send(Ok(Reply::Terminal(Ready {
        slot: Arc::clone(&slot),
        _ack: ack,
    })));
    ctx.request_repaint();
    for _ in 0..600 {
        if tasks::cancellation().is_cancelled() {
            break;
        }
        match rx.recv_timeout(std::time::Duration::from_millis(50)) {
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            _ => break,
        }
    }
    // If UI accepted it, the slot is empty. Otherwise owned child disposal stays here.
    drop(
        slot.lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take(),
    );
}
enum Request {
    Detect(std::path::PathBuf),
    Start(model::Config, Option<model::Binding>),
    Terminal(model::Config, model::Binding),
    Editor(model::Config, model::Binding),
}
impl RepoTab {
    pub(super) fn devcontainer_window(&mut self, ctx: &egui::Context, acts: &mut Vec<Action>) {
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
                                    self.terminals.push(term);
                                    self.active_terminal = self.terminals.len() - 1;
                                    self.terminal_error = None;
                                    self.terminal_open = true;
                                    state.remember_result(LocalResult::TerminalOpened);
                                } else {
                                    state.remember_result(LocalResult::TerminalExpired);
                                }
                            }
                            Err(error) => state.result = error,
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
    pub(super) fn devcontainer_act(&mut self, act: Act, ctx: &egui::Context) {
        if matches!(act, Act::Open) {
            self.devcontainer.open = true;
            if self.devcontainer.pending.is_none() {
                self.devcontainer.checked = false;
            }
            return;
        }
        if matches!(act, Act::Cancel) {
            if let Some(p) = &self.devcontainer.pending {
                p.scope.0.cancel();
            }
            return;
        }
        let state = &mut self.devcontainer;
        if state.pending.is_some() || (!matches!(act, Act::Read) && self.busy_op.is_some()) {
            return;
        }
        let mutation = !matches!(act, Act::Read);
        let request = match act {
            Act::Read => {
                state.checked = true;
                Request::Detect(self.path.clone())
            }
            Act::Start | Act::Rebuild => {
                if !state.trust || (matches!(act, Act::Rebuild) && !state.rebuild) {
                    return;
                }
                let Some(config) = state.config.clone() else {
                    return;
                };
                let old = if matches!(act, Act::Rebuild) {
                    let Some(old) = state.binding.clone() else {
                        return;
                    };
                    Some(old)
                } else {
                    None
                };
                Request::Start(config, old)
            }
            Act::Terminal | Act::Editor => {
                let (Some(config), Some(binding)) = (state.config.clone(), state.binding.clone())
                else {
                    return;
                };
                if matches!(act, Act::Terminal) {
                    Request::Terminal(config, binding)
                } else {
                    Request::Editor(config, binding)
                }
            }
            Act::Open | Act::Cancel => return,
        };
        state.trust = false;
        state.rebuild = false;
        let scope = tasks::Scope(self._jobs.0.child());
        let (tx, rx) = mpsc::channel();
        let failure = tx.clone();
        let repaint = ctx.clone();
        let fail_repaint = ctx.clone();
        let key = mutation.then(|| super::messages::mutation_key(&self.path));
        let submitted = tasks::pool().and_then(|pool| {
            pool.submit_cancellable(
                scope.0.clone(),
                tasks::Priority::Interactive,
                key,
                Some(Box::new(move || {
                    let _ = failure.send(Err("Cancelled before starting".into()));
                    fail_repaint.request_repaint();
                })),
                Box::new(move || {
                    let reply = match request {
                        Request::Detect(path) => run::detect(&path).map(Reply::Config),
                        Request::Start(config, old) => {
                            run::start(&config, old.as_ref()).map(Reply::Started)
                        }
                        Request::Editor(config, binding) => {
                            run::editor(&binding, &config).map(Reply::Editor)
                        }
                        Request::Terminal(config, binding) => {
                            match run::terminal(&binding, &config, &repaint) {
                                Ok(term) => {
                                    offer(term, tx, repaint);
                                    return;
                                }
                                Err(e) => Err(e),
                            }
                        }
                    };
                    let _ = tx.send(reply);
                    repaint.request_repaint();
                }),
            )
            .map_err(str::to_owned)
        });
        match submitted {
            Ok(()) => {
                if mutation {
                    self.busy_op = Some("Dev container operation".into());
                }
                state.pending = Some(Pending {
                    rx,
                    scope,
                    context: connections::current(),
                    mutation,
                });
                state.remember_result(LocalResult::Running);
            }
            Err(error) => {
                state.result_presentation = None;
                state.result = error;
            }
        }
    }
}

#[derive(Clone)]
enum LocalResult {
    ProfileChanged,
    Detected,
    Missing,
    Started,
    TerminalOpened,
    TerminalExpired,
    WorkerEnded,
    Running,
}
impl LocalResult {
    fn raw(&self) -> String {
        match self {
Self::ProfileChanged => "Profile changed or operation cancelled. A created container may remain; inspect its displayed worktree labels before retrying.".into(),
Self::Detected => "Configuration detected; review it before starting".into(),
Self::Missing => "No .devcontainer/devcontainer.json detected. Remote worktrees require the CLI on their own machine; this integration supports local worktrees.".into(),
Self::Started => "Environment started and its worktree mount/ownership checked".into(),
Self::TerminalOpened => "Opened /bin/sh inside the displayed container".into(),
Self::TerminalExpired => "Terminal handoff expired or was cancelled; open it again".into(),
Self::WorkerEnded => "Worker ended or was cancelled. A container may remain; inspect this worktree's labels before retrying.".into(),
Self::Running => "Working…".into(),
}
    }
    fn display(&self) -> String {
        crate::i18n::text(match self {
            Self::ProfileChanged => "repo-workflows-devcontainer-devcontainer-window-7cb7e0b2",
            Self::Detected => "repo-workflows-devcontainer-devcontainer-window-a79d7c99",
            Self::Missing => "repo-workflows-devcontainer-devcontainer-window-22f9e7ca",
            Self::Started => "repo-workflows-devcontainer-devcontainer-window-d66d64d1",
            Self::TerminalOpened => "repo-workflows-devcontainer-devcontainer-window-bbd55e3e",
            Self::TerminalExpired => "repo-workflows-devcontainer-devcontainer-window-4564ef3d",
            Self::WorkerEnded => "repo-workflows-devcontainer-devcontainer-window-0c79d376",
            Self::Running => "repo-workflows-devcontainer-devcontainer-act-5474eef8",
        })
        .into()
    }
}

impl State {
    fn remember_result(&mut self, note: LocalResult) {
        let raw = note.raw();
        self.result = raw.clone();
        self.result_presentation = Some((raw, note));
    }
    fn display_result(&self) -> String {
        match &self.result_presentation {
            Some((raw, note)) if raw == &self.result => note.display(),
            _ => crate::repo::activity::display_workflow_error(&self.result),
        }
    }
}

impl State {
    fn result_ui(&self, ui: &mut egui::Ui) {
        ui.add(egui::Label::new(self.display_result()).selectable(true));
    }
}

#[cfg(test)]
mod fluent_r1_result_tests {
    use super::*;
    use egui_kittest::{Harness, kittest::Queryable};
    #[test]
    fn fluent_partition_r1_devcontainer_same_retained_state_repaints_without_work_or_consent_change()
     {
        for note in [
            LocalResult::Detected,
            LocalResult::Missing,
            LocalResult::Started,
            LocalResult::TerminalOpened,
            LocalResult::TerminalExpired,
            LocalResult::ProfileChanged,
            LocalResult::WorkerEnded,
            LocalResult::Running,
        ] {
            crate::i18n::set_language("de");
            let mut state = State::default();
            state.trust = true;
            state.remember_result(note);
            let raw = state.result.clone();
            let mut h = Harness::new_ui_state(|ui, state: &mut State| state.result_ui(ui), state);
            for lang in ["de", "en-US", "de"] {
                crate::i18n::set_language(lang);
                h.run();
                h.get_by_label(&h.state().display_result());
                assert_eq!(h.state().display_result() == raw, lang == "en-US");
                assert_eq!(h.state().result, raw);
                assert!(h.state().trust);
                assert!(h.state().pending.is_none());
            }
            h.state_mut().result_presentation = None;
            for lang in ["de", "en-US"] {
                crate::i18n::set_language(lang);
                assert_eq!(h.state().display_result(), raw);
            }
        }
        crate::i18n::set_language("en-US");
    }
}
