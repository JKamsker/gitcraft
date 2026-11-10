//! Status probes and generation-bound file read completion.

use super::*;

impl RepoTab {
    /// One active probe per view. Further status observations request one follow-up,
    /// rather than queueing work that will only be discarded later.
    pub(in crate::repo) fn status_changed_file_view(&mut self, ctx: &egui::Context) {
        if let Some(view) = &mut self.file_view {
            view.probe.dirty = true;
        }
        self.poll_file_view(ctx);
    }

    pub(in crate::repo) fn poll_file_view(&mut self, ctx: &egui::Context) {
        let now = Instant::now();
        let Some(view) = &mut self.file_view else {
            return;
        };
        if let Some(due) = view.retry_load {
            if now >= due {
                view.retry_load = None;
                self.load_file_view(ctx);
            } else {
                ctx.request_repaint_after(due - now);
            }
        }
        let Some(view) = &mut self.file_view else {
            return;
        };
        if view.probe.active.is_some() || !view.probe.dirty {
            return;
        }
        if let Some(due) = view.probe.retry
            && now < due
        {
            ctx.request_repaint_after(due - now);
            return;
        }
        view.probe.retry = None;
        view.probe.dirty = false;
        let check = next_generation();
        view.probe.active = Some(check);
        view.input_check = check;
        view.line_input_check = check;
        let (generation, blame_generation, path, rev, lines) = (
            view.generation,
            view.blame_generation,
            view.path.clone(),
            view.rev.clone(),
            view.lines.clone(),
        );
        let blame = view.blame.is_some() || view.pending[1].is_some() || view.tab == FileTab::Blame;
        let repo = self.path.clone();
        self.then_or(
            ctx,
            false,
            Err,
            move || {
                Ok(match lines {
                    Some((at, range)) => {
                        ProbeInputs::Lines(git::line_history_key(&repo, &at, &path, range))
                    }
                    None => ProbeInputs::File(file_inputs(&repo, rev.as_deref(), &path, blame)),
                })
            },
            move |tab, result, ctx| {
                tab.finish_file_probe(generation, blame_generation, check, result, ctx)
            },
        );
    }

    pub(in crate::repo::file_view) fn finish_file_probe(
        &mut self,
        generation: u64,
        blame_generation: u64,
        check: u64,
        result: Result<ProbeInputs, String>,
        ctx: &egui::Context,
    ) {
        let Some(view) = &mut self.file_view else {
            return;
        };
        if view.probe.active != Some(check) {
            return;
        }
        view.probe.active = None;
        match result {
            Err(error) => {
                if error == crate::tasks::BUSY {
                    view.probe.dirty = true;
                    view.probe.retry = Some(Instant::now() + Duration::from_millis(100));
                } else {
                    view.probe.dirty = false;
                } // No automatic cancellation/panic/error loop.
            }
            Ok(_) if view.generation != generation || view.blame_generation != blame_generation => {
                view.probe.dirty = true
            }
            Ok(ProbeInputs::File(inputs)) => {
                self.file_status_inputs(generation, check, inputs, ctx)
            }
            Ok(ProbeInputs::Lines(inputs)) => {
                let Some(view) = &mut self.file_view else {
                    return;
                };
                if let Ok(inputs) = &inputs {
                    view.line_observed = Some(inputs.clone());
                }
                // A pending history read supplies its own captured key on completion.
                // Until then, a successful observation cannot establish a mismatch.
                if view.history.is_none() && view.pending[0].is_some() && inputs.is_ok() {
                } else if !view.reuses_line_inputs(&inputs) {
                    self.invalidate_mutable_file_view(ctx);
                }
            }
        }
        self.poll_file_view(ctx);
    }

    pub(in crate::repo) fn begin_file_load(&mut self, pane: FileTab) -> Option<u64> {
        let view = self.file_view.as_mut()?;
        let slot = &mut view.pending[pane_index(pane)];
        if slot.is_some() {
            return None;
        }
        let token = next_generation();
        *slot = Some(token);
        Some(token)
    }

