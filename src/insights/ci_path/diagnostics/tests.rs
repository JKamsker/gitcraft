use super::*;
use crate::ci::timing::{RunInfo, TimedStep};

fn run(id: u64, attempt: u64, state: CheckState) -> TimedRun {
    TimedRun {
        info: RunInfo {
            id,
            attempt,
            workflow_id: Some(5),
            sha: "same".into(),
            branch: "main".into(),
            ..RunInfo::default()
        },
        jobs: vec![TimedJob {
            name: "test (linux)".into(),
            state,
            url: format!("exact-job-{id}-{attempt}"),
            steps: vec![TimedStep {
                name: "Run tests".into(),
                state,
                number: 2,
                ..TimedStep::default()
            }],
            ..TimedJob::default()
        }],
        ..TimedRun::default()
    }
}

#[test]
fn failure_transitions_are_independent_from_duration_and_missing_evidence() {
    use CheckState::{Failed, Passed, Running, Skipped};
    for (now, before, expected) in [
        (Failed, Passed, Transition::NewlyFailed),
        (Passed, Failed, Transition::Fixed),
        (Failed, Failed, Transition::ConsistentlyFailed),
        (Passed, Passed, Transition::Passed),
        (Skipped, Passed, Transition::Unknown),
        (Running, Failed, Transition::Unknown),
    ] {
        let result = compare(&run(2, 1, now), &run(1, 1, before), None, None);
        assert_eq!(result[0].transition, expected);
        assert_eq!(result[0].steps[0].transition, expected);
    }
    let mut incomplete = run(2, 1, Passed);
    incomplete.jobs.clear();
    assert_eq!(
        compare(&incomplete, &run(1, 1, Failed), None, None)[0].transition,
        Transition::Unknown
    );
    assert!(UNKNOWN_EVIDENCE.contains("Individual test reports: unknown"));
}

#[test]
fn intermittent_evidence_requires_actual_pass_and_failure_at_the_same_sha() {
    use CheckState::{Failed, Passed};
    let current = run(10, 3, Passed);
    let history = History {
        runs: vec![
            run(10, 1, Failed),
            run(10, 2, Failed),
            current.clone(),
            run(11, 1, Passed),
        ],
        ..History::default()
    };
    for step in [None, Some(0)] {
        let observations = observations(&current, 0, step, &history);
        assert_eq!(
            observations
                .iter()
                .map(|seen| seen.state)
                .collect::<Vec<_>>(),
            [Some(Failed), Some(Failed), Some(Passed), Some(Passed)]
        );
        assert!(intermittent(&observations));
        assert_eq!(
            observations[0].key,
            RunKey {
                run: 10,
                attempt: 1
            }
        );
        assert!(observations[0].url.starts_with("exact-job-10-1"));
    }
    let mut wrong = run(9, 1, Failed);
    wrong.info.sha = "other revision".into();
    let history = History {
        runs: vec![wrong, current.clone()],
        ..History::default()
    };
    let observations = observations(&current, 0, None, &history);
    assert_eq!(observations[0].state, None);
    assert!(!intermittent(&observations));
}

#[test]
fn missing_or_ambiguous_jobs_and_steps_never_turn_into_passed_tests() {
    let current = run(10, 2, CheckState::Passed);
    let mut missing = run(10, 1, CheckState::Passed);
    missing.jobs[0].steps.clear();
    let mut duplicate = current.clone();
    duplicate.info.attempt = 4;
    duplicate.jobs.push(duplicate.jobs[0].clone());
    let history = History {
        runs: vec![missing, duplicate],
        ..History::default()
    };
    let seen = observations(&current, 0, Some(0), &history);
    assert_eq!(
        seen.iter().map(|seen| seen.state).collect::<Vec<_>>(),
        [None, None]
    );
    assert!(!intermittent(&seen));
}
