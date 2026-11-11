use super::*;

fn info(id: u64, attempt: u64) -> RunInfo {
    RunInfo {
        id,
        attempt,
        workflow_id: Some(7),
        sha: "revision".into(),
        branch: "topic/one".into(),
        ..RunInfo::default()
    }
}

#[test]
fn exact_attempt_endpoints_cannot_drift_to_latest_on_a_rerun() {
    let mut calls = Vec::new();
    let run = read_attempt(
        "owner/repo",
        RunKey {
            run: 99,
            attempt: 2,
        },
        |endpoint| {
            calls.push(endpoint.to_owned());
            Ok(if endpoint.contains("/jobs?") {
                r#"{"jobs":[],"total_count":1}"#.into()
            } else {
                r#"{"id":99,"run_attempt":2,"head_sha":"revision"}"#.into()
            })
        },
    )
    .unwrap();
    assert!(run.more);
    assert_eq!(
        calls,
        [
            "repos/owner/repo/actions/runs/99/attempts/2",
            "repos/owner/repo/actions/runs/99/attempts/2/jobs?per_page=100"
        ]
    );
    for answer in [
        r#"{"id":99,"run_attempt":3,"head_sha":"revision"}"#,
        r#"{"id":98,"run_attempt":2,"head_sha":"revision"}"#,
        r#"{"id":99,"run_attempt":2}"#,
    ] {
        assert!(
            read_attempt(
                "owner/repo",
                RunKey {
                    run: 99,
                    attempt: 2
                },
                |_| Ok(answer.into())
            )
            .is_err()
        );
    }
}

#[test]
fn recent_history_queries_pin_workflow_branch_and_revision_without_query_injection() {
    let mut run = info(99, 3);
    run.sha = "sha&other=1".into();
    assert_eq!(
        recent_endpoint("o/r", &run, true).unwrap(),
        "repos/o/r/actions/workflows/7/runs?per_page=8&branch=topic%2Fone&head_sha=sha%26other%3D1"
    );
    assert!(
        !recent_endpoint("o/r", &run, false)
            .unwrap()
            .contains("head_sha")
    );
    run.url = "https://github.com/o/r/actions/runs/99".into();
    assert_eq!(
        run.attempt_url(),
        "https://github.com/o/r/actions/runs/99/attempts/3"
    );
}

#[test]
fn real_history_retains_failures_missing_attempts_and_exact_same_sha_scope() {
    let current = TimedRun {
        info: info(99, 3),
        ..TimedRun::default()
    };
    let mut wrong_sha = info(98, 5);
    wrong_sha.sha = "other".into();
    let mut wrong_workflow = info(97, 5);
    wrong_workflow.workflow_id = Some(8);
    let mut calls = Vec::new();
    let history = collect(
        &current,
        &[info(99, 3), info(96, 1), wrong_sha, wrong_workflow],
        false,
        |key| {
            calls.push(key);
            if key.attempt == 2 {
                return Err("logs expired".into());
            }
            Ok(TimedRun {
                info: info(key.run, key.attempt),
                ..TimedRun::default()
            })
        },
    );
    assert_eq!(
        calls,
        [
            RunKey {
                run: 99,
                attempt: 2
            },
            RunKey {
                run: 99,
                attempt: 1
            },
            RunKey {
                run: 96,
                attempt: 1
            }
        ]
    );
    assert_eq!(history.runs.len(), 3);
    assert!(history.notes[0].contains("attempt 2: history unknown"));
    let history = collect(&current, &[info(99, 100)], false, |key| {
        Ok(TimedRun {
            info: info(key.run, key.attempt),
            ..TimedRun::default()
        })
    });
    assert_eq!(history.runs.len(), LIMIT);
    assert!(history.more);
    let mut failed_reads = 0;
    let history = collect(&current, &[info(99, u64::MAX)], false, |_| {
        failed_reads += 1;
        Err("unavailable".into())
    });
    assert_eq!(failed_reads, LIMIT - 1);
    assert!(history.more);
}

#[test]
fn failed_history_listing_preserves_the_already_observed_current_attempt() {
    let current = TimedRun {
        info: info(99, 3),
        ..TimedRun::default()
    };
    let history = from_recent(&current, Err("rate limited".into()), |_| {
        panic!("no attempts requested without a listing")
    });
    assert_eq!(history.runs, [current]);
    assert_eq!(history.notes, ["Same-SHA history unknown: rate limited"]);
}
