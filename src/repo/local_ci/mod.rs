//! Local runs retain their own identity and output beside hosted CI. They never masquerade
//! as a forge run, and never rerun or publish a hosted check.
mod context;
mod model;
mod run;
#[cfg(test)]
mod tests;
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
    Catalog(Result<(Vec<Workflow>, Vec<String>), String>),
    Finished(Result<String, String>),
}
struct Pending {
    reply: mpsc::Receiver<Reply>,
    scope: tasks::Scope,
    context: connections::Context,
}
pub(super) struct State {
    workflows: Vec<Workflow>,
    errors: Vec<String>,
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
            self.result=Some(Err("Profile changed; the local operation was cancelled. An act run's Docker resources may remain; inspect them before retrying.".into()));
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
                        Err(e) => self.result = Some(Err(e)),
                    },
                    Reply::Finished(result) => self.result = Some(result),
                }
            }
            Err(mpsc::TryRecvError::Disconnected) => {
                self.pending = None;
                self.result=Some(Err("Local operation ended without an answer (cancelled or worker failed). Inspect Docker resources before retrying.".into()));
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
            Err(e) => self.result = Some(Err(e)),
        }
    }
    fn ui(&mut self, ui: &mut egui::Ui, acts: &mut Vec<Action>, remotes: &[crate::git::Remote]) {
        self.poll(ui.ctx());
        egui::CollapsingHeader::new("Local Actions · installed act")
            .default_open(true)
            .show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .id_salt("local-actions-controls")
                    .max_height(340.0)
                    .show(ui, |ui| self.controls(ui, acts, remotes));
            });
        ui.separator();
    }

    fn controls(
        &mut self,
        ui: &mut egui::Ui,
        acts: &mut Vec<Action>,
        remotes: &[crate::git::Remote],
    ) {
        ui.label("Run the selected workflow against this worktree's current files. Local output is separate from hosted checks.");
        ui.horizontal(|ui| {
            if ui
                .add_enabled(!self.busy(), egui::Button::new("Read workflows"))
                .clicked()
            {
                acts.push(Action::LocalCi(Act::Load));
            }
            if self.busy() {
                ui.add(crate::spinner::Spinner::new());
                if ui.button("Cancel act process").clicked() {
                    acts.push(Action::LocalCi(Act::Cancel));
                }
            }
        });
        for error in &self.errors {
            ui.colored_label(egui::Color32::YELLOW, error);
        }
        if let Some(result) = &self.result {
            match result {
                Ok(s) => {
                    ui.label(s);
                }
                Err(e) => {
                    ui.colored_label(egui::Color32::LIGHT_RED, e);
                }
            }
        }
        ui.add_enabled_ui(!self.busy(), |ui| self.selection_ui(ui, acts, remotes));
        if let Some(source) = &self.provenance {
            ui.label(source);
        }
        let log = self
            .log
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if !log.text.is_empty() {
            egui::CollapsingHeader::new("Local output")
                .default_open(true)
                .show(ui, |ui| {
                    egui::ScrollArea::vertical()
                        .id_salt("local-act-output")
                        .max_height(240.0)
                        .stick_to_bottom(true)
                        .show_rows(ui, 18.0, log.rows.len(), |ui, rows| {
                            for i in rows {
                                ui.monospace(&log.rows[i]);
                            }
                        });
                    if log.truncated {
                        ui.label(
                            "Display capped at 2 MiB / 32768 rows; additional output omitted.",
                        );
                    }
                });
        }
    }

    fn selection_ui(
        &mut self,
        ui: &mut egui::Ui,
        acts: &mut Vec<Action>,
        remotes: &[crate::git::Remote],
    ) {
        let before = (self.workflow, self.job);
        egui::ComboBox::from_id_salt("local-workflow")
            .selected_text(
                self.workflows
                    .get(self.workflow)
                    .map_or("Choose workflow", |w| w.file.as_str()),
            )
            .show_ui(ui, |ui| {
                for (i, w) in self.workflows.iter().enumerate() {
                    ui.selectable_value(&mut self.workflow, i, &w.file);
                }
            });
        if before.0 != self.workflow {
            self.job = 0;
            self.event = 0;
            self.matrix = 0;
            self.trust = false;
        }
        let Some(workflow) = self.workflows.get(self.workflow) else {
            return;
        };
        egui::ComboBox::from_id_salt("local-event")
            .selected_text(
                workflow
                    .events
                    .get(self.event)
                    .map_or("Choose event", String::as_str),
            )
            .show_ui(ui, |ui| {
                for (i, event) in workflow.events.iter().enumerate() {
                    ui.selectable_value(&mut self.event, i, event);
                }
            });
        egui::ComboBox::from_id_salt("local-job")
            .selected_text(
                workflow
                    .jobs
                    .get(self.job)
                    .map_or("Choose job", String::as_str),
            )
            .show_ui(ui, |ui| {
                for (i, job) in workflow.jobs.iter().enumerate() {
                    ui.selectable_value(&mut self.job, i, job);
                }
            });
        if before.1 != self.job {
            self.matrix = 0;
            self.trust = false;
        }
        let Some(job) = workflow.jobs.get(self.job) else {
            return;
        };
        let entries = match workflow.matrices(job) {
            Ok(entries) => entries,
            Err(e) => {
                ui.colored_label(egui::Color32::YELLOW, e);
                return;
            }
        };
        egui::ComboBox::from_id_salt("local-matrix")
            .selected_text(
                entries
                    .get(self.matrix)
                    .map(text)
                    .unwrap_or_else(|| "No matrix entry".into()),
            )
            .show_ui(ui, |ui| {
                for (i, entry) in entries.iter().enumerate() {
                    ui.selectable_value(&mut self.matrix, i, text(entry));
                }
            });
        match workflow.dependencies(job) {
            Ok(deps) => {
                let needs = deps
                    .iter()
                    .filter(|j| *j != job)
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(", ");
                ui.label(format!("Runs {job} and prerequisites: {needs}. Prerequisite matrices retain all their entries."));
            }
            Err(e) => {
                ui.colored_label(egui::Color32::YELLOW, e);
                return;
            }
        }
        ui.collapsing("Event payload (JSON)", |ui| {
            ui.add(
                egui::TextEdit::multiline(&mut self.payload)
                    .code_editor()
                    .desired_rows(5)
                    .char_limit(model::MAX),
            );
        });
        ui.horizontal(|ui| {
            ui.label("Context remote");
            if ui.text_edit_singleline(&mut self.remote).changed() {
                self.remote_override = true;
                self.trust = false;
            }
        });
        let context = context::Context::capture(&self.remote, remotes);
        if self.displayed_context.as_ref() != context.as_ref().ok() {
            self.trust = false;
            self.displayed_context = context.as_ref().ok().cloned();
        }
        match &context {
            Ok(context) => {
                ui.label(context.display());
            }
            Err(error) => {
                ui.colored_label(egui::Color32::YELLOW, error);
            }
        }
        ui.label("act reads this remote's fetch URL for repository context. If its pushURL differs, review repository fields in the event JSON. Hosted CI retains its push-destination routing.");
        ui.label("Runner mappings · one label=container-image per line");
        ui.add(
            egui::TextEdit::multiline(&mut self.platforms)
                .desired_rows(2)
                .hint_text("ubuntu-latest=your-trusted-runner-image")
                .char_limit(8192),
        );
        ui.horizontal(|ui| {
            ui.label("Architecture (optional)");
            ui.text_edit_singleline(&mut self.architecture);
            ui.label("Timeout minutes");
            ui.add(egui::DragValue::new(&mut self.minutes).range(1..=120));
        });
        ui.collapsing("act limitations and execution scope", |ui| {
            ui.label("Linux containers do not reproduce native macOS/Windows runners. act ignores or incompletely supports concurrency, run-name, permissions, timeout-minutes, continue-on-error, environments/protected environment secrets, OIDC and cancellation inside a run step. GitHub context and event filtering differ. Supply event inputs in JSON; missing secrets, private actions or unsupported reusable workflows may fail.");
            ui.label("The selected job uses a private workflow snapshot with exactly one matrix entry. needs jobs also run. Local files are copied, not bind-mounted. Automatic .env/.secrets/.vars/.input and ambient actrc flags are refused. The Docker socket is not mounted inside the runner. Container options in a trusted workflow still matter. act and Docker must already be installed. No hosted check is written.");
            ui.label("Cancel/timeout stops gitcraft's act child. Docker containers or services may continue; inspect Docker before retrying. No automatic global Docker cleanup is performed.");
            ui.hyperlink_to("act supported features", "https://nektosact.com/not_supported.html");
        });
        ui.collapsing("Selected workflow snapshot", |ui| {
            if let (Some(entry), Some(event)) =
                (entries.get(self.matrix), workflow.events.get(self.event))
            {
                match workflow.snapshot(event, job, entry, "preview") {
                    Ok(source) => {
                        ui.add(
                            egui::Label::new(egui::RichText::new(source).monospace())
                                .selectable(true),
                        );
                    }
                    Err(error) => {
                        ui.label(error);
                    }
                }
            }
        });
        ui.checkbox(
            &mut self.trust,
            "I trust this workflow, its actions and runner images to execute on this machine",
        );
        if ui
            .add_enabled(
                self.trust && entries.get(self.matrix).is_some() && context.is_ok(),
                egui::Button::new("Run selected entry locally"),
            )
            .clicked()
        {
            acts.push(Action::LocalCi(Act::Run));
        }
    }
}
impl RepoTab {
    pub(super) fn local_ci_context_default(&mut self) {
        if self.local_ci.remote_override || self.local_ci.busy() {
            return;
        }
        let remote = self
            .refs
            .current
            .as_ref()
            .and_then(|b| self.refs.push_remotes.get(b))
            .cloned()
            .or_else(|| {
                self.remotes
                    .iter()
                    .find(|r| r.name == "origin")
                    .or_else(|| self.remotes.first())
                    .map(|r| r.name.clone())
            });
        if let Some(remote) = remote {
            if self.local_ci.remote != remote {
                self.local_ci.trust = false;
            }
            self.local_ci.remote = remote;
        }
    }

