use super::*;

impl RepoTab {
    pub(in crate::repo) fn local_ci_context_default(&mut self) {
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

    pub(in crate::repo) fn local_ci_act(&mut self, act: Act, ctx: &egui::Context) {
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
                let key = super::super::messages::mutation_key(&path);
                state.submit(&self._jobs.0, ctx, Some(key), move || {
                    Reply::Finished(run::execute(path, selection, log, repaint))
                });
            }
            Act::Open | Act::Cancel => {}
        }
    }
    pub(in crate::repo) fn local_ci_ui(&mut self, ui: &mut egui::Ui, acts: &mut Vec<Action>) {
        self.local_ci_context_default();
        self.local_ci.ui(ui, acts, &self.remotes);
    }
}
