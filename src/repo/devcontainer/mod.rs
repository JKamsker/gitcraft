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
                        state.result = "Profile changed or operation cancelled. A created container may remain; inspect its displayed worktree labels before retrying.".into();
                    } else {
                        match reply {
                            Ok(Reply::Config(config)) => {
                                if config.is_some() {
                                    state.open = true;
                                }
                                state.config = config;
                                state.result = if state.config.is_some() { "Configuration detected; review it before starting" } else { "No .devcontainer/devcontainer.json detected. Remote worktrees require the CLI on their own machine; this integration supports local worktrees." }.into();
                            }
                            Ok(Reply::Started(binding)) => {
                                state.result =
                                    "Environment started and its worktree mount/ownership checked"
                                        .into();
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
                                    state.result =
                                        "Opened /bin/sh inside the displayed container".into();
                                } else {
                                    state.result =
                                        "Terminal handoff expired or was cancelled; open it again"
                                            .into();
                                }
                            }
                            Err(error) => {
                                state.result = format!(
                                    "{error}\nStart/rebuild may have partly completed. Check Docker availability, installed Dev Container CLI, build files and lifecycle-command errors above. No global cleanup is performed; inspect this worktree's labels before retrying."
                                )
                            }
                        }
                    }
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    if pending.mutation {
                        self.busy_op = None;
                    }
                    state.pending = None;
                    state.result = "Worker ended or was cancelled. A container may remain; inspect this worktree's labels before retrying.".into();
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
        egui::Window::new("Worktree environment · Dev Containers").id(egui::Id::new(("devcontainer", &self.path))).open(&mut open).default_width(730.0).vscroll(true).show(ctx, |ui| {
            ui.label("Uses installed Docker and the Dev Container CLI on this machine. Linux containers with an exact worktree bind mount are supported. Linked Git worktrees require standard relative POSIX metadata and the native common-directory mount. Compose or combined custom workspaceFolder/workspaceMount linked layouts are refused; metadata is never rewritten. Containers persist after closing this window.");
            if state.pending.is_some() { ui.add(crate::spinner::Spinner::new()); if ui.button("Cancel operation").clicked() { acts.push(Action::Devcontainer(Act::Cancel)); } }
            ui.add(egui::Label::new(&state.result).selectable(true));
            ui.add_enabled_ui(state.pending.is_none() && self.busy_op.is_none(), |ui| {
                if ui.button("Read worktree configuration again").clicked() { acts.push(Action::Devcontainer(Act::Read)); }
                let Some(config) = &state.config else { return; };
                ui.monospace(format!("Worktree: {}\nOwnership: {}\nConfiguration SHA256: {}", config.path.display(), config.label(), model::hash(config.source.as_bytes())));
                egui::CollapsingHeader::new("devcontainer.json (JSON with comments supported by native CLI)").show(ui, |ui| { ui.add(egui::Label::new(&config.source).selectable(true)); });
                ui.label("Starting runs this repository's Docker/build/features and lifecycle commands. Review the repository and its referenced files first. Rebuilding deletes only the displayed owned container; unsaved container-only files are lost.");
                ui.checkbox(&mut state.trust, "I trust this worktree's container configuration and lifecycle commands");
                if ui.add_enabled(state.trust && !state.binding.as_ref().is_some_and(|b| b.config != *config), egui::Button::new("Start environment")).clicked() { acts.push(Action::Devcontainer(Act::Start)); }
                if let Some(binding) = &state.binding {
                    ui.separator(); ui.monospace(format!("Container: {}\nDocker context: {}\nDaemon: {}\nWorkspace: {}\nUser: {}", binding.id, binding.engine.context, binding.engine.id, binding.folder, binding.user));
                    ui.label(format!("Container configuration SHA256: {}", model::hash(binding.config.source.as_bytes())));
                    if binding.config != *config { ui.label("Configuration changed: rebuild the displayed container before terminal/editor use. Containers from older app sessions/configurations can remain in Docker."); }
                    ui.label("Terminal uses Docker exec with this native remote user and /bin/sh; editor requires VS Code with its Dev Containers extension.");
                    ui.horizontal(|ui| { if ui.button("Terminal inside container").clicked() { acts.push(Action::Devcontainer(Act::Terminal)); }
if ui.button("Open container in VS Code").clicked() { acts.push(Action::Devcontainer(Act::Editor)); } });
                    ui.checkbox(&mut state.rebuild, "Delete this container and rebuild (container-only files will be lost)");
                    if ui.add_enabled(state.trust && state.rebuild, egui::Button::new("Rebuild displayed environment")).clicked() { acts.push(Action::Devcontainer(Act::Rebuild)); }
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
                state.result = "Working…".into();
            }
            Err(error) => state.result = error,
        }
    }
}
