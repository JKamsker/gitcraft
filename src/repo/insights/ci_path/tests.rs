//! The view in a tab: unavailable without a forge, and a run read before drawn as a picture
//! and as a table, with its picks and Esc.

use super::*;
use crate::ci::timing::{RunInfo, TimedJob, TimedRun, TimedStep};
use crate::insights::ci_path::{Compared, compare, critical_path, timing_deps};
use crate::repo::insights::{InsightsAct, Kind};

impl crate::repo::insights::context::namespace_tests::Fixture for State {
    fn seed(&mut self) {
        use crate::repo::insights::context::namespace_tests::seed_slot;
        seed_slot(&mut self.runs);
        seed_slot(&mut self.explorer);
        self.selected = Some(7);
        self.pinned = Some(RunKey { run: 9, attempt: 3 });
        self.workflow = Some(12);
    }
    fn assert_retired(&self) {
        use crate::repo::insights::context::namespace_tests::assert_slot_retired;
        assert_slot_retired(&self.runs);
        assert_slot_retired(&self.explorer);
        assert!(self.selected.is_none() && self.run.is_none() && self.expanded.is_empty());
        assert!(self.pinned.is_none() && self.workflow.is_none());
    }
}

fn draw(tab: &mut RepoTab, ctx: &egui::Context) -> Vec<Action> {
    let mut acts = Vec::new();
    let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(1200.0, 800.0),
        )),
        ..Default::default()
    };
    ctx.run_ui(input, |ui| {
        egui::CentralPanel::default().show(ui, |ui| tab.insights_ui(ui, &mut acts));
    })
    .drop_without_applying_deltas();
    acts
}

fn job(name: &str, state: CheckState, times: (Option<i64>, Option<i64>, Option<i64>)) -> TimedJob {
    TimedJob {
        id: 1,
        name: name.to_owned(),
        state,
        created: times.0,
        started: times.1,
        finished: times.2,
        url: "https://github.com/o/r/actions/runs/9/job/1".to_owned(),
        steps: vec![TimedStep {
            name: "Run".to_owned(),
            number: 2,
            state,
            started: times.1,
            finished: times.2,
        }],
        ..TimedJob::default()
    }
}

fn timed(jobs: Vec<TimedJob>, number: u64) -> TimedRun {
    TimedRun {
        info: RunInfo {
            id: 9,
            name: "CI".to_owned(),
            number,
            sha: "a".repeat(40),
            started: Some(1000),
            ..RunInfo::default()
        },
        jobs,
        more: false,
    }
}

pub(super) fn explorer() -> Explorer {
    let run = timed(
        vec![
            job(
                "build",
                CheckState::Passed,
                (Some(1000), Some(1010), Some(1300)),
            ),
            job(
                "test",
                CheckState::Failed,
                (Some(1301), Some(1320), Some(1500)),
            ),
            job("queued", CheckState::Running, (Some(1301), None, None)),
            job("unknown", CheckState::Skipped, (None, None, None)),
        ],
        8,
    );
    let before = timed(
        vec![
            job("build", CheckState::Passed, (Some(0), Some(5), Some(100))),
            job(
                "test",
                CheckState::Passed,
                (Some(101), Some(102), Some(200)),
            ),
        ],
        7,
    );
    let deps = timing_deps(&run.jobs);
    let critical = critical_path(&run.jobs, &deps);
    let compared = Compared {
        critical: critical_path(&before.jobs, &timing_deps(&before.jobs)),
        comparison: compare::compare(&run, &before),
        run: before,
    };
    let project = Project {
        forge: crate::forge::Forge::GitHub,
        host: "github.com".to_owned(),
        path: "o/r".to_owned(),
    };
    let provenance =
        crate::insights::ci_path::provenance(&project, &run, Some(&compared), ("HEAD", "abc"));
    Explorer {
        project,
        run,
        deps,
        critical,
        baselines: vec![compared.run.info.clone()],
        baseline_error: None,
        recent: Vec::new(),
        history: Default::default(),
        diagnostics: Vec::new(),
        compared: Some(compared),
        now: 1600,
        provenance,
    }
}

#[test]
fn without_a_forge_the_view_says_what_it_needs() {
    let ctx = egui::Context::default();
    crate::theme::install_fonts(&ctx);
    let mut tab = RepoTab::new_deferred("/nonexistent/ci-path-view".into(), &ctx);
    tab.apply(Action::Insights(InsightsAct::Open(Kind::CiPath)), &ctx);
    let state = tab.insights.as_ref().map(|v| &v.ci_path);
    assert!(state.is_some_and(|s| s.unavailable.is_some()));
    draw(&mut tab, &ctx);
    tab.apply(Action::Insights(InsightsAct::ToggleTable), &ctx);
    draw(&mut tab, &ctx);
}

