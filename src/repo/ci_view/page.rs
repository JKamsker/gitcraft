//! The page of the pipeline view: which run, how it went, its jobs and the steps of one.

use egui::{RichText, Ui};

use super::{CiAct, CiView, graph, steps};
use crate::ci::{self, CheckState, Job, Project, Run};
use crate::forge::Forge;
use crate::git;
use crate::icons;
use crate::repo::ci_status::{state_color, state_word};
use crate::repo::popout::{self, Panel};
use crate::repo::{Action, RepoTab, short, widgets};
use crate::theme::{self, Tokens};

const MARGIN_X: i8 = 24;

impl RepoTab {
    /// The big area.
    pub(in crate::repo) fn ci_view_ui(&mut self, ui: &mut Ui, acts: &mut Vec<Action>) {
        let t = Tokens::get(ui.ctx());
        self.local_ci_ui(ui, acts);
        let Some(view) = &self.ci_view else {
            return;
        };
        let noun = match self.forge() {
            Some(Forge::GitLab) => crate::i18n::text("repo-workflows-ci_view-ci-view-ui-d1a8fffe"),
            _ => crate::i18n::text("repo-workflows-ci_view-ci-view-ui-c454bec0"),
        };
        let popped = !self.docked(Panel::Ci);
        page_header(
            ui,
            view,
            &crate::i18n::message(
                "repo-workflows-ci_view-ci-view-ui-b431c0d7",
                &[
                    ("noun", (noun).into()),
                    ("short-view-sha-1", (short(&view.sha)).into()),
                ],
            ),
            popped,
            acts,
        );
        let runs = match &view.runs {
            None => {
                ui.add_space(40.0);
                ui.vertical_centered(|ui| ui.add(crate::spinner::Spinner::new()));
                return;
            }
            Some(Err(e)) => {
                ui.add_space(20.0);
                ui.vertical_centered(|ui| ui.colored_label(t.danger, e));
                return;
            }
            Some(Ok(builds)) => {
                for error in &builds.errors {
                    ui.colored_label(t.warning, error);
                }
                &builds.runs
            }
        };
        egui::ScrollArea::vertical()
            .id_salt(("ci-view", &view.sha))
            .auto_shrink(false)
            .show(ui, |ui| {
                egui::Frame::new()
                    .inner_margin(egui::Margin::symmetric(MARGIN_X, 18))
                    .show(ui, |ui| {
                        if runs.is_empty() {
                            let partial = view.runs.as_ref().is_some_and(|result| {
                                result
                                    .as_ref()
                                    .is_ok_and(|builds| !builds.errors.is_empty())
                            });
                            ui.label(
                                RichText::new(if partial {
                                    crate::i18n::text("repo-workflows-ci_view-ci-view-ui-e50c77eb")
                                } else {
                                    crate::i18n::text("repo-workflows-ci_view-ci-view-ui-90db5328")
                                })
                                .color(t.text_faint)
                                .italics(),
                            );
                            return;
                        }
                        page(ui, view, runs, acts);
                    });
            });
    }
}

/// The title bar: back, open the run in the browser, load again.
/// `popped`: the view is in its own window (see [`popout::button`]).
fn page_header(ui: &mut Ui, view: &CiView, title: &str, popped: bool, acts: &mut Vec<Action>) {
    let url = view.shown_run().map(|run| run.url.clone());
    widgets::panel_header(ui, title, |ui| {
        if icons::small_button(
            ui,
            "x",
            popout::close_tip(
                popped,
                crate::i18n::text("repo-workflows-ci_view-page-header-8c6c9cf3"),
            ),
        )
        .clicked()
        {
            acts.push(Action::CloseCi);
        }
        popout::button(ui, Panel::Ci, popped, acts);
        if let Some(url) = url
            && icons::small_button(
                ui,
                "external-link",
                crate::i18n::text("repo-workflows-ci_view-page-header-a7eac6b5"),
            )
            .clicked()
        {
            acts.push(Action::OpenUrl(url));
        }
        if view.loading {
            ui.add(crate::spinner::Spinner::new());
        } else if icons::small_button(
            ui,
            "refresh-cw",
            crate::i18n::text("repo-workflows-ci_view-page-header-d62bc4a3"),
        )
        .clicked()
        {
            acts.push(Action::Ci(CiAct::Reload));
        }
    });
}

