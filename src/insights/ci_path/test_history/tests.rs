use super::*;
use crate::ci::timing::{RunInfo, TimedRun};
use crate::ci::{CheckState, Project};
use crate::insights::ci_path::{Compared, compare, critical_path, timing_deps};

const MATRIX: [&str; 2] = ["test (linux)", "test (windows)"];

/// A run of workflow 5 on `main` whose matrix jobs have ids `run * 100 + attempt * 10 + n`.
fn run(id: u64, attempt: u64, sha: &str, number: u64) -> TimedRun {
    TimedRun {
        info: RunInfo {
            id,
            attempt,
            number,
            workflow_id: Some(5),
            sha: sha.into(),
            branch: "main".into(),
            url: format!("https://github.com/o/r/actions/runs/{id}"),
            ..RunInfo::default()
        },
        jobs: MATRIX
            .iter()
            .enumerate()
            .map(|(n, name)| TimedJob {
                id: id * 100 + attempt * 10 + n as u64,
                name: (*name).into(),
                state: CheckState::Failed,
                url: format!("job-{id}-{attempt}-{n}"),
                ..TimedJob::default()
            })
            .collect(),
        more: false,
    }
}

fn explorer(now: TimedRun, before: Option<TimedRun>, history: Vec<TimedRun>) -> Explorer {
    let compared = before.map(|before| Compared {
        critical: critical_path(&before.jobs, &timing_deps(&before.jobs)),
        comparison: compare::compare(&now, &before),
        run: before,
    });
    let diagnostics = compared
        .as_ref()
        .map(|c| diagnostics::compare(&now, &c.run, None, None))
        .unwrap_or_default();
    let deps = timing_deps(&now.jobs);
    Explorer {
        project: Project {
            forge: crate::forge::Forge::GitHub,
            host: "github.com".into(),
            path: "o/r".into(),
        },
        critical: critical_path(&now.jobs, &deps),
        deps,
        baselines: Vec::new(),
        recent: Vec::new(),
        history: crate::ci::timing::history::History {
            runs: history,
            ..Default::default()
        },
        diagnostics,
        baseline_error: None,
        compared,
        now: 0,
        provenance: Default::default(),
        run: now,
    }
}

/// A complete libtest log with these outcomes.
fn log(tests: &[(&str, &str)]) -> String {
    let mut text = format!(
        "Running unittests src/lib.rs (target/debug/deps/g-1)\nrunning {} tests\n",
        tests.len()
    );
    for (name, result) in tests {
        text.push_str(&format!("test {name} ... {result}\n"));
    }
    text + "test result: ok.\n"
}

fn row<'a>(evidence: &'a JobEvidence, name: &str) -> Option<&'a TestRow> {
    evidence.rows.iter().find(|row| row.test.1 == name)
}

#[test]
fn reruns_at_one_sha_show_a_flaky_test_with_exact_attempt_links() {
    let sha = "abc";
    let (first, second, current) = (run(10, 1, sha, 4), run(10, 2, sha, 4), run(10, 3, sha, 4));
    let baseline = run(9, 1, "older", 3);
    let explorer = explorer(
        current.clone(),
        Some(baseline),
        vec![first, second, current],
    );
    // The windows matrix leg: its own job id in every attempt, never the linux one.
    let plan = plan(&explorer, 1).expect("job");
    assert_eq!(plan.now.job, 10 * 100 + 30 + 1);
    assert_eq!(plan.before.as_ref().map(|t| t.job), Some(9 * 100 + 10 + 1));
    assert_eq!(
        plan.history.iter().map(|t| t.job).collect::<Vec<_>>(),
        [1011, 1021]
    );
    let logs = HashMap::from([
        (
            1031,
            log(&[("flaky", "ok"), ("broken", "FAILED"), ("steady", "ok")]),
        ),
        (
            911,
            log(&[("flaky", "ok"), ("broken", "ok"), ("steady", "ok")]),
        ),
        (
            1011,
            log(&[("flaky", "FAILED"), ("broken", "FAILED"), ("steady", "ok")]),
        ),
        (
            1021,
            log(&[("flaky", "FAILED"), ("broken", "FAILED"), ("steady", "ok")]),
        ),
    ]);
    let mut reads = Vec::new();
    let evidence = read(plan, |job| {
        reads.push(job);
        logs.get(&job)
            .cloned()
            .map(|t| (t, false))
            .ok_or("gone".into())
    });
    assert_eq!(reads, [1031, 911, 1011, 1021]);
    let flaky = row(&evidence, "flaky").expect("flaky row");
    assert!(flaky.intermittent);
    assert_eq!(flaky.transition, Transition::Passed);
    assert_eq!(
        flaky
            .history
            .iter()
            .map(|o| (o.key.attempt, o.state))
            .collect::<Vec<_>>(),
        [
            (1, Some(CheckState::Failed)),
            (2, Some(CheckState::Failed)),
            (3, Some(CheckState::Passed))
        ]
    );
    assert_eq!(flaky.history[0].url, "job-10-1-1");
    let broken = row(&evidence, "broken").expect("broken row");
    assert_eq!(broken.transition, Transition::NewlyFailed);
    assert!(!broken.intermittent);
    assert_eq!(evidence.rows[0].test.1, "broken");
    assert_eq!((evidence.passed_both, evidence.unknown), (1, 0));
}

