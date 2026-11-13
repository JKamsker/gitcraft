use super::*;

const STAMP: &str = "2026-11-12T10:20:30.1234567Z ";

/// A GitHub job log the way the API returns it: timestamped, coloured, two cargo targets.
fn complete_log() -> String {
    [
        "##[group]Run Swatinem/rust-cache@v2",
        "Restored from cache key \"v0-rust-test-Linux-abc\"",
        "##[endgroup]",
        "rustc 1.95.0 (4d91de4e4 2026-10-02)",
        "cargo 1.95.0 (abc123 2026-09-30)",
        "     \u{1b}[1m\u{1b}[32mRunning\u{1b}[0m unittests src/lib.rs (target/debug/deps/gitcraft-0f3a)",
        "running 3 tests",
        "test a::passes ... \u{1b}[32mok\u{1b}[0m",
        "test a::flaky ... FAILED",
        "test a::slow ... ignored, needs a network",
        "test result: FAILED. 1 passed; 1 failed; 1 ignored; 0 measured; 0 filtered out",
        "     Running tests/cli.rs (target/debug/deps/cli-77aa)",
        "running 1 test",
        "test a::passes ... ok",
        "test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out",
        "##[error]Process completed with exit code 101.",
    ]
    .iter()
    .map(|line| format!("{STAMP}{line}\n"))
    .collect()
}

fn id(suite: &str, name: &str) -> TestId {
    (suite.to_owned(), name.to_owned())
}

#[test]
fn a_complete_log_names_each_test_by_target_with_caches_toolchains_and_errors() {
    let evidence = parse(&complete_log(), false);
    assert_eq!(evidence.report(), Report::Complete);
    let lib = "unittests src/lib.rs";
    assert_eq!(
        evidence.tests.get(&id(lib, "a::passes")),
        Some(&Some(Outcome::Passed))
    );
    assert_eq!(
        evidence.tests.get(&id(lib, "a::flaky")),
        Some(&Some(Outcome::Failed))
    );
    assert_eq!(
        evidence.tests.get(&id(lib, "a::slow")),
        Some(&Some(Outcome::Ignored))
    );
    // The same path in another target is another test, and the build hash is not identity.
    assert_eq!(
        evidence.tests.get(&id("tests/cli.rs", "a::passes")),
        Some(&Some(Outcome::Passed))
    );
    assert_eq!(evidence.tests.len(), 4);
    assert_eq!(
        evidence.caches,
        vec![Cache {
            hit: true,
            key: "v0-rust-test-Linux-abc".into()
        }]
    );
    assert_eq!(evidence.toolchains.len(), 2);
    assert!(
        evidence
            .toolchains
            .iter()
            .any(|t| t.starts_with("rustc 1.95.0"))
    );
    assert_eq!(
        evidence.errors,
        vec!["Process completed with exit code 101."]
    );
}

#[test]
fn incomplete_logs_never_claim_a_complete_report() {
    let full = complete_log();
    // A test binary that aborted (or a job cancelled) before its summary.
    let aborted = full
        .lines()
        .filter(|line| !line.contains("test result: ok."))
        .collect::<Vec<_>>()
        .join("\n");
    assert_eq!(parse(&aborted, false).report(), Report::Incomplete);
    // A binary that announced more results than the log holds.
    let short = full.replace("running 1 test", "running 2 tests");
    assert_eq!(parse(&short, false).report(), Report::Incomplete);
    // The download was cut at the size bound.
    assert_eq!(parse(&full, true).report(), Report::Incomplete);
    // Result lines without any binary header: what is seen counts, the rest is unknown.
    let orphan = parse("test x ... ok\n", false);
    assert_eq!(orphan.report(), Report::Incomplete);
    assert_eq!(orphan.tests.len(), 1);
    // No test output at all.
    let none = parse("##[group]Run cargo build\nCompiling gitcraft\n", false);
    assert_eq!(none.report(), Report::Missing);
    assert!(none.tests.is_empty() && none.caches.is_empty());
}

#[test]
fn conflicting_outcomes_in_one_log_are_ambiguous_and_odd_lines_are_ignored() {
    let log = [
        "Running unittests src/lib.rs (target/debug/deps/x-1)",
        "running 2 tests",
        "test retried ... FAILED",
        "test retried ... ok",
        "test bench ... bench:  12 ns/iter",
        "test noisy ... ok but it printed this",
        "test result: FAILED. 1 passed; 1 failed",
        "Cache not found for input keys: cargo-Linux-1, cargo-Linux",
        "No cache found.",
        "cargo build --release",
        "rustc is not a version",
    ]
    .join("\n");
    let evidence = parse(&log, false);
    let retried = id("unittests src/lib.rs", "retried");
    assert_eq!(evidence.tests.get(&retried), Some(&None));
    assert_eq!(evidence.tests.len(), 1);
    assert_eq!(evidence.report(), Report::Complete);
    assert_eq!(
        evidence.caches.iter().map(|c| c.hit).collect::<Vec<_>>(),
        [false, false]
    );
    assert!(evidence.toolchains.is_empty());
}

#[test]
fn carriage_returns_and_bounded_notes_keep_the_parse_small() {
    let evidence = parse("progress 10%\rtest a ... ok\r\n", false);
    assert_eq!(evidence.tests.len(), 1);
    let errors = (0..40)
        .map(|i| format!("##[error]failure {i}"))
        .collect::<Vec<_>>()
        .join("\n");
    assert_eq!(parse(&errors, false).errors.len(), MAX_NOTES);
}
