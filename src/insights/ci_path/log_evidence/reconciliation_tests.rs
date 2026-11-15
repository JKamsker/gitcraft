//! Contradictory output, summary validation, binary boundaries and allocation budgets.
use super::*;

fn seen(log: &LogEvidence, suite: &str, name: &str) -> Seen {
    log.tests
        .get(&(suite.into(), name.into()))
        .copied()
        .unwrap_or_default()
}

#[test]
fn coderabbit_exact_stdout_sample_never_turns_failure_into_a_pass() {
    let evidence = parse(include_str!("fixtures/stdout-outcome.log"), false);
    let sample = seen(&evidence, "", "sample");
    assert_eq!(sample.outcome(), None);
    assert!(sample.failed());
    assert_eq!(evidence.report(), Report::Incomplete);
}

#[test]
fn panic_named_failures_and_summary_totals_contradict_provisional_passes() {
    let prefix = "running 1 test\ntest x ... diagnostic\nok\nthread 'x' panicked at src/lib.rs:9:1:\nnot okay\n";
    for rest in [
        "FAILED\nfailures:\n    x\ntest result: FAILED. 0 passed; 1 failed; 0 ignored\n",
        "failures:\n    x\ntest result: FAILED. 0 passed; 1 failed; 0 ignored\n",
        "test result: FAILED. 0 passed; 1 failed; 0 ignored\n",
    ] {
        let evidence = parse(&(prefix.to_owned() + rest), false);
        assert_eq!(seen(&evidence, "", "x").outcome(), None);
        assert_eq!(evidence.report(), Report::Incomplete);
    }
    let direct = parse(
        "running 1 test\ntest x ... ok\ntest result: FAILED. 0 passed; 1 failed; 0 ignored\n",
        false,
    );
    assert_eq!(seen(&direct, "", "x").outcome(), None);
}

#[test]
fn headerless_binary_does_not_inherit_the_previous_cargo_identity() {
    let log = "Running unittests src/lib.rs (target/debug/deps/first-0123456789abcdef)\nrunning 1 test\ntest shared ... ok\ntest result: ok. 1 passed; 0 failed; 0 ignored\nrunning 1 test\ntest shared ... FAILED\ntest result: FAILED. 0 passed; 1 failed; 0 ignored\n";
    let evidence = parse(log, false);
    assert_eq!(
        seen(&evidence, "first (unittests src/lib.rs)", "shared").outcome(),
        Some(Outcome::Passed)
    );
    assert_eq!(
        seen(&evidence, "", "shared").outcome(),
        Some(Outcome::Failed)
    );
    assert_eq!(evidence.tests.len(), 2);
    assert_eq!(evidence.report(), Report::Complete);
}

#[test]
fn successful_shards_require_the_final_marker_and_fail_fast_is_partial() {
    let success = include_str!("fixtures/windows.log");
    let fail_fast = parse(include_str!("fixtures/windows-fail-fast.log"), false);
    assert_eq!(fail_fast.report(), Report::Incomplete);
    assert_eq!(
        seen(&fail_fast, "gitcraft", "standalone::tests::saves").outcome(),
        Some(Outcome::Failed)
    );
    assert!(
        !fail_fast
            .tests
            .keys()
            .any(|(suite, _)| suite == "gitcraft_os")
    );
    assert_eq!(parse(success, false).report(), Report::Complete);
    assert_eq!(
        parse(&success.replace("Passed 3 tests in core", ""), false).report(),
        Report::Incomplete
    );
}

#[test]
fn test_count_identity_and_line_budgets_are_bounded_and_marked_incomplete() {
    let log: String = (0..=MAX_TESTS)
        .map(|i| format!("test t{i} ... ok\n"))
        .collect();
    let evidence = parse(&log, false);
    assert_eq!(evidence.tests.len(), MAX_TESTS);
    assert_eq!(evidence.report(), Report::Incomplete);
    let small = parse_bounded(&log, false, 2);
    assert_eq!(small.tests.len(), 2);
    assert_eq!(small.report(), Report::Incomplete);
    assert_eq!(parse_bounded(&log, false, 0).report(), Report::Incomplete);
    for text in [
        format!("test {} ... ok\n", "x".repeat(MAX_NAME_BYTES + 1)),
        "x".repeat(MAX_PARSE_LINE + 1),
    ] {
        let evidence = parse(&text, false);
        assert!(evidence.tests.is_empty());
        assert_eq!(evidence.report(), Report::Incomplete);
    }
}
