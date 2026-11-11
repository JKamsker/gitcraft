//! b4 owns mail transport and patch-series conventions. All Git/b4/file work is queued.
mod model;
mod run;
#[cfg(test)]
mod tests;
mod view;
use super::{Action, RepoTab};
use crate::{connections, tasks};
use std::sync::{Arc, mpsc};

pub(crate) enum Act {
    Open,
    Read,
    Receive,
    Apply,
    Enroll,
    Cover,
    Revision,
    Reroll,
    Preview,
    Send,
    Cancel,
}
enum Reply {
    Read(run::Snapshot, Result<String, String>, String),
    Received(Arc<run::Received>),
    Preview(run::Preview),
    Done(String),
}
struct Pending {
    rx: mpsc::Receiver<Result<Reply, String>>,
    scope: tasks::Scope,
    context: connections::Context,
}
#[derive(Default)]
pub(super) struct State {
    pub(super) open: bool,
    source: String,
    receive_revision: u32,
    forkpoint: String,
    revision: String,
    previous: String,
    cover: String,
    info: String,
    info_failure: Option<(String, String)>,
    to: String,
    cc: String,
    trust: bool,
    confirm: bool,
    snapshot: Option<run::Snapshot>,
    received: Option<Arc<run::Received>>,
    preview: Option<run::Preview>,
    pending: Option<Pending>,
    result: String,
    result_presentation: Option<(String, LocalResult)>,
}
impl RepoTab {
    pub(super) fn email_act(&mut self, act: Act, ctx: &egui::Context) {
        if matches!(act, Act::Open) {
            self.email.open = true;
            return;
        }
        if matches!(act, Act::Cancel) {
            if let Some(p) = &self.email.pending {
                p.scope.0.cancel();
            }
            return;
        }
        if self.email.pending.is_some() || self.busy_op.is_some() {
            return;
        }
        let state = &mut self.email;
        let path = self.path.clone();
        let work: Box<dyn FnOnce() -> Result<Reply, String> + Send> = match act {
            Act::Read => Box::new(move || {
                let snapshot = run::snapshot(&path)?;
                // An unenrolled branch is still readable so it can be enrolled from this UI.
                let info = run::b4(&path, &["prep".into(), "--show-info".into()]);
                let cover =
                    run::b4(&path, &["prep".into(), "--show-cover".into()]).unwrap_or_default();
                Ok(Reply::Read(snapshot, info, cover))
            }),
            Act::Receive => {
                let source = state.source.clone();
                let revision = state.receive_revision;
                Box::new(move || {
                    run::receive(&path, &source, revision).map(|r| Reply::Received(Arc::new(r)))
                })
            }
            Act::Apply => {
                let Some(received) = state.received.clone() else {
                    return;
                };
                Box::new(move || run::apply(&path, &received).map(Reply::Done))
            }
            Act::Preview => {
                let args = model::recipients(&state.to, &state.cc);
                Box::new(move || run::preview(&path, args?).map(Reply::Preview))
            }
            Act::Send => {
                if !state.confirm {
                    return;
                }
                let Some(preview) = state.preview.clone() else {
                    return;
                };
                if model::recipients(&state.to, &state.cc).ok().as_ref()
                    != Some(&preview.recipients)
                {
                    state.remember_result(LocalResult::RecipientsChanged);
                    return;
                }
                state.confirm = false;
                Box::new(move || run::send(&path, &preview).map(Reply::Done))
            }
            Act::Enroll | Act::Cover | Act::Revision | Act::Reroll => {
                if !state.trust {
                    return;
                }
                let Some(snapshot) = state.snapshot.clone() else {
                    return;
                };
                let (mode, value) = match act {
                    Act::Enroll => ("enroll", state.forkpoint.clone()),
                    Act::Cover => ("cover", state.cover.clone()),
                    Act::Revision => ("revision", state.revision.clone()),
                    _ => ("reroll", state.previous.clone()),
                };
                state.trust = false;
                Box::new(move || run::prepare(&path, &snapshot, mode, &value).map(Reply::Done))
            }
            Act::Open | Act::Cancel => return,
        };
        let scope = tasks::Scope(self._jobs.0.child());
        let (tx, rx) = mpsc::channel();
        let cancelled = tx.clone();
        let repaint = ctx.clone();
        let failure_repaint = ctx.clone();
        let submitted = tasks::pool().and_then(|pool| {
            pool.submit_cancellable(
                scope.0.clone(),
                tasks::Priority::Interactive,
                Some(super::messages::mutation_key(&self.path)),
                Some(Box::new(move || {
                    let _ = cancelled.send(Err("Cancelled before starting".into()));
                    failure_repaint.request_repaint();
                })),
                Box::new(move || {
                    let _ = tx.send(work());
                    repaint.request_repaint();
                }),
            )
            .map_err(str::to_owned)
        });
        match submitted {
            Ok(()) => {
                self.busy_op = Some("Email patches · b4".into());
                state.pending = Some(Pending {
                    rx,
                    scope,
                    context: connections::current(),
                });
                state.remember_result(LocalResult::Running);
            }
            Err(e) => {
                state.result_presentation = None;
                state.result = e;
            }
        }
    }
}
#[derive(Clone)]
enum LocalResult {
    Failed { detail: String },
    ProfileChanged,
    Prepared,
    Received,
    Preview,
    WorkerEnded,
    Running,
    RecipientsChanged,
}
impl LocalResult {
    fn raw(&self) -> String {
        match self {
            Self::Failed { detail } => format!("{detail}\nIf cancelled or partially completed, inspect Git/b4/mail state before retrying. Apply conflicts use the existing Continue/Abort operation controls."),
Self::ProfileChanged => "Profile changed. Operation cancelled; inspect repository and mail state before retrying.".into(),
Self::Prepared => "Prepared branch read".into(),
Self::Received => "Series fetched; review the cover, patches and trailers before applying".into(),
Self::Preview => "Preview generated without sending mail".into(),
Self::WorkerEnded => "Worker ended or was cancelled. Inspect repository/mail state before retrying; mail may have been delivered.".into(),
Self::Running => "Working…".into(),
Self::RecipientsChanged => "Recipients changed; generate a new preview".into(),
}
    }
    fn display(&self) -> String {
        let id = match self {
            Self::Failed { detail } => {
                return crate::i18n::message(
                    "repo-workflows-email-email-window-632693ae",
                    &[("error", detail.as_str().into())],
                );
            }
            Self::ProfileChanged => "repo-workflows-email-email-window-8c1d9c9f",
            Self::Prepared => "repo-workflows-email-email-window-b8c5cd6e",
            Self::Received => "repo-workflows-email-email-window-624d6ba0",
            Self::Preview => "repo-workflows-email-email-window-2a42475c",
            Self::WorkerEnded => "repo-workflows-email-email-window-c6755006",
            Self::Running => "repo-workflows-email-email-act-5474eef8",
            Self::RecipientsChanged => "repo-workflows-email-email-act-3eece223",
        };
        crate::i18n::text(id).into()
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
        ui.label(self.display_result());
    }
}

#[cfg(test)]
mod fluent_r1_result_tests;

#[cfg(test)]
mod fluent_r1_failure_tests;
