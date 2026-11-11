//! Per-worktree environments. Workers own CLI/PTY starts; UI owns selections and confirmation.
mod model;
mod run;
#[cfg(test)]
mod tests;
mod view;
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
    Failed { detail: String },
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
Self::Failed { detail } => format!("{detail}\nStart/rebuild may have partly completed. Check Docker availability, installed Dev Container CLI, build files and lifecycle-command errors above. No global cleanup is performed; inspect this worktree's labels before retrying."),
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
            Self::Failed { detail } => {
                return crate::i18n::message(
                    "repo-workflows-devcontainer-devcontainer-window-d8db68fe",
                    &[("error", detail.clone().into())],
                );
            }
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
mod fluent_r1_result_tests;

#[cfg(test)]
mod fluent_r2_failure_tests;
