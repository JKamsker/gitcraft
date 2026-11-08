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
    fn ui(&mut self, ui: &mut egui::Ui, acts: &mut Vec<Action>, remotes: &[crate::git::Remote]) {
        self.poll(ui.ctx());
        egui::CollapsingHeader::new(crate::i18n::text("repo-workflows-local_ci-ui-a38baafc"))
            .id_salt("local-actions-controls")
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
        ui.label(crate::i18n::text(
            "repo-workflows-local_ci-controls-3e8197fb",
        ));
        ui.horizontal(|ui| {
            if ui
                .add_enabled(
                    !self.busy(),
                    egui::Button::new(crate::i18n::text(
                        "repo-workflows-local_ci-controls-683b3dce",
                    )),
                )
                .clicked()
            {
                acts.push(Action::LocalCi(Act::Load));
            }
            if self.busy() {
                ui.add(crate::spinner::Spinner::new());
                if ui
                    .button(crate::i18n::text(
                        "repo-workflows-local_ci-controls-8bc74e1f",
                    ))
                    .clicked()
                {
                    acts.push(Action::LocalCi(Act::Cancel));
                }
            }
        });
        for error in &self.errors {
            ui.colored_label(egui::Color32::YELLOW, error.display());
        }
        if let Some(result) = &self.result {
            match result {
                Ok(s) => {
                    ui.label(s);
                }
                Err(e) => {
                    ui.colored_label(egui::Color32::LIGHT_RED, self.display_result_error(e));
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
            egui::CollapsingHeader::new(crate::i18n::text(
                "repo-workflows-local_ci-controls-b19c65b7",
            ))
            .id_salt("local-actions-output")
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
                    ui.label(crate::i18n::text(
                        "repo-workflows-local_ci-controls-a5c8e0e7",
                    ));
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
            .selected_text(self.workflows.get(self.workflow).map_or(
                crate::i18n::text("repo-workflows-local_ci-selection-ui-6d053cdd"),
                |w| w.file.as_str(),
            ))
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
            .selected_text(workflow.events.get(self.event).map_or(
                crate::i18n::text("repo-workflows-local_ci-selection-ui-6252e493"),
                String::as_str,
            ))
            .show_ui(ui, |ui| {
                for (i, event) in workflow.events.iter().enumerate() {
                    ui.selectable_value(&mut self.event, i, event);
                }
            });
        egui::ComboBox::from_id_salt("local-job")
            .selected_text(workflow.jobs.get(self.job).map_or(
                crate::i18n::text("repo-workflows-local_ci-selection-ui-f87dca5f"),
                String::as_str,
            ))
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
            .selected_text(entries.get(self.matrix).map(text).unwrap_or_else(|| {
                crate::i18n::text("repo-workflows-local_ci-selection-ui-67076d36").into()
            }))
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
                ui.label(crate::i18n::message(
                    "repo-workflows-local_ci-selection-ui-6c9c931f",
                    &[
                        ("job", (job).clone().into()),
                        ("needs", (needs).clone().into()),
                    ],
                ));
            }
            Err(e) => {
                ui.colored_label(egui::Color32::YELLOW, e);
                return;
            }
        }
        ui.collapsing(
            crate::i18n::text("repo-workflows-local_ci-selection-ui-57d9c265"),
            |ui| {
                ui.add(
                    egui::TextEdit::multiline(&mut self.payload)
                        .code_editor()
                        .desired_rows(5)
                        .char_limit(model::MAX),
                );
            },
        );
        ui.horizontal(|ui| {
            ui.label(crate::i18n::text(
                "repo-workflows-local_ci-selection-ui-4c541172",
            ));
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
        ui.label(crate::i18n::text(
            "repo-workflows-local_ci-selection-ui-d3f86564",
        ));
        ui.label(crate::i18n::text(
            "repo-workflows-local_ci-selection-ui-e9ff68bc",
        ));
        ui.add(
            egui::TextEdit::multiline(&mut self.platforms)
                .desired_rows(2)
                .hint_text("ubuntu-latest=your-trusted-runner-image")
                .char_limit(8192),
        );
        ui.horizontal(|ui| {
            ui.label(crate::i18n::text(
                "repo-workflows-local_ci-selection-ui-2d7d7bb9",
            ));
            ui.text_edit_singleline(&mut self.architecture);
            ui.label(crate::i18n::text(
                "repo-workflows-local_ci-selection-ui-32f7c3c5",
            ));
            ui.add(egui::DragValue::new(&mut self.minutes).range(1..=120));
        });
        ui.collapsing(
            crate::i18n::text("repo-workflows-local_ci-selection-ui-5784e351"),
            |ui| {
                ui.label(crate::i18n::text(
                    "repo-workflows-local_ci-selection-ui-cfd96599",
                ));
                ui.label(crate::i18n::text(
                    "repo-workflows-local_ci-selection-ui-289ae09d",
                ));
                ui.label(crate::i18n::text(
                    "repo-workflows-local_ci-selection-ui-85e1c7fb",
                ));
                ui.hyperlink_to(
                    crate::i18n::text("repo-workflows-local_ci-selection-ui-88017c1f"),
                    "https://nektosact.com/not_supported.html",
                );
            },
        );
        ui.collapsing(
            crate::i18n::text("repo-workflows-local_ci-selection-ui-2c597b18"),
            |ui| {
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
            },
        );
        ui.checkbox(
            &mut self.trust,
            crate::i18n::text("repo-workflows-local_ci-selection-ui-9052f5fc"),
        );
        if ui
            .add_enabled(
                self.trust && entries.get(self.matrix).is_some() && context.is_ok(),
                egui::Button::new(crate::i18n::text(
                    "repo-workflows-local_ci-selection-ui-e07ff1de",
                )),
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
                    Reply::Catalog(run::read_catalog_with_witness(&path))
                });
            }
            Act::Run => {
                let context = match context::Context::capture(&self.local_ci.remote, &self.remotes)
                {
                    Ok(context) => context,
                    Err(error) => {
                        self.local_ci.result_presentation = None;
                        self.local_ci.result = Some(Err(error));
                        return;
                    }
                };
                let state = &mut self.local_ci;
                if !state.run_context_matches(&context) {
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
                let entries = match workflow.matrices(&job) {
                    Ok(entries) => entries,
                    Err(e) => {
                        state.result_presentation = None;
                        state.result = Some(Err(e));
                        return;
                    }
                };
                let Some(matrix) = state.run_matrix(&entries) else {
                    return;
                };
                state.provenance = Some(crate::i18n::message(
                    "repo-workflows-local_ci-local-ci-act-476da961",
                    &[
                        ("workflow-file-1", (workflow.file).clone().into()),
                        ("event", (event).clone().into()),
                        ("job", (job).clone().into()),
                        ("text-matrix-2", (text(&matrix)).clone().into()),
                        (
                            "crate-remote-display-self-path-3",
                            (crate::remote::display(&self.path)).clone().into(),
                        ),
                        ("context-display-4", (context.display()).clone().into()),
                    ],
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
mod fluent_r2_guard_tests {
    use super::*;
    use egui_kittest::{Harness, kittest::Queryable};

    fn remotes() -> Vec<crate::git::Remote> {
        vec![crate::git::Remote {
            name: "fork".into(),
            url: "https://github.com/supplied/opaque-repo.git".into(),
        }]
    }
    fn supplied_state() -> Result<State, String> {
        let mut state = State::default();
        state.workflows = vec![model::parse("opaque.yml".into(), "on: [push]\njobs:\n  opaque-job:\n    runs-on: ubuntu-latest\n    steps: [{run: 'opaque command --unchanged'}]\n".into())?];
        state.remote = "fork".into();
        state.remote_override = true;
        state.displayed_context = Some(context::Context::capture("fork", &remotes())?);
        state.payload = "{\"opaque\":\"Working…\"}".into();
        state.platforms = "ubuntu-latest=supplied/image".into();
        state.architecture = "linux/arm64".into();
        state.minutes = 17;
        state.trust = true;
        state.provenance = Some("opaque prior provenance\nKein Matrixeintrag ausgewählt".into());
        Ok(state)
    }
    fn snapshot(state: &State) -> Result<String, String> {
        let log = state.log.lock().map_err(|e| e.to_string())?;
        Ok(format!(
            "{:?}",
            (
                (
                    &state.workflows[0].file,
                    &state.workflows[0].source,
                    &state.workflows[0].events,
                    &state.workflows[0].jobs
                ),
                (state.workflow, state.event, state.job, state.matrix),
                (
                    &state.remote,
                    state.remote_override,
                    &state.displayed_context
                ),
                (
                    &state.payload,
                    &state.platforms,
                    &state.architecture,
                    state.minutes
                ),
                (state.trust, state.busy(), &state.provenance),
                (Arc::as_ptr(&state.log), &log.text, &log.rows, log.truncated),
            )
        ))
    }
    fn repaint_same_state(state: State, raw: &str, german: &str) -> Result<(), String> {
        let before = snapshot(&state)?;
        let mut h = Harness::new_ui_state(
            |ui, state: &mut State| {
                let mut actions = Vec::new();
                state.controls(ui, &mut actions, &remotes());
                assert!(actions.is_empty());
            },
            state,
        );
        for (lang, expected) in [("en-US", raw), ("de", german), ("en-US", raw)] {
            crate::i18n::set_language(lang);
            h.run();
            h.get_by_label(expected);
            assert_eq!(h.state().display_result_error(raw), expected);
            assert_eq!(h.state().result.as_ref(), Some(&Err(raw.to_owned())));
            assert_eq!(snapshot(h.state())?, before);
            assert!(h.state().pending.is_none());
        }
        Ok(())
    }

    #[test]
    fn actual_context_guard_retains_raw_error_and_repaints_without_restoring_consent()
    -> Result<(), String> {
        for created_in in ["en-US", "de"] {
            crate::i18n::set_language(created_in);
            let mut state = supplied_state()?;
            let old = snapshot(&state)?;
            let displayed = state
                .displayed_context
                .clone()
                .ok_or("missing supplied context")?;
            assert!(state.run_context_matches(&displayed));
            assert_eq!(snapshot(&state)?, old);
            assert!(state.result.is_none());
            let changed = context::Context::capture(
                "fork",
                &[crate::git::Remote {
                    name: "fork".into(),
                    url: "https://github.com/replacement/repo.git".into(),
                }],
            )?;
            assert!(!state.run_context_matches(&changed));
            assert!(!state.trust);
            assert_eq!(state.displayed_context.as_ref(), Some(&displayed));
            repaint_same_state(
                state,
                "Repository context changed since display; review it before running",
                "Der Repository-Kontext hat sich seit der Anzeige geändert. Vor dem Start erneut prüfen.",
            )?;
        }
        crate::i18n::set_language("en-US");
        Ok(())
    }

    #[test]
    fn actual_matrix_guard_retains_raw_error_and_selection_eligibility_without_submitting()
    -> Result<(), String> {
        for created_in in ["en-US", "de"] {
            crate::i18n::set_language(created_in);
            let mut state = supplied_state()?;
            let entries = state.workflows[0].matrices("opaque-job")?;
            let before = snapshot(&state)?;
            assert_eq!(state.run_matrix(&entries), Some(entries[0].clone()));
            assert_eq!(snapshot(&state)?, before);
            assert!(state.result.is_none());
            state.matrix = entries.len();
            assert!(state.run_matrix(&entries).is_none());
            assert!(state.trust);
            assert!(entries.get(state.matrix).is_none());
            repaint_same_state(
                state,
                "No selected matrix entry",
                "Kein Matrixeintrag ausgewählt",
            )?;
        }
        crate::i18n::set_language("en-US");
        Ok(())
    }

    #[test]
    fn local_guard_receipt_rejects_mismatch_and_leaves_unmarked_provider_details_opaque()
    -> Result<(), String> {
        let mut state = supplied_state()?;
        state.remember_failure(ResultFailure::MissingMatrix);
        state.result = Some(Err(
            "provider diagnostic\nNo selected matrix entry\nKein Matrixeintrag ausgewählt".into(),
        ));
        for lang in ["en-US", "de", "en-US"] {
            crate::i18n::set_language(lang);
            let raw = state
                .result
                .as_ref()
                .and_then(|r| r.as_ref().err())
                .ok_or("missing supplied failure")?;
            assert_eq!(state.display_result_error(raw), *raw);
        }
        state.result_presentation = None;
        for raw in [
            "No selected matrix entry",
            "Repository context changed since display; review it before running",
            "Kein Matrixeintrag ausgewählt",
        ] {
            state.result = Some(Err(raw.into()));
            for lang in ["en-US", "de", "en-US"] {
                crate::i18n::set_language(lang);
                assert_eq!(state.display_result_error(raw), raw);
                assert_eq!(state.result.as_ref(), Some(&Err(raw.to_owned())));
            }
        }
        crate::i18n::set_language("en-US");
        Ok(())
    }
}
