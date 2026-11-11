use super::*;
use crate::repo::insights::{Kind, test_support as schedule};

fn assert_retired(tab: &RepoTab) {
    let view = tab.insights.as_ref().unwrap();
    let state = &view.ci_path;
    assert!(!view.loaded.contains(&Kind::CiPath));
    assert!(state.subject.is_none());
    assert!(state.unavailable.is_none());
    assert!(!state.runs.loading && state.runs.value.is_none());
    assert!(!state.explorer.loading && state.explorer.value.is_none());
    assert!(state.run.is_none() && state.selected.is_none() && state.expanded.is_empty());
    assert!(state.pinned.is_none() && state.workflow.is_none());
}

#[test]
fn context_switches_retire_both_ci_stages_and_reload_the_visible_page() {
    let _pause = crate::network::test_pause::set(Some(crate::network::Pause::Switch));
    for transition in schedule::TRANSITIONS {
        for error in [false, true] {
            let ctx = egui::Context::default();
            let mut tab = schedule::tab(&ctx, Kind::CiPath);
            let (runs, explorer) = {
                let state = &mut tab.insights.as_mut().unwrap().ci_path;
                state.selected = Some(0);
                state.pinned = Some(RunKey { run: 9, attempt: 3 });
                state.workflow = Some(12);
                state.expanded.insert(0);
                let initial = state.explorer.start();
                state.explorer.accept(initial, Ok(tests::explorer()));
                (state.runs.start(), state.explorer.start())
            };
            let stale_runs = if error {
                Err("gh auth login".into())
            } else {
                Ok(Runs::default())
            };
            let old_runs = schedule::reply(&tab, &ctx, stale_runs, move |tab, result, ctx| {
                tab.ci_path_runs(runs, result, ctx);
            });
            let stale_run = if error {
                Err("old explorer".into())
            } else {
                Ok(tests::explorer())
            };
            let old_run = schedule::reply(&tab, &ctx, stale_run, move |tab, result, _| {
                tab.ci_path_run(explorer, result);
            });
            schedule::transition(&mut tab, &ctx, transition);
            assert_retired(&tab);
            old_runs(&mut tab, &ctx);
            old_run(&mut tab, &ctx);
            assert_retired(&tab);
            schedule::reload_from_frame(&mut tab, &ctx);
            assert!(tab.insights.as_ref().unwrap().ci_path.runs.loading);
            let current = tab.insights.as_mut().unwrap().ci_path.runs.start();
            let fresh =
                schedule::reply(&tab, &ctx, Ok(Runs::default()), move |tab, result, ctx| {
                    tab.ci_path_runs(current, result, ctx);
                });
            fresh(&mut tab, &ctx);
            assert!(
                tab.insights
                    .as_ref()
                    .unwrap()
                    .ci_path
                    .runs
                    .ready()
                    .is_some()
            );
            let current = tab.insights.as_mut().unwrap().ci_path.explorer.start();
            let fresh =
                schedule::reply(&tab, &ctx, Ok(tests::explorer()), move |tab, result, _| {
                    tab.ci_path_run(current, result);
                });
            fresh(&mut tab, &ctx);
            assert!(
                tab.insights
                    .as_ref()
                    .unwrap()
                    .ci_path
                    .explorer
                    .ready()
                    .is_some()
            );
        }
    }
}

#[test]
fn unavailable_ci_subjects_retire_outstanding_runs_and_explorers() {
    for missing_commit in [false, true] {
        let ctx = egui::Context::default();
        let mut tab = schedule::tab(&ctx, Kind::CiPath);
        let (runs, explorer) = {
            let state = &mut tab.insights.as_mut().unwrap().ci_path;
            (state.runs.start(), state.explorer.start())
        };
        tab.remotes.clear();
        if missing_commit {
            tab.refs.head.clear();
        }
        crate::repo::messages::record_jobs(|| tab.ci_path_load(&ctx));
        tab.ci_path_runs(runs, Ok(Runs::default()), &ctx);
        tab.ci_path_run(explorer, Ok(tests::explorer()));
        let state = &tab.insights.as_ref().unwrap().ci_path;
        assert!(state.unavailable.is_some());
        assert!(!state.runs.loading && state.runs.value.is_none());
        assert!(!state.explorer.loading && state.explorer.value.is_none());
    }
}