#[test]
fn incomplete_or_unread_logs_leave_tests_unknown_rather_than_fixed_or_green() {
    let mut current = run(20, 1, "abc", 2);
    current.jobs[0].state = CheckState::Running;
    let explorer = explorer(current, Some(run(19, 1, "old", 1)), Vec::new());
    let plan = plan(&explorer, 0).expect("job");
    assert!(!plan.now.finished);
    let evidence = read(plan, |job| match job {
        2010 => Ok((
            "Running unittests src/lib.rs (x)\nrunning 2 tests\ntest done ... ok\n".into(),
            false,
        )),
        _ => Ok((log(&[("done", "FAILED"), ("pending", "FAILED")]), false)),
    });
    assert_eq!(evidence.now.report(), Some(Report::Incomplete));
    let done = row(&evidence, "done").expect("done row");
    assert_eq!(done.transition, Transition::Fixed);
    // Absent from the incomplete log: unknown, never fixed.
    assert!(row(&evidence, "pending").is_none());
    assert_eq!(evidence.unknown, 1);
    assert!(evidence.differences.iter().any(|d| d.what == "test report"));
    let unread = read(super::plan(&explorer, 0).expect("job"), |_| {
        Err("410 expired".into())
    });
    assert!(unread.rows.is_empty() && unread.now.log.is_err());
    assert_eq!(unread.unknown + unread.passed_both, 0);
}

#[test]
fn cache_toolchain_and_error_differences_are_recorded_not_explained() {
    let explorer = explorer(run(30, 1, "abc", 2), Some(run(29, 1, "old", 1)), Vec::new());
    let plan = plan(&explorer, 0).expect("job");
    let evidence = read(plan, |job| {
        Ok((
            if job == 3010 {
                "Cache not found for input keys: k1\nrustc 1.96.0 (x)\n##[error]boom\n##[error]same\n"
            } else {
                "Cache restored from key: k1\nrustc 1.95.0 (x)\n##[error]same\n"
            }
            .to_owned(),
            false,
        ))
    });
    let what: Vec<_> = evidence
        .differences
        .iter()
        .map(|d| d.what.as_str())
        .collect();
    assert_eq!(what, ["cache", "toolchain"]);
    assert_eq!(evidence.differences[0].before, "hit k1");
    assert_eq!(evidence.differences[0].now, "miss k1");
    assert_eq!(evidence.new_errors, ["boom"]);
}

#[test]
fn ambiguous_matrix_legs_and_other_revisions_are_unaligned_not_guessed() {
    let current = run(40, 2, "abc", 2);
    let mut renamed = run(40, 1, "abc", 2);
    renamed.jobs[1].name = "test (linux)".into();
    let other_sha = run(41, 1, "zzz", 3);
    let explorer = explorer(current.clone(), None, vec![renamed, other_sha, current]);
    let plan = plan(&explorer, 1).expect("job");
    assert!(plan.baseline.is_none() && plan.before.is_none() && plan.history.is_empty());
    assert_eq!(
        plan.unaligned,
        [
            RunKey {
                run: 40,
                attempt: 1
            },
            RunKey {
                run: 41,
                attempt: 1
            }
        ]
    );
    assert!(super::plan(&explorer, 9).is_none());
}