    pub(in crate::repo) fn finish_file_load(
        &mut self,
        pane: FileTab,
        token: u64,
        generation: u64,
        failure: Option<&str>,
        ctx: &egui::Context,
    ) -> bool {
        let Some(view) = &mut self.file_view else {
            return false;
        };
        let slot = &mut view.pending[pane_index(pane)];
        if *slot != Some(token) {
            return false;
        }
        *slot = None;
        let current = if pane == FileTab::Blame {
            view.blame_generation
        } else {
            view.generation
        };
        if current != generation {
            self.load_file_view(ctx);
            return false;
        }
        if failure == Some(crate::tasks::BUSY) {
            view.retry_load = Some(Instant::now() + Duration::from_millis(100));
            ctx.request_repaint_after(Duration::from_millis(100));
            return false;
        }
        true
    }

    pub(in crate::repo) fn accept_blame_input(
        &mut self,
        input: Option<String>,
        ctx: &egui::Context,
    ) -> bool {
        let Some(view) = &mut self.file_view else {
            return false;
        };
        if view
            .observed_blame
            .as_ref()
            .is_some_and(|observed| Some(observed) != input.as_ref())
        {
            view.invalidate_blame();
            self.load_file_view(ctx);
            return false;
        }
        view.blame_input = input;
        true
    }

    pub(in crate::repo::file_view) fn file_status_inputs(
        &mut self,
        generation: u64,
        check: u64,
        inputs: FileInputs,
        ctx: &egui::Context,
    ) {
        let Some(view) = &mut self.file_view else {
            return;
        };
        if view.generation != generation || view.input_check != check {
            return;
        }
        let history_changed = view
            .history_input
            .as_ref()
            .is_some_and(|old| inputs.history.as_ref() != Some(old))
            || view
                .history
                .as_ref()
                .is_some_and(|result| result.is_err() || view.history_input.is_none());
        let text_changed = view
            .text_input
            .as_ref()
            .is_some_and(|old| inputs.text.as_ref() != Some(old))
            || view
                .text
                .as_ref()
                .is_some_and(|result| result.is_err() || view.text_input.is_none());
        if history_changed || text_changed {
            self.invalidate_mutable_file_view(ctx);
            return;
        }
        // A request that has not replied yet compares its own captured inputs with
        // this observation upon arrival. Unrelated probes do not supersede it.
        let blame_changed = view
            .blame_input
            .as_ref()
            .is_some_and(|old| inputs.blame.as_ref() != Some(old))
            || matches!(view.blame, Some(Err(_)));
        view.observed_blame = inputs.blame.clone();
        view.observed_inputs = Some(inputs);
        if blame_changed {
            view.invalidate_blame();
            if view.tab == FileTab::Blame {
                self.load_file_view(ctx);
            }
        }
    }

    pub(in crate::repo) fn accept_file_inputs(
        &mut self,
        generation: u64,
        path: &str,
        rev: Option<&str>,
        tab: FileTab,
        input: Option<String>,
        ctx: &egui::Context,
    ) -> bool {
        let Some(view) = &mut self.file_view else {
            return false;
        };
        if view.generation != generation || view.path != path || view.rev.as_deref() != rev {
            return false;
        }
        let observed = view.observed_inputs.as_ref().map(|inputs| match tab {
            FileTab::History => &inputs.history,
            FileTab::File => &inputs.text,
            FileTab::Blame => unreachable!("blame uses its independent epoch"),
        });
        if observed.is_some_and(|expected| expected != &input) {
            self.invalidate_mutable_file_view(ctx);
            return false;
        }
        match tab {
            FileTab::History => view.history_input = input,
            FileTab::File => view.text_input = input,
            FileTab::Blame => unreachable!("blame uses its independent epoch"),
        }
        true
    }

    pub(in crate::repo) fn invalidate_mutable_file_view(&mut self, ctx: &egui::Context) {
        if let Some(view) = &mut self.file_view {
            if !view.invalidate_mutable() {
                // Full OIDs pin content, but blame.ignoreRevsFile can change attribution.
                view.invalidate_blame();
            }
            self.load_file_view(ctx);
        }
    }
}
