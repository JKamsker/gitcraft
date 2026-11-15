//! Pending-commit UI and generation-guarded background verification.
use super::*;

#[derive(Default)]
pub(super) struct Staged {
    command: String,
    initialized: bool,
    generation: u64,
    freshness: u64,
    stop: Option<RunStop>,
    result: Option<Result<git::StagedResult, String>>,
    stale: bool,
    checking: bool,
    checked: Option<Instant>,
}
impl Drop for Staged {
    fn drop(&mut self) {
        if let Some(stop) = &self.stop {
            stop.stop();
        }
    }
}

impl RepoTab {
    pub(in crate::repo) fn staged_before_action(&mut self, action: &Action, ctx: &egui::Context) {
        if matches!(
            action,
            Action::Ops { .. }
                | Action::UndoableOps { .. }
                | Action::PickLines { .. }
                | Action::Commit
        ) {
            self.invalidate_staged(ctx);
        }
    }

    pub(in crate::repo) fn staged_box(&mut self, ui: &mut egui::Ui, acts: &mut Vec<Action>) {
        if !self.test_lane.staged.initialized {
            self.test_lane.staged.initialized = true;
            let path = self.path.clone();
            self.then(
                ui.ctx(),
                move || git::test_command(&path, git::DEFAULT_TEST),
                |tab, result, _| {
                    if tab.test_lane.staged.command.is_empty() {
                        tab.test_lane.staged.command = result.ok().flatten().unwrap_or_default();
                    }
                },
            );
        }
        self.check_staged(ui.ctx());
        let state = &mut self.test_lane.staged;
        if self.busy_op.is_some() || self.work_pending != 0 {
            state.stale = true;
        }
        let running = state.stop.is_some();
        ui.label(crate::i18n::text("staged-command"));
        let edit = ui.add_enabled(
            !running,
            egui::TextEdit::singleline(&mut state.command)
                .hint_text("cargo test, make check, ./test.sh…"),
        );
        edit.widget_info(|| {
            egui::WidgetInfo::labeled(
                egui::WidgetType::TextEdit,
                true,
                crate::i18n::text("staged-command"),
            )
        });
        if edit.changed() {
            state.stale = true;
            state.freshness += 1;
            state.checked = None;
        }
        ui.horizontal(|ui| {
            if ui
                .add_enabled(
                    !running && !state.command.trim().is_empty() && self.busy_op.is_none(),
                    egui::Button::new(crate::i18n::text("staged-test")),
                )
                .clicked()
            {
                acts.push(Action::TestLane(LaneAct::StagedStart));
            }
            if running && ui.button(crate::i18n::text("staged-stop")).clicked() {
                acts.push(Action::TestLane(LaneAct::StagedStop));
            }
        });
        ui.small(crate::i18n::text("staged-isolation"));
        if running {
            ui.label(crate::i18n::text("staged-running"));
        }
        if let Some(result) = &state.result {
            if state.stale || state.checking {
                ui.colored_label(
                    ui.visuals().warn_fg_color,
                    crate::i18n::text("staged-stale"),
                );
            }
            match result {
                Err(error) => {
                    ui.label(error);
                }
                Ok(result) => {
                    ui.monospace(&result.command);
                    ui.monospace(crate::i18n::message(
                        "staged-result",
                        &[
                            ("tree", result.key.tree.clone().into()),
                            (
                                "code",
                                result
                                    .ended
                                    .code
                                    .map_or_else(|| "—".into(), |c| c.to_string())
                                    .into(),
                            ),
                            (
                                "seconds",
                                format!("{:.2}", result.elapsed.as_secs_f64()).into(),
                            ),
                        ],
                    ));
                    if result.ended.stopped {
                        ui.label(crate::i18n::text("staged-cancelled"));
                    }
                    if result.ended.code == Some(127) {
                        ui.label(crate::i18n::text("staged-tooling"));
                    }
                    egui::CollapsingHeader::new(crate::i18n::text("staged-output")).show(
                        ui,
                        |ui| {
                            egui::ScrollArea::vertical()
                                .max_height(150.0)
                                .show(ui, |ui| {
                                    ui.monospace(result.lines.join("\n"));
                                });
                        },
                    );
                }
            }
        }
        ui.ctx().request_repaint_after(Duration::from_secs(1));
    }

    pub(in crate::repo) fn invalidate_staged(&mut self, ctx: &egui::Context) {
        self.test_lane.staged.stale = true;
        self.test_lane.staged.freshness += 1;
        self.test_lane.staged.checked = None;
        self.check_staged(ctx);
    }

    fn check_staged(&mut self, ctx: &egui::Context) {
        let state = &mut self.test_lane.staged;
        if state.checking
            || state.result.is_none()
            || state
                .checked
                .is_some_and(|t| t.elapsed() < Duration::from_secs(1))
        {
            return;
        }
        state.checking = true;
        let generation = state.generation;
        let freshness = state.freshness;
        let path = self.path.clone();
        let command = state.command.clone();
        self.then_or(
            ctx,
            false,
            Err,
            move || git::staged_fingerprint(&path, &command),
            move |tab, fingerprint, _| {
                let state = &mut tab.test_lane.staged;
                if state.generation != generation {
                    return;
                }
                state.checking = false;
                if state.freshness != freshness {
                    return;
                }
                state.checked = Some(Instant::now());
                state.stale = match (&state.result, fingerprint) {
                    (Some(Ok(result)), Ok(current)) => result.key.fingerprint != current,
                    _ => true,
                };
            },
        );
    }

    pub(super) fn staged_start(&mut self, ctx: &egui::Context) {
        let state = &mut self.test_lane.staged;
        if state.stop.is_some() || state.command.trim().is_empty() {
            return;
        }
        state.generation += 1;
        let generation = state.generation;
        let stop = RunStop::default();
        state.stop = Some(stop.clone());
        state.result = None;
        state.stale = true;
        state.checking = false;
        let path = self.path.clone();
        let command = state.command.trim().to_owned();
        state.command = command.clone();
        self.then_or(
            ctx,
            false,
            Err,
            move || {
                git::set_test_command(&path, git::DEFAULT_TEST, &command)?;
                git::test_staged(&path, &command, &stop)
            },
            move |tab, result, ctx| {
                let state = &mut tab.test_lane.staged;
                if state.generation != generation {
                    return;
                }
                state.stop = None;
                state.result = Some(result);
                state.checked = None;
                tab.check_staged(ctx);
            },
        );
    }

    pub(super) fn staged_stop(&mut self) {
        if let Some(stop) = &self.test_lane.staged.stop {
            stop.stop();
        }
    }
}

#[cfg(test)]
mod tests;