/// Shows project-qualified runs and the selected run's jobs, actions and log.
fn page(ui: &mut Ui, view: &CiView, runs: &[(Project, Run)], acts: &mut Vec<Action>) {
    let t = Tokens::get(ui.ctx());
    let now = git::now();
    if runs.len() > 1 {
        run_picker(ui, runs, view.project.as_ref(), view.run, acts);
        ui.add_space(14.0);
    }
    let Some(run) = view.shown_run() else {
        return;
    };
    run_header(ui, run, now, acts);
    let jobs = view.shown_jobs();
    let ok_jobs = jobs.and_then(|jobs| jobs.as_ref().ok());
    ui.add_space(10.0);
    run_actions(ui, view, run, ok_jobs.map_or(&[], Vec::as_slice), acts);
    section(
        ui,
        crate::i18n::text("repo-workflows-ci_view-page-a47c5497"),
    );
    match jobs {
        None => {
            ui.add(crate::spinner::Spinner::new());
        }
        Some(Err(e)) => {
            ui.colored_label(t.danger, e);
        }
        Some(Ok(jobs)) if jobs.is_empty() => {
            ui.label(
                RichText::new(crate::i18n::text("repo-workflows-ci_view-page-66f52811"))
                    .color(t.text_faint)
                    .italics(),
            );
        }
        Some(Ok(jobs)) => {
            egui::ScrollArea::horizontal()
                .id_salt(("ci-graph", run.id))
                .show(ui, |ui| {
                    graph::job_graph(ui, jobs, run.started, view.job, now, acts)
                });
            if let Some(job) = jobs.iter().find(|job| Some(job.id) == view.job) {
                section(
                    ui,
                    &crate::i18n::message(
                        "repo-workflows-ci_view-page-30fc4b32",
                        &[(
                            "job-name-to-uppercase-1",
                            (job.name.to_uppercase()).clone().into(),
                        )],
                    ),
                );
                steps::timeline(ui, job, now);
            }
        }
    }
    if let Some(log) = &view.log {
        section(
            ui,
            crate::i18n::text("repo-workflows-ci_view-page-04ec3970"),
        );
        match log {
            None => {
                ui.add(crate::spinner::Spinner::new());
            }
            Some(Err(e)) => {
                ui.colored_label(t.danger, e);
            }
            Some(Ok(text)) if text.trim().is_empty() => {
                ui.label(
                    RichText::new(crate::i18n::text("repo-workflows-ci_view-page-7261125c"))
                        .color(t.text_faint)
                        .italics(),
                );
            }
            Some(Ok(text)) => {
                if ui
                    .button(crate::i18n::text("repo-workflows-ci_view-page-77a860cb"))
                    .on_hover_text(crate::i18n::text("repo-workflows-ci_view-page-da532fb6"))
                    .clicked()
                {
                    acts.push(Action::Agent(crate::repo::agents::AgentAct::Send(
                        crate::agent::handoff::ci_message(&run.name, text),
                        crate::i18n::text("repo-workflows-ci_view-page-20681c9f"),
                    )));
                }
                steps::log(ui, text);
            }
        }
    }
}

/// A tab per run of the commit: its state, workflow and number.
fn run_picker(
    ui: &mut Ui,
    runs: &[(Project, Run)],
    project: Option<&Project>,
    shown: Option<u64>,
    acts: &mut Vec<Action>,
) {
    let t = Tokens::get(ui.ctx());
    ui.horizontal_wrapped(|ui| {
        for (owner, run) in runs {
            let selected = Some(run.id) == shown && Some(owner) == project;
            let text = RichText::new(format!("    {} #{}", run.name, run.number))
                .color(if selected { t.text } else { t.text_dim });
            let resp = ui.selectable_label(selected, text).on_hover_text(format!(
                "{}\n{}\n{}",
                owner.path,
                run.title,
                state_word(run.state)
            ));
            // A dot of the run's colour in the room left before its name.
            let dot = resp.rect.left_center() + egui::vec2(10.0, 0.0);
            ui.painter()
                .circle_filled(dot, 3.5, state_color(&t, run.state));
            if resp.clicked() {
                acts.push(Action::Ci(CiAct::SelectRun(owner.clone(), run.id)));
            }
        }
    });
}

