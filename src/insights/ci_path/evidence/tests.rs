use super::*;

pub(crate) const RED: &str = include_str!("fixtures/red_libtest.txt");
pub(crate) const GREEN: &str = include_str!("fixtures/green_libtest.txt");
pub(crate) const NEXTEST: &str = include_str!("fixtures/nextest_retry.txt");
pub(crate) const TRUNCATED: &str = include_str!("fixtures/truncated_libtest.txt");

pub(crate) fn id(suite: &str, name: &str) -> TestId {
    TestId {
        suite: suite.to_owned(),
        name: name.to_owned(),
    }
}

const LIB: &str = "unittests src/lib.rs";

#[test]
fn libtest_outcomes_are_keyed_by_their_binary_and_counted_against_the_summaries() {
    let red = parse(RED, true);
    assert_eq!(red.report, Report::Complete);
    assert_eq!(red.outcome(&id(LIB, "ci::a")), Some(CheckState::Passed));
    assert_eq!(red.outcome(&id(LIB, "ci::b")), Some(CheckState::Failed));
    assert_eq!(red.outcome(&id(LIB, "ci::slow")), Some(CheckState::Skipped));
    // The same name in another binary is another test; the build hash isn't part of either.
    assert_eq!(
        red.outcome(&id("tests/cli.rs", "ci::a")),
        Some(CheckState::Passed)
    );
    assert_eq!(red.tests.len(), 4);
    assert_eq!(red.errors, ["Process completed with exit code 101."]);
}

#[test]
fn caches_toolchains_and_the_runner_image_are_read_as_recorded() {
    let red = parse(RED, true);
    assert_eq!(
        red.caches,
        [Cache {
            kind: CacheKind::Miss,
            key: "cargo-Linux-abc123".into()
        }]
    );
    assert_eq!(
        red.toolchain,
        [
            ("rustc".into(), "1.92.0 (ded5c06cf 2026-10-30)".into()),
            ("cargo".into(), "1.92.0 (2b3c4d5e6 2026-10-20)".into()),
        ]
    );
    assert_eq!(
        red.runner,
        [
            ("runner version".into(), "2.320.0".into()),
            ("operating system".into(), "Ubuntu 24.04.1 LTS".into()),
            ("image".into(), "ubuntu-24.04".into()),
            ("image version".into(), "20261101.1.0".into()),
        ],
        "the provisioner's version isn't the image's"
    );
    let green = parse(GREEN, true);
    let kinds: Vec<_> = green.caches.iter().map(|c| c.kind).collect();
    assert_eq!(kinds, [CacheKind::Hit, CacheKind::Saved]);
    let nextest = parse(NEXTEST, true);
    assert_eq!(
        nextest.caches,
        [Cache {
            kind: CacheKind::Hit,
            key: "v0-rust-test-Linux-x64-1a2b".into()
        }]
    );
}

#[test]
fn nextest_retries_are_kept_in_order_and_its_repeated_summary_lines_are_not_new_runs() {
    let log = parse(NEXTEST, true);
    assert_eq!(log.report, Report::Complete);
    assert_eq!(
        log.tests.get(&id("gitcraft", "ci::flaky")),
        Some(&vec![CheckState::Failed, CheckState::Passed])
    );
    assert_eq!(
        log.tests.get(&id("gitcraft", "ci::crash")),
        Some(&vec![CheckState::Failed])
    );
    assert_eq!(log.outcome(&id("gitcraft", "ci::a")), Some(CheckState::Passed));
}

#[test]
fn incomplete_logs_are_partial_and_logs_without_tests_have_no_report() {
    let cut = parse(TRUNCATED, true);
    assert!(matches!(cut.report, Report::Partial(ref why) if why.contains("no summary")));
    assert_eq!(cut.outcome(&id(LIB, "ci::a")), Some(CheckState::Passed));
    assert_eq!(cut.outcome(&id(LIB, "ci::b")), None, "never a pass");
    let running = parse(GREEN, false);
    assert!(matches!(running.report, Report::Partial(ref why) if why.contains("hadn't finished")));
    let missing = "2026-11-02T22:00:00.0000000Z test result: ok. 2 passed; 0 failed\n";
    let short = parse(missing, true);
    assert!(matches!(short.report, Report::Missing(_)));
    let without_a: Vec<&str> = RED
        .lines()
        .filter(|line| !line.ends_with("test ci::a ... ok"))
        .collect();
    let mismatch = parse(&without_a.join("\n"), true);
    assert!(
        matches!(mismatch.report, Report::Partial(ref why) if why.contains("count 4 tests, 2")),
        "{:?}",
        mismatch.report
    );
    let empty = parse("", true);
    assert!(matches!(empty.report, Report::Missing(_)));
    assert!(empty.caches.is_empty() && empty.toolchain.is_empty() && empty.runner.is_empty());
}

#[test]
fn lines_without_timestamps_and_with_crlf_read_the_same() {
    let plain: String = GREEN
        .lines()
        .map(|line| format!("{}\r\n", line.split_once(' ').map_or(line, |(_, r)| r)))
        .collect();
    assert_eq!(parse(&plain, true), parse(GREEN, true));
    assert_eq!(content("\u{feff}2026-11-02T22:00:00.1Z rustc 1.0.0"), "rustc 1.0.0");
    assert_eq!(content("2026-11-02T22:00:00.1Z"), "");
    assert_eq!(content("Image: x"), "Image: x");
}
