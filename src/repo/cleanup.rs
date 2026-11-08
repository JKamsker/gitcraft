//! A local fresh-clone cleanup wizard; publication is a separate manual review step.
use super::{
    RepoTab,
    history_reader::Reader,
    messages::{Msg, spawn_mutation},
};
use crate::git::{
    self,
    cleanup::{Config, Replacement, Report},
};

#[derive(Default)]
pub(super) struct State {
    capability: Reader<String>,
    version: Option<String>,
    parent: String,
    paths: String,
    replacements: Vec<Replacement>,
    acknowledged: bool,
    error: Option<String>,
    running: Option<crate::tasks::Cancellation>,
    report: Option<Report>,
    publishing: bool,
    profile: Option<std::path::PathBuf>,
}
impl Drop for State {
    fn drop(&mut self) {
        if let Some(token) = &self.running {
            token.cancel();
        }
    }
}
impl RepoTab {
    pub(super) fn open_cleanup(&mut self) {
        if self.cleanup.as_ref().is_some_and(|s| s.running.is_some()) {
            return;
        }
        let repo = self.path.clone();
        let mut state = State {
            parent: self
                .path
                .parent()
                .map(|p| p.display().to_string())
                .unwrap_or_default(),
            profile: git::profile_config(),
            capability: Default::default(),
            version: None,
            paths: String::new(),
            replacements: Vec::new(),
            acknowledged: false,
            error: None,
            running: None,
            report: None,
            publishing: false,
        };
        state
            .capability
            .queue(move |_| git::cleanup::capability(&repo));
        self.cleanup = Some(state);
    }
    pub(super) fn cleanup_window(&mut self, ctx: &egui::Context) {
        if self
            .cleanup
            .as_ref()
            .is_some_and(|s| s.running.is_none() && s.profile != git::profile_config())
        {
            self.cleanup = None;
        }
        let Some(state) = &mut self.cleanup else {
            return;
        };
        if let Some(result) = state.capability.poll(&self.tx, ctx) {
            match result {
                Ok(version) => state.version = Some(version),
                Err(e) => {
                    state.error = Some(crate::i18n::message("repo-core-0293", &[("e", format!("{}", e).into())]))
                }
            }
        }
        let mut open = true;
        let mut start = false;
        let mut cancel = false;
        let idle = state.running.is_none() && self.busy_op.is_none() && !state.capability.busy();
        egui::Window::new(crate::i18n::text("repo-core-0294")).id(egui::Id::new(("cleanup", &self.path))).open(&mut open).default_width(620.0).show(ctx, |ui| {
            egui::ScrollArea::vertical().max_height(650.0).show(ui, |ui| {
                ui.label(crate::i18n::message("repo-core-0296", &[("self", format!("{}", self.path.display()).into())]));
                ui.label(crate::i18n::text("repo-core-0297"));
                if let Some(version) = &state.version { ui.label(crate::i18n::message("repo-core-0298", &[("version", format!("{}", version).into())])); }
                if let Some(report) = &state.report {
                    result_ui(ui, report, &mut state.publishing);
                } else {
                    ui.add_enabled_ui(idle, |ui| {
                        ui.label(crate::i18n::text("repo-core-0299"));
                        ui.horizontal(|ui| {
                            ui.text_edit_singleline(&mut state.parent);
                            if ui.button(crate::i18n::text("repo-core-0300")).clicked() && let Some(folder) = rfd::FileDialog::new().pick_folder() { state.parent = folder.display().to_string(); }
                        });
                        ui.label(crate::i18n::text("repo-core-0301"));
                        ui.add(egui::TextEdit::multiline(&mut state.paths).desired_rows(3).char_limit(64 << 10));
                        ui.label(crate::i18n::text("repo-core-0302"));
                        let mut remove = None;
                        for (index, rule) in state.replacements.iter_mut().enumerate() {
                            ui.horizontal(|ui| {
                                ui.label(format!("{}", index + 1));
                                ui.add(egui::TextEdit::singleline(&mut rule.find).hint_text(crate::i18n::text("repo-core-0303")).password(true).char_limit(4096).desired_width(200.0));
                                ui.label("→");
                                ui.add(egui::TextEdit::singleline(&mut rule.replace).hint_text(crate::i18n::text("repo-core-0304")).char_limit(4096).desired_width(200.0));
                                if ui.small_button(crate::i18n::text("repo-core-0305")).clicked() { remove = Some(index); }
                            });
                        }
                        if let Some(index) = remove { state.replacements.remove(index); }
                        if ui.add_enabled(state.replacements.len() < 32, egui::Button::new(crate::i18n::text("repo-core-0306"))).clicked() { state.replacements.push(Replacement::default()); }
                        ui.label(crate::i18n::text("repo-core-0307"));
                        ui.checkbox(&mut state.acknowledged, crate::i18n::text("repo-core-0308"));
                        if ui.add_enabled(state.version.is_some() && state.acknowledged && !state.parent.trim().is_empty(), egui::Button::new(crate::i18n::text("repo-core-0309"))).clicked() { start = true; }
                    });
                }
                if state.capability.busy() { ui.label(crate::i18n::text("repo-core-0310")); }
                if state.running.is_some() { ui.label(crate::i18n::text("repo-core-0311")); if ui.button(crate::i18n::text("repo-core-0312")).clicked() { cancel = true; } }
                if let Some(error) = &state.error { ui.colored_label(crate::theme::Tokens::get(ctx).danger, error); }
            });
        });
        if cancel && let Some(token) = &state.running {
            token.cancel();
        }
        if start {
            let config = Config {
                paths: state
                    .paths
                    .lines()
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(str::to_owned)
                    .collect(),
                replacements: state.replacements.clone(),
            };
            if let Err(e) = config.validate() {
                state.error = Some(e);
                return;
            }
            let parent = std::path::PathBuf::from(state.parent.trim());
            let repo = self.path.clone();
            let mut sender = self.tx.clone();
            sender.cancel = self.tx.cancel.child();
            state.running = Some(sender.cancel.clone());
            state.error = None;
            self.busy_op = Some(crate::i18n::text("repo-core-0294").into());
            spawn_mutation(
                &sender,
                ctx,
                |e| completion(Err(e)),
                move || completion(git::cleanup::execute(&repo, &parent, &config)),
            );
        } else if !open && state.running.is_none() {
            self.cleanup = None;
        }
    }
}
fn completion(result: Result<Report, String>) -> Msg {
    Msg::Then(Box::new(move |tab, _| {
        tab.busy_op = None;
        if let Some(state) = &mut tab.cleanup {
            state.running = None;
            match result {
                Ok(report) => state.report = Some(report),
                Err(e) => state.error = Some(e),
            }
            // Do not retain sensitive find/replacement values after the operation.
            state.replacements.clear();
        }
    }))
}
fn report_ui(ui: &mut egui::Ui, report: &Report) {
    ui.label(crate::i18n::message("repo-core-0313", &[("report", format!("{}", report.repo.display()).into())]));
    if ui.button(crate::i18n::text("repo-core-0314")).clicked() {
        ui.ctx().copy_text(report.repo.display().to_string());
    }
    ui.label(crate::i18n::message("repo-core-0315", &[("count", (report.changes.len()).into())]));
    for change in &report.changes {
        ui.label(format!(
            "{}: {} → {}",
            change.name,
            change.before.as_deref().unwrap_or("∅"),
            change.after.as_deref().unwrap_or("∅")
        ));
    }
    for check in &report.checks {
        ui.label(format!(
            "{} {}: {}",
            if check.passed { "✓" } else { "⚠" },
            check.label,
            check.detail
        ));
    }
    ui.collapsing(crate::i18n::text("repo-core-0316"), |ui| {
        ui.monospace(&report.output);
    });
    ui.label(crate::i18n::text("repo-core-0317"));
}
fn publishing_ui(ui: &mut egui::Ui, report: &Report) {
    ui.separator();
    ui.label(crate::i18n::text("repo-core-0318"));
    if report.checks.iter().any(|check| !check.passed) {
        ui.colored_label(
            crate::theme::Tokens::get(ui.ctx()).danger,
            crate::i18n::text("repo-core-0319"),
        );
    }
    ui.label(crate::i18n::text("repo-core-0320"));
    ui.label(crate::i18n::text("repo-core-0321"));
    ui.label(crate::i18n::text("repo-core-0322"));
    ui.label(crate::i18n::text("repo-core-0323"));
}

