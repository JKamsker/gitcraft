//! Log evidence is read on request for the picked job and kept only while the job, pinned
//! attempt, baseline and same-SHA history it was read for are still the ones shown.

use super::*;
use crate::insights::ci_path::test_history;
use crate::repo::insights::{Kind, test_support as schedule};

fn state(tab: &mut RepoTab) -> Result<&mut State, String> {
    tab.insights
        .as_mut()
        .map(|view| &mut view.ci_path)
        .ok_or("Insights missing".into())
}

fn ready(tab: &mut RepoTab, ctx: &egui::Context) -> Result<test_history::JobEvidence, String> {
    crate::theme::install_fonts(ctx);
    let snapshot = tests::explorer();
    {
        let state = state(tab)?;
        state.unavailable = None;
        state.subject = Some(("HEAD".into(), snapshot.run.info.sha.clone()));
        state.run = Some((snapshot.project.clone(), snapshot.run.info.id));
        state.pinned = Some(RunKey::of(&snapshot.run.info));
        let request = state.runs.start();
        state.runs.accept(
            request,
            Ok(Runs {
                list: vec![(snapshot.project.clone(), Run::default())],
                errors: Vec::new(),
            }),
        );
        let request = state.explorer.start();
        assert!(state.explorer.accept(request, Ok(snapshot.clone())));
        state.selected = Some(1);
    }
    tests::draw(tab, ctx);
    let after = state(tab)?;
    assert!(after.explorer.ready().is_some(), "explorer kept");
    assert_eq!(after.selected, Some(1), "selection kept");
    assert!(after.run.is_some() && after.subject.is_some(), "run kept");
    let plan = test_history::plan(&snapshot, 1).ok_or("plan")?;
    Ok(test_history::read(plan, |job| {
        Ok((
            format!(
                "Running unittests src/lib.rs (x)\nrunning 1 test\ntest t{job} ... FAILED\ntest result: FAILED.\n"
            ),
            false,
        ))
    }))
}

#[test]
fn log_evidence_is_accepted_for_the_picked_job_and_drawn() -> Result<(), String> {
    let ctx = egui::Context::default();
    let mut tab = schedule::tab(&ctx, Kind::CiPath);
    let evidence = ready(&mut tab, &ctx)?;
    assert!(evidence.rows.iter().any(|row| row.test.1 == "t1"));
    let (_, jobs) = crate::repo::messages::record_jobs(|| {
        tab.ci_path_act(Act::ReadLogs(1), &ctx);
    });
    assert_eq!(jobs.len(), 1, "one bounded background read");
    let request = {
        let state = state(&mut tab)?;
        assert!(state.logs.loading);
        state.logs.start()
    };
    tab.ci_path_logs(request, Ok(evidence));
    assert!(state(&mut tab)?.logs.ready().is_some());
    tests::draw(&mut tab, &ctx);
    Ok(())
}

#[test]
fn changing_job_attempt_or_baseline_retires_log_evidence_and_late_answers() -> Result<(), String> {
    for change in ["job", "baseline", "plan"] {
        let ctx = egui::Context::default();
        let mut tab = schedule::tab(&ctx, Kind::CiPath);
        let mut evidence = ready(&mut tab, &ctx)?;
        let request = state(&mut tab)?.logs.start();
        crate::repo::messages::record_jobs(|| match change {
            "job" => tab.ci_path_act(Act::Select(Some(0)), &ctx),
            "baseline" => tab.ci_path_act(Act::PickBaseline(Baseline::None), &ctx),
            _ => {}
        });
        if change == "plan" {
            // The same request, answered for another baseline than the one shown.
            evidence.plan.baseline = Some(RunKey { run: 1, attempt: 7 });
        } else {
            assert!(!state(&mut tab)?.logs.loading, "{change}");
        }
        tab.ci_path_logs(request, Ok(evidence));
        assert!(state(&mut tab)?.logs.ready().is_none(), "{change}");
    }
    Ok(())
}