#[test]
fn a_run_draws_as_a_waterfall_and_as_a_table_and_esc_unpicks_first() {
    let ctx = egui::Context::default();
    crate::theme::install_fonts(&ctx);
    let mut tab = RepoTab::new_deferred("/nonexistent/ci-path-run".into(), &ctx);
    tab.apply(Action::Insights(InsightsAct::Open(Kind::CiPath)), &ctx);
    let project = explorer().project;
    if let Some(state) = tab.insights.as_mut().map(|v| &mut v.ci_path) {
        state.unavailable = None;
        state.subject = Some(("HEAD".to_owned(), "abc".to_owned()));
        let request = state.runs.start();
        let runs = Runs {
            list: vec![(
                project.clone(),
                Run {
                    id: 9,
                    name: "CI".to_owned(),
                    ..Run::default()
                },
            )],
            errors: vec!["up/r: offline".to_owned()],
        };
        state.runs.accept(request, Ok(runs));
        state.run = Some((project, 9));
        let request = state.explorer.start();
        state.explorer.accept(request, Ok(explorer()));
    }
    // The failed job is on the critical path, after the one it was queued behind.
    let critical = tab
        .insights
        .as_ref()
        .and_then(|v| v.ci_path.explorer.ready())
        .map(|e| e.critical.jobs.clone());
    assert_eq!(critical, Some(vec![0, 1]));
    for i in [0, 1] {
        tab.apply(Action::Insights(InsightsAct::CiPath(Act::Expand(i))), &ctx);
    }
    tab.apply(
        Action::Insights(InsightsAct::CiPath(Act::Select(Some(1)))),
        &ctx,
    );
    draw(&mut tab, &ctx);
    tab.apply(Action::Insights(InsightsAct::ToggleTable), &ctx);
    draw(&mut tab, &ctx);
    tab.apply(Action::Insights(InsightsAct::Back), &ctx);
    assert!(
        tab.insights
            .as_ref()
            .is_some_and(|v| v.ci_path.selected.is_none())
    );
    tab.apply(Action::Insights(InsightsAct::Back), &ctx);
    assert!(tab.insights.is_none());
}

#[test]
fn a_missing_tool_is_what_is_needed_not_a_failure() {
    assert!(tool_problem(
        "gh isn't installed: gitcraft uses it for pull requests"
    ));
    assert!(tool_problem(
        "To get started with GitHub CLI, please run:  gh auth login"
    ));
    assert!(!tool_problem("HTTP 502"));
}

#[test]
fn only_current_run_list_completion_changes_availability() {
    for error in [
        "gh isn't installed",
        "gh isn't signed in",
        "run gh auth login",
    ] {
        let ctx = egui::Context::default();
        let mut tab = RepoTab::new_deferred("/nonexistent/ci-path-completions".into(), &ctx);
        tab.insights = Some(Box::new(super::super::InsightsView::new(Kind::CiPath)));
        let (old, current) = {
            let state = &mut tab.insights.as_mut().unwrap().ci_path;
            state.selected = Some(7);
            state.expanded.insert(7);
            (state.runs.start(), state.runs.start())
        };
        // Old errors neither end a newer read nor hide its eventual success.
        tab.ci_path_runs(old, Err(error.into()), &ctx);
        let state = &tab.insights.as_ref().unwrap().ci_path;
        assert!(state.runs.loading);
        assert!(state.runs.value.is_none());
        assert!(state.unavailable.is_none());
        assert_eq!(state.selected, Some(7));
        assert!(state.expanded.contains(&7));

        tab.ci_path_runs(current, Ok(Runs::default()), &ctx);
        tab.ci_path_runs(old, Err(error.into()), &ctx);
        let state = &tab.insights.as_ref().unwrap().ci_path;
        assert!(!state.runs.loading);
        assert!(state.runs.ready().is_some());
        assert!(state.unavailable.is_none());
        assert_eq!(state.selected, Some(7));
        assert!(state.expanded.contains(&7));

        // The current tool error is authoritative; an old success cannot clear it.
        let current = tab.insights.as_mut().unwrap().ci_path.runs.start();
        tab.ci_path_runs(current, Err(error.into()), &ctx);
        tab.ci_path_runs(old, Ok(Runs::default()), &ctx);
        let state = &tab.insights.as_ref().unwrap().ci_path;
        assert!(!state.runs.loading);
        assert_eq!(state.runs.error(), Some(error));
        assert_eq!(
            state.unavailable.as_ref().map(|v| v.1.as_str()),
            Some(error)
        );

        let current = tab.insights.as_mut().unwrap().ci_path.runs.start();
        tab.ci_path_runs(current, Ok(Runs::default()), &ctx);
        let state = &tab.insights.as_ref().unwrap().ci_path;
        assert!(state.unavailable.is_none());
        assert!(state.runs.ready().is_some());
    }
}