fn result_ui(ui: &mut egui::Ui, report: &Report, publishing: &mut bool) {
    report_ui(ui, report);
    if ui
        .button(crate::i18n::text("repo-core-0324"))
        .clicked()
    {
        *publishing = true;
    }
    if *publishing {
        publishing_ui(ui, report);
    }
}
#[cfg(test)]
mod headless_tests {
    use super::*;
    use egui_kittest::{Harness, kittest::Queryable};
    #[test]
    fn publishing_is_a_separate_deliberate_step_with_failed_check_and_revocation_guidance() {
        let report = Report {
            repo: "/recorded/rewritten.git".into(),
            changes: vec![git::cleanup::RefChange {
                name: "refs/heads/main".into(),
                before: Some("a".repeat(40)),
                after: Some("b".repeat(40)),
            }],
            checks: vec![git::cleanup::Check {
                label: "Residual search".into(),
                passed: false,
                detail: "Matching history remains".into(),
            }],
            output: "Rewritten".into(),
        };
        let mut harness = Harness::builder()
            .with_size([1000.0, 1200.0])
            .build_ui_state(
                |ui, state: &mut (Report, bool)| result_ui(ui, &state.0, &mut state.1),
                (report, false),
            );
        harness.run();
        assert!(
            harness
                .query_by_label_contains("Revoke or rotate exposed secrets")
                .is_none()
        );
        harness
            .get_by_label("Review separate publishing and revocation steps")
            .click();
        harness.run();
        harness.get_by_label_contains("Revoke or rotate exposed secrets");
        harness.get_by_label_contains("Resolve the failed result checks");
        harness.get_by_label_contains("no remote write is performed");
    }
}
