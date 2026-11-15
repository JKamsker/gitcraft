//! Regression logs using cargo and ci/test-shards.py's actual output formats.
use super::*;

fn outcome(log: &LogEvidence, suite: &str, name: &str) -> Option<Outcome> {
    log.tests.get(&(suite.into(), name.into()))?.outcome()
}

#[test]
fn crates_with_the_same_source_and_test_path_remain_distinct() {
    let log = parse(include_str!("fixtures/cargo.log"), false);
    assert_eq!(log.report(), Report::Complete);
    assert_eq!(
        outcome(&log, "gitcraft (unittests src/lib.rs)", "tests::shared"),
        Some(Outcome::Failed)
    );
    assert_eq!(
        outcome(&log, "gitcraft_os (unittests src/lib.rs)", "tests::shared"),
        Some(Outcome::Passed)
    );
    assert_eq!(log.tests.len(), 2);
    let rebuilt =
        include_str!("fixtures/cargo.log").replace("0123456789abcdef", "fedcba9876543210");
    assert_eq!(parse(&rebuilt, false).tests, log.tests);
}

#[test]
fn windows_shards_and_serial_nocapture_keep_suite_and_delayed_outcomes() {
    let log = parse(include_str!("fixtures/windows.log"), false);
    assert_eq!(log.report(), Report::Complete);
    assert_eq!(
        outcome(&log, "gitcraft", "standalone::tests::saves"),
        Some(Outcome::Passed)
    );
    assert_eq!(
        outcome(&log, "gitcraft", "standalone::tests::opens"),
        Some(Outcome::Passed)
    );
    assert_eq!(
        outcome(&log, "gitcraft_os", "tests::paths"),
        Some(Outcome::Passed)
    );
    assert!(log.tests.keys().all(|(suite, _)| !suite.is_empty()));
}

#[test]
fn parallel_completion_does_not_discard_a_pending_serial_result() {
    let log = parse(
        "running 2 tests\ntest slow ... output\ntest fast ... ok\nFAILED\ntest result: FAILED. 1 passed; 1 failed\n",
        false,
    );
    assert_eq!(outcome(&log, "", "slow"), Some(Outcome::Failed));
    assert_eq!(outcome(&log, "", "fast"), Some(Outcome::Passed));
}

#[test]
fn cargo_fail_fast_and_crash_before_announcement_are_incomplete() {
    assert_eq!(
        parse("build output only", true).report(),
        Report::Incomplete
    );
    let full = include_str!("fixtures/cargo.log");
    assert_eq!(
        parse(
            &(full.to_owned() + "error: test failed, to rerun pass `--lib`\n"),
            false
        )
        .report(),
        Report::Incomplete
    );
    let crash = full.to_owned()
        + "     Running unittests src/main.rs (target/debug/deps/gitcraft-abcdef0123456789)\nerror: process didn't exit successfully (signal: 11, SIGSEGV)\n";
    assert_eq!(parse(&crash, false).report(), Report::Incomplete);
    assert_eq!(
        parse(
            "     Running unittests src/lib.rs (target/debug/deps/gitcraft-0123456789abcdef)\n",
            false
        )
        .report(),
        Report::Incomplete
    );
    assert_eq!(
        parse("gitcraft-0123456789abcdef.exe: core: 2 tests\n", false).report(),
        Report::Incomplete
    );
}