    pub(super) fn local_ci_act(&mut self, act: Act, ctx: &egui::Context) {
        if matches!(act, Act::Open) {
            self.open_ci(self.refs.head.clone(), None, ctx);
            if self.ci_view.is_some() {
                self.doctor = None;
                self.bakeoff.view = None;
            }
            return;
        }
        if matches!(act, Act::Cancel) {
            if let Some(pending) = &self.local_ci.pending {
                pending.scope.0.cancel();
            }
            return;
        }
        if self.local_ci.busy() {
            return;
        }
        match act {
            Act::Load => {
                let path = self.path.clone();
                self.local_ci.submit(&self._jobs.0, ctx, None, move || {
                    Reply::Catalog(run::read_catalog(&path))
                });
            }
            Act::Run => {
                let context = match context::Context::capture(&self.local_ci.remote, &self.remotes)
                {
                    Ok(context) => context,
                    Err(error) => {
                        self.local_ci.result = Some(Err(error));
                        return;
                    }
                };
                let state = &mut self.local_ci;
                if state.displayed_context.as_ref() != Some(&context) {
                    state.trust = false;
                    state.result = Some(Err(
                        "Repository context changed since display; review it before running".into(),
                    ));
                    return;
                }
                if !state.trust {
                    return;
                }
                let Some(workflow) = state.workflows.get(state.workflow).cloned() else {
                    return;
                };
                let Some(event) = workflow.events.get(state.event).cloned() else {
                    return;
                };
                let Some(job) = workflow.jobs.get(state.job).cloned() else {
                    return;
                };
                let matrix = match workflow.matrices(&job).and_then(|e| {
                    e.get(state.matrix)
                        .cloned()
                        .ok_or("No selected matrix entry".into())
                }) {
                    Ok(m) => m,
                    Err(e) => {
                        state.result = Some(Err(e));
                        return;
                    }
                };
                state.provenance = Some(format!(
                    "LOCAL · {} · {event} · {job} · {} · worktree {} · {}",
                    workflow.file,
                    text(&matrix),
                    crate::remote::display(&self.path),
                    context.display()
                ));
                let selection = run::Selection {
                    workflow,
                    context,
                    event,
                    job,
                    matrix,
                    payload: state.payload.clone(),
                    platforms: state.platforms.clone(),
                    architecture: state.architecture.clone(),
                    minutes: state.minutes,
                };
                state.log = Arc::default();
                let log = Arc::clone(&state.log);
                let path = self.path.clone();
                let repaint = ctx.clone();
                let key = super::messages::mutation_key(&path);
                state.submit(&self._jobs.0, ctx, Some(key), move || {
                    Reply::Finished(run::execute(path, selection, log, repaint))
                });
            }
            Act::Open | Act::Cancel => {}
        }
    }
    pub(super) fn local_ci_ui(&mut self, ui: &mut egui::Ui, acts: &mut Vec<Action>) {
        self.local_ci_context_default();
        self.local_ci.ui(ui, acts, &self.remotes);
    }
}