/// What the run is, how it went and how long it took.
fn run_header(ui: &mut Ui, run: &Run, now: i64, acts: &mut Vec<Action>) {
    let t = Tokens::get(ui.ctx());
    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new(&run.name).font(theme::semibold(18.0)));
        ui.label(
            RichText::new(format!("#{}", run.number))
                .font(egui::FontId::proportional(18.0))
                .color(t.text_faint),
        );
    });
    if !run.title.is_empty() && run.title != run.branch {
        ui.label(RichText::new(&run.title).color(t.text_dim));
    }
    ui.add_space(6.0);
    ui.horizontal_wrapped(|ui| {
        let state = match state_word(run.state) {
            "skipped" => crate::i18n::text("repo-workflows-ci_view-run-header-b368daf0"),
            word => word,
        };
        widgets::chip(ui, state, state_color(&t, run.state));
        if !run.event.is_empty() {
            widgets::chip(ui, &run.event, t.text_dim);
        }
        if run.attempt > 1 {
            widgets::chip(
                ui,
                &crate::i18n::message(
                    "repo-workflows-ci_view-run-header-396f73af",
                    &[("run-attempt-1", (run.attempt).into())],
                ),
                t.text_dim,
            );
        }
        if !run.branch.is_empty() {
            ui.label(RichText::new(&run.branch).font(theme::mono(11.5)));
        }
        if !run.sha.is_empty()
            && ui
                .link(RichText::new(short(&run.sha)).font(theme::mono(11.5)))
                .on_hover_text(crate::i18n::text(
                    "repo-workflows-ci_view-run-header-0b4ad7c1",
                ))
                .clicked()
        {
            acts.push(Action::SelectHash(run.sha.clone()));
        }
        if let Some(seconds) = ci::duration(run.started, run.finished, now) {
            let took = if run.finished.is_some() {
                crate::i18n::message(
                    "repo-workflows-ci_view-run-header-ac6a17db",
                    &[(
                        "ci-duration-label-seconds-1",
                        (ci::duration_label(seconds)).clone().into(),
                    )],
                )
            } else {
                crate::i18n::message(
                    "repo-workflows-ci_view-run-header-420fc08d",
                    &[(
                        "ci-duration-label-seconds-1",
                        (ci::duration_label(seconds)).clone().into(),
                    )],
                )
            };
            ui.label(RichText::new(took).color(t.text_faint));
        }
        if let Some(started) = run.started {
            ui.label(
                RichText::new(crate::i18n::message(
                    "repo-workflows-ci_view-run-header-4e8bc57f",
                    &[(
                        "git-relative-time-started-now-1",
                        (git::relative_time(started, now)).clone().into(),
                    )],
                ))
                .color(t.text_faint),
            );
        }
    });
}

/// Run the failed jobs again, cancel, and the failed jobs' log.
fn run_actions(ui: &mut Ui, view: &CiView, run: &Run, jobs: &[Job], acts: &mut Vec<Action>) {
    let failed = jobs.iter().any(|job| job.state == CheckState::Failed);
    let running = run.state == CheckState::Running;
    ui.horizontal(|ui| {
        // A run can fail with no job failed (one was cancelled): those count as failed then.
        if (failed || run.state == CheckState::Failed) && !running {
            let rerun = ui
                .add_enabled(
                    !view.acting,
                    egui::Button::new(crate::i18n::text(
                        "repo-workflows-ci_view-run-actions-8c40a11a",
                    )),
                )
                .on_hover_text(crate::i18n::text(
                    "repo-workflows-ci_view-run-actions-46677f1c",
                ));
            if rerun.clicked() {
                acts.push(Action::Ci(CiAct::RerunFailed));
            }
        }
        if running
            && ui
                .add_enabled(
                    !view.acting,
                    egui::Button::new(crate::i18n::text(
                        "repo-workflows-ci_view-run-actions-19766ed6",
                    )),
                )
                .clicked()
        {
            acts.push(Action::Ci(CiAct::Cancel));
        }
        if failed {
            let label = if view.log.is_some() {
                crate::i18n::text("repo-workflows-ci_view-run-actions-7b68b715")
            } else {
                crate::i18n::text("repo-workflows-ci_view-run-actions-1c703119")
            };
            if ui.button(label).clicked() {
                acts.push(Action::Ci(CiAct::ToggleLog));
            }
        }
        if view.acting {
            ui.add(crate::spinner::Spinner::new());
        }
    });
}

fn section(ui: &mut Ui, title: &str) {
    let t = Tokens::get(ui.ctx());
    ui.add_space(22.0);
    ui.label(
        RichText::new(title)
            .font(theme::semibold(10.5))
            .color(t.text_faint),
    );
    ui.add_space(6.0);
}
