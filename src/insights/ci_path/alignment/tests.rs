use super::super::workflow::FileJob;
use super::*;
use crate::ci::timing::RunInfo;

fn run(names: &[&str]) -> TimedRun {
    TimedRun {
        info: RunInfo {
            workflow_id: Some(8),
            ..RunInfo::default()
        },
        jobs: names
            .iter()
            .map(|name| TimedJob {
                name: (*name).into(),
                ..TimedJob::default()
            })
            .collect(),
        ..TimedRun::default()
    }
}

fn workflow(name: &str, matrix: bool) -> Workflow {
    Workflow {
        jobs: vec![FileJob {
            id: "test".into(),
            name: Some(name.into()),
            matrix,
            matrix_axes: if matrix {
                vec!["os".into()]
            } else {
                Vec::new()
            },
            ..FileJob::default()
        }],
        ..Workflow::default()
    }
}

#[test]
fn stable_job_ids_survive_title_renames_and_parallel_matrix_reordering() {
    let now = run(&["New ubuntu / 1.95", "New windows / 1.95"]);
    let before = run(&["Old 1.95 on windows", "Old 1.95 on ubuntu"]);
    let a = workflow("New ${{ matrix.os }} / ${{ matrix.rust }}", true);
    let b = workflow("Old ${{ matrix.rust }} on ${{ matrix.os }}", true);
    let pairs = jobs(&now, &before, Some(&a), Some(&b));
    assert_eq!(
        pairs
            .iter()
            .map(|pair| (pair.now, pair.before))
            .collect::<Vec<_>>(),
        [(Some(0), Some(1)), (Some(1), Some(0))]
    );
    assert!(pairs.iter().all(|pair| pair.evidence == Evidence::Inferred));
    let pairs = jobs(
        &run(&["New title"]),
        &run(&["Old title"]),
        Some(&workflow("New title", false)),
        Some(&workflow("Old title", false)),
    );
    assert_eq!(pairs[0].before, Some(0));
    let pairs = jobs(
        &run(&["New (ubuntu)", "New (windows)"]),
        &run(&["Old (windows)", "Old (ubuntu)"]),
        Some(&workflow("New", true)),
        Some(&workflow("Old", true)),
    );
    assert_eq!(pairs[0].before, Some(1));
}

#[test]
fn ambiguity_and_different_workflows_never_pair_jobs_by_row_order() {
    for (now, before) in [
        (run(&["test", "test"]), run(&["test", "test"])),
        (run(&["renamed"]), run(&["old"])),
    ] {
        assert!(
            jobs(&now, &before, None, None)
                .iter()
                .all(|pair| pair.now.is_none() || pair.before.is_none())
        );
    }
    let mut other = run(&["test"]);
    other.info.workflow_id = Some(9);
    assert_eq!(jobs(&run(&["test"]), &other, None, None).len(), 2);
    assert_eq!(
        bindings("${{ matrix.os }}-${{ matrix.rust }}", "a-b-c"),
        None
    );
    assert_eq!(bindings("${{ github.ref }}", "main"), None);
}

#[test]
fn duplicate_steps_use_recorded_positions_and_unmatched_renames_remain_unknown() {
    let steps_at = |entries: &[(&str, u64)]| {
        entries
            .iter()
            .map(|(name, number)| TimedStep {
                name: (*name).into(),
                number: *number,
                ..TimedStep::default()
            })
            .collect::<Vec<_>>()
    };
    let a = steps_at(&[("Run", 2), ("Run", 4), ("Unique", 8), ("Renamed", 9)]);
    let b = steps_at(&[("Run", 4), ("Run", 2), ("Unique", 7), ("Old", 9)]);
    let pairs = steps(&a, &b);
    assert_eq!(
        pairs.iter().map(|pair| pair.before).collect::<Vec<_>>(),
        [Some(1), Some(0), Some(2), None, Some(3)]
    );
    let unknown = steps_at(&[("Run", 0), ("Run", 0)]);
    assert!(
        steps(&unknown, &unknown)
            .iter()
            .all(|pair| pair.now.is_none() || pair.before.is_none())
    );
}
