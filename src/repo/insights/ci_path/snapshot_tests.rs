use super::*;
use crate::repo::insights::{Kind, test_support as schedule};

fn state(tab: &mut RepoTab) -> Result<&mut State, String> {
    tab.insights
        .as_mut()
        .map(|view| &mut view.ci_path)
        .ok_or("Insights missing".into())
}

#[test]
fn exact_revision_project_workflow_and_attempt_guard_snapshot_acceptance() {
    let snapshot = tests::explorer();
    let mut state = State {
        run: Some((snapshot.project.clone(), snapshot.run.info.id)),
        subject: Some(("HEAD".into(), snapshot.run.info.sha.clone())),
        pinned: Some(RunKey::of(&snapshot.run.info)),
        ..State::default()
    };
    assert!(state.accepts(&snapshot));
    for mismatch in [
        "revision", "project", "run", "attempt", "workflow", "baseline",
    ] {
        let mut wrong = snapshot.clone();
        match mismatch {
            "revision" => wrong.run.info.sha = "other".into(),
            "project" => wrong.project.path = "other/repo".into(),
            "run" => wrong.run.info.id += 1,
            "attempt" => wrong.run.info.attempt += 2,
            "workflow" => {
                state.workflow = Some(7);
                wrong.run.info.workflow_id = Some(8);
            }
            _ => {
                state.workflow = None;
                state.baseline = Baseline::Attempt(RunKey {
                    run: 999,
                    attempt: 9,
                });
            }
        }
        assert!(!state.accepts(&wrong), "{mismatch}");
    }
}

#[test]
fn changing_attempt_or_baseline_retires_prior_successes_and_errors() -> Result<(), String> {
    for error in [false, true] {
        let ctx = egui::Context::default();
        let mut tab = schedule::tab(&ctx, Kind::CiPath);
        let mut snapshot = tests::explorer();
        snapshot.run.info.attempt = 3;
        let mut previous = snapshot.run.clone();
        previous.info.attempt = 2;
        snapshot.history.runs = vec![previous, snapshot.run.clone()];
        let project = snapshot.project.clone();
        let sha = snapshot.run.info.sha.clone();
        let key = RunKey::of(&snapshot.run.info);
        let old = {
            let state = state(&mut tab)?;
            state.run = Some((project, key.run));
            state.subject = Some(("HEAD".into(), sha));
            state.pinned = Some(key);
            let ready = state.explorer.start();
            assert!(state.explorer.accept(ready, Ok(snapshot.clone())));
            state.explorer.start()
        };
        crate::repo::messages::record_jobs(|| {
            tab.ci_path_act(
                Act::PickAttempt(RunKey {
                    run: key.run,
                    attempt: 2,
                }),
                &ctx,
            )
        });
        assert!(state(&mut tab)?.explorer.ready().is_none());
        assert_eq!(
            state(&mut tab)?.pinned,
            Some(RunKey {
                run: key.run,
                attempt: 2
            })
        );
        tab.ci_path_run(
            old,
            if error {
                Err("old failure".into())
            } else {
                Ok(snapshot)
            },
        );
        assert!(state(&mut tab)?.explorer.ready().is_none());
        assert!(state(&mut tab)?.explorer.loading);
    }
    Ok(())
}

#[test]
fn run_inventory_attempt_is_pinned_and_refresh_does_not_follow_a_later_rerun() -> Result<(), String>
{
    let ctx = egui::Context::default();
    let mut tab = schedule::tab(&ctx, Kind::CiPath);
    let snapshot = tests::explorer();
    let entry = |attempt| Runs {
        list: vec![(
            snapshot.project.clone(),
            Run {
                id: snapshot.run.info.id,
                attempt,
                ..Run::default()
            },
        )],
        errors: Vec::new(),
    };
    state(&mut tab)?.subject = Some(("HEAD".into(), snapshot.run.info.sha.clone()));
    for attempt in [2, 3, 4] {
        let request = state(&mut tab)?.runs.start();
        crate::repo::messages::record_jobs(|| tab.ci_path_runs(request, Ok(entry(attempt)), &ctx));
        assert_eq!(
            state(&mut tab)?.pinned,
            Some(RunKey {
                run: snapshot.run.info.id,
                attempt: 2
            })
        );
    }
    Ok(())
}
