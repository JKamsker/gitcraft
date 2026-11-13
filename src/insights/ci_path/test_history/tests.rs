use super::*;
use crate::insights::ci_path::evidence::parse;
use crate::insights::ci_path::evidence::tests::{GREEN, NEXTEST, RED, TRUNCATED, id};

const LIB: &str = "unittests src/lib.rs";

fn log(attempt: u64, text: Option<&str>) -> JobLog {
    JobLog {
        key: RunKey { run: 5, attempt },
        job: attempt * 10,
        url: format!("https://github.com/o/r/actions/runs/5/job/{}", attempt * 10),
        read: text.map_or_else(
            || LogRead::Unavailable("HTTP 410: Gone".into()),
            |text| LogRead::Read(parse(text, true)),
        ),
    }
}

#[test]
fn test_changes_are_separate_from_job_status_and_ranked_newly_failed_first() {
    let c = compare(&log(2, Some(RED)).read, &log(1, Some(GREEN)).read);
    let changes: Vec<(String, Transition)> = c
        .changes
        .iter()
        .map(|c| (c.id.label(), c.transition))
        .collect();
    assert_eq!(
        changes,
        [
            (format!("ci::b ({LIB})"), Transition::NewlyFailed),
            // In the green log only: not run, or renamed, in the red one. Unknown, not fixed.
            (format!("ci::gone ({LIB})"), Transition::Unknown),
        ]
    );
    assert_eq!((c.passed, c.skipped), (2, 1));
    assert!(c.gaps.is_empty());
    let fixed = compare(&log(2, Some(GREEN)).read, &log(1, Some(RED)).read);
    assert_eq!(fixed.changes[0].transition, Transition::Fixed);
}

#[test]
fn missing_partial_and_expired_reports_make_outcomes_unknown_and_say_why() {
    let expired = compare(&log(2, Some(RED)).read, &log(1, None).read);
    assert_eq!(expired.passed, 0, "no pass is taken from a log that wasn't read");
    assert!(
        expired
            .changes
            .iter()
            .all(|c| c.transition == Transition::Unknown)
    );
    assert_eq!(
        expired.gaps,
        ["the earlier run: log unavailable (HTTP 410: Gone)"]
    );
    let cut = compare(&log(2, Some(TRUNCATED)).read, &log(1, Some(GREEN)).read);
    assert!(cut.gaps[0].starts_with("this run: test report partial"));
    let b = cut.changes.iter().find(|c| c.id == id(LIB, "ci::b"));
    assert_eq!(b.map(|c| c.transition), Some(Transition::Unknown));
    let none = compare(&log(2, Some("no tests here")).read, &log(1, Some(GREEN)).read);
    assert!(none.gaps[0].contains("no test report"));
    assert_eq!(none.passed, 0);
}

#[test]
fn flaky_tests_need_an_actual_failure_and_pass_at_the_sha_and_tell_their_story() {
    let history = [log(1, Some(RED)), log(2, Some(RED)), log(3, Some(GREEN))];
    let found = flaky(&history);
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].id, id(LIB, "ci::b"));
    assert_eq!(found[0].story(), "failed ×2, then passed");
    let urls: Vec<&str> = found[0].seen.iter().map(|s| s.url.as_str()).collect();
    assert_eq!(
        urls,
        [
            "https://github.com/o/r/actions/runs/5/job/10",
            "https://github.com/o/r/actions/runs/5/job/20",
            "https://github.com/o/r/actions/runs/5/job/30",
        ]
    );
    let gaps = [log(1, Some(RED)), log(2, None), log(3, Some(TRUNCATED)), log(4, Some(GREEN))];
    let found = flaky(&gaps);
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].story(), "failed, then unknown ×2, then passed");
    // Failing every time is not flaky; nor is one failure with nothing else known.
    assert!(flaky(&[log(1, Some(RED)), log(2, Some(RED))]).is_empty());
    assert!(flaky(&[log(1, Some(RED)), log(2, None)]).is_empty());
}

#[test]
fn a_retry_within_one_run_is_flake_evidence_too() {
    let found = flaky(&[log(1, Some(NEXTEST))]);
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].id, id("gitcraft", "ci::flaky"));
    assert_eq!(found[0].story(), "failed, then passed");
}
