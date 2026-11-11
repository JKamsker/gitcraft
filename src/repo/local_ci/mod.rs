//! Local runs retain their own identity and output beside hosted CI. They never masquerade
//! as a forge run, and never rerun or publish a hosted check.
mod actions;
mod context;
mod model;
mod run;
#[cfg(test)]
mod tests;
mod view;
use super::{Action, RepoTab};
use crate::{connections, tasks};
use model::{Workflow, text};
use std::sync::{Arc, Mutex, mpsc};

pub(crate) enum Act {
    Open,
    Load,
    Run,
    Cancel,
}
enum Reply {
    Catalog(Result<(Vec<Workflow>, Vec<run::CatalogFailure>), run::CatalogFailure>),
    Finished(Result<String, String>),
}
struct Pending {
    reply: mpsc::Receiver<Reply>,
    scope: tasks::Scope,
    context: connections::Context,
}
pub(super) struct State {
    workflows: Vec<Workflow>,
    errors: Vec<run::CatalogFailure>,
    workflow: usize,
    event: usize,
    job: usize,
    matrix: usize,
    remote: String,
    remote_override: bool,
    displayed_context: Option<context::Context>,
    payload: String,
    platforms: String,
    architecture: String,
    minutes: u64,
    trust: bool,
    pending: Option<Pending>,
    result: Option<Result<String, String>>,
    result_presentation: Option<(String, ResultFailure)>,
    log: Arc<Mutex<run::Log>>,
    provenance: Option<String>,
}
impl Default for State {
    fn default() -> Self {
        Self {
            workflows: Vec::new(),
            errors: Vec::new(),
            workflow: 0,
            event: 0,
            job: 0,
            matrix: 0,
            remote: "origin".into(),
            remote_override: false,
            displayed_context: None,
            payload: "{}".into(),
            platforms: String::new(),
            architecture: String::new(),
            minutes: 30,
            trust: false,
            pending: None,
            result: None,
            result_presentation: None,
            log: Arc::default(),
            provenance: None,
        }
    }
}
impl State {
    pub fn busy(&self) -> bool {
        self.pending.is_some()
    }
    pub fn poll(&mut self, ctx: &egui::Context) {
        let Some(pending) = &self.pending else {
            return;
        };
        if pending.context != connections::current() {
            self.pending = None;
            self.remember_catalog_failure(run::CatalogFailure::Stale);
            return;
        }
        match pending.reply.try_recv() {
            Ok(reply) => {
                self.pending = None;
                match reply {
                    Reply::Catalog(result) => match result {
                        Ok((workflows, errors)) => {
                            self.workflows = workflows;
                            self.errors = errors;
                            self.workflow = 0;
                            self.event = 0;
                            self.job = 0;
                            self.matrix = 0;
                            self.trust = false;
                            self.result = None;
                        }
                        Err(e) => self.remember_catalog_failure(e),
                    },
                    Reply::Finished(result) => {
                        self.result_presentation = None;
                        self.result = Some(result);
                    }
                }
            }
            Err(mpsc::TryRecvError::Disconnected) => {
                self.pending = None;
                self.remember_catalog_failure(run::CatalogFailure::Ended);
            }
            Err(mpsc::TryRecvError::Empty) => {
                ctx.request_repaint_after(std::time::Duration::from_millis(100));
            }
        }
    }
    fn submit(
        &mut self,
        parent: &tasks::Cancellation,
        ctx: &egui::Context,
        key: Option<std::path::PathBuf>,
        work: impl FnOnce() -> Reply + Send + 'static,
    ) {
        let scope = tasks::Scope(parent.child());
        let (tx, rx) = mpsc::channel();
        let cancelled = tx.clone();
        let repaint = ctx.clone();
        let fail_repaint = ctx.clone();
        let result = tasks::pool().and_then(|pool| {
            pool.submit_cancellable(
                scope.0.clone(),
                tasks::Priority::Background,
                key,
                Some(Box::new(move || {
                    let _ =
                        cancelled.send(Reply::Finished(Err("Cancelled before starting".into())));
                    fail_repaint.request_repaint();
                })),
                Box::new(move || {
                    let _ = tx.send(work());
                    repaint.request_repaint();
                }),
            )
            .map_err(str::to_owned)
        });
        match result {
            Ok(()) => {
                self.result = None;
                self.pending = Some(Pending {
                    reply: rx,
                    scope,
                    context: connections::current(),
                });
            }
            Err(e) => {
                self.result_presentation = None;
                self.result = Some(Err(e));
            }
        }
    }
}

enum ResultFailure {
    Catalog(run::CatalogFailure),
    ContextChanged,
    MissingMatrix,
}
impl ResultFailure {
    fn raw(&self) -> String {
        match self {
            Self::Catalog(failure) => failure.raw(),
            Self::ContextChanged => {
                "Repository context changed since display; review it before running".into()
            }
            Self::MissingMatrix => "No selected matrix entry".into(),
        }
    }
    fn display(&self) -> String {
        match self {
            Self::Catalog(failure) => failure.display(),
            Self::ContextChanged => {
                crate::i18n::text("repo-workflows-local_ci-local-ci-act-933801f0").into()
            }
            Self::MissingMatrix => {
                crate::i18n::text("repo-workflows-local_ci-local-ci-act-3feaaea5").into()
            }
        }
    }
}

impl State {
    fn remember_catalog_failure(&mut self, failure: run::CatalogFailure) {
        self.remember_failure(ResultFailure::Catalog(failure));
    }
    fn remember_failure(&mut self, failure: ResultFailure) {
        let raw = failure.raw();
        self.result = Some(Err(raw.clone()));
        self.result_presentation = Some((raw, failure));
    }
    fn run_context_matches(&mut self, context: &context::Context) -> bool {
        if self.displayed_context.as_ref() == Some(context) {
            return true;
        }
        self.trust = false;
        self.remember_failure(ResultFailure::ContextChanged);
        false
    }
    fn run_matrix(&mut self, entries: &[yaml_rust2::Yaml]) -> Option<yaml_rust2::Yaml> {
        let entry = entries.get(self.matrix).cloned();
        if entry.is_none() {
            self.remember_failure(ResultFailure::MissingMatrix);
        }
        entry
    }
    fn display_result_error(&self, raw: &str) -> String {
        match &self.result_presentation {
            Some((old, failure)) if old == raw => failure.display(),
            _ => crate::repo::activity::display_workflow_error(raw),
        }
    }
}

#[cfg(test)]
mod fluent_r2_guard_tests;
