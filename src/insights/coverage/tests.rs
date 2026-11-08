use std::time::{Duration, SystemTime};

use super::*;

const LCOV: &str = "TN:unit\nSF:/home/ci/work/app/src/lib.rs\nDA:1,1\nDA:2,0\nDA:3,4\n\
                    LF:3\nLH:2\nend_of_record\nTN:integration\nSF:src/lib.rs\nDA:2,0\n\
                    DA:3,1\nend_of_record\nTN:\nSF:src/other.rs\nDA:1,0,abcdef\nend_of_record\n";

#[test]
fn lcov_sections_are_read_per_test_and_added_up() -> Result<(), String> {
    let report = lcov::parse(LCOV)?;
    assert_eq!(report.tests, ["unit", "integration"]);
    assert!(report.per_test());
    assert_eq!(report.files.len(), 3);
    let lib = &report.files[1];
    assert_eq!(lib.path, "src/lib.rs");
    assert_eq!(lib.lines, BTreeMap::from([(2, 0), (3, 1)]));
    assert_eq!(lib.tests.get(&1), Some(&BTreeSet::from([3])));
    assert_eq!(report.files[0].tests.get(&0), Some(&BTreeSet::from([1, 3])));
    // A record with no test name counts towards no test.
    assert!(report.files[2].tests.is_empty());
    // One test name for everything is the total, not per test.
    let one = lcov::parse("TN:all\nSF:a.rs\nDA:1,1\nend_of_record\n")?;
    assert!(!one.per_test());
    assert!(lcov::parse("hello\n").is_err());
    Ok(())
}

#[test]
fn cobertura_classes_are_read_without_counting_method_lines_twice() -> Result<(), String> {
    let xml = r#"<?xml version="1.0" ?>
<!DOCTYPE coverage SYSTEM "http://cobertura.sourceforge.net/xml/coverage-04.dtd">
<coverage line-rate="0.5" version="7.4">
  <!-- generated <class filename="fake.py"> -->
  <sources><source>/home/ci/app</source></sources>
  <packages><package name="pkg"><classes>
    <class name="a" filename="src/a&amp;b.py" line-rate="0.5">
      <methods><method name="f"><lines><line number="1" hits="9"/></lines></method></methods>
      <lines>
        <line number="1" hits="1"/>
        <line number="2" hits="0" branch="true"><conditions/></line>
      </lines>
    </class>
    <class name="b" filename='src/a&amp;b.py'><lines><line number="5" hits="3"/></lines></class>
  </classes></package></packages>
</coverage>"#;
    let report = cobertura::parse(xml)?;
    assert_eq!(report.sources, ["/home/ci/app"]);
    assert_eq!(report.files.len(), 1);
    assert_eq!(report.files[0].path, "src/a&b.py");
    assert_eq!(
        report.files[0].lines,
        BTreeMap::from([(1, 1), (2, 0), (5, 3)])
    );
    assert!(!report.per_test());
    assert!(cobertura::parse("<html></html>").is_err());
    assert_eq!(parse("coverage.xml", xml)?.format, Format::Cobertura);
    assert_eq!(parse("lcov.info", LCOV)?.format, Format::Lcov);
    Ok(())
}

#[test]
fn report_paths_match_exactly_under_the_root_or_by_their_end_when_made_elsewhere()
-> Result<(), String> {
    let report = lcov::parse(
        "SF:/repo/a.rs\nDA:1,1\nend_of_record\nSF:/ci/x/src/b.rs\nDA:1,1\nend_of_record\n\
         SF:.\\src\\c.rs\nDA:1,1\nend_of_record\nSF:src/d.rs\nDA:1,1\nend_of_record\n",
    )?;
    let at = |path: &str| {
        report
            .file(path, "/repo/")
            .map(|(f, s)| (f.path.clone(), s))
    };
    assert_eq!(at("a.rs"), Some(("/repo/a.rs".to_owned(), false)));
    assert_eq!(at("src/b.rs"), Some(("/ci/x/src/b.rs".to_owned(), true)));
    assert_eq!(at("src/c.rs"), Some((".\\src\\c.rs".to_owned(), false)));
    // A relative name is never matched by its end: `d.rs` is not `src/d.rs`.
    assert_eq!(at("d.rs"), None);
    assert_eq!(at("b.rs").map(|(_, s)| s), Some(true));
    let cobertura = cobertura::parse(
        "<coverage><sources><source>/repo/src</source></sources>\
         <class filename=\"e.py\"><lines><line number=\"1\" hits=\"0\"/></lines></class></coverage>",
    )?;
    assert!(cobertura.file("src/e.py", "/repo").is_some_and(|(_, s)| !s));
    Ok(())
}

#[test]
fn a_report_is_trusted_only_for_the_revision_its_sidecar_records() {
    let sha = "0123456789abcdef0123456789abcdef01234567";
    let commit = Ask::Commit(sha.to_owned());
    let matched = judge(Some("0123456789ab"), &commit, sha, Some(10), 20, None);
    assert_eq!(matched.evidence, Evidence::Observed);
    assert_eq!(matched.stale, None);
    let other = judge(Some("fedcba9876"), &commit, sha, Some(30), 20, None);
    assert_eq!(other.evidence, Evidence::Unknown);
    assert!(other.stale.is_some());
    // Without a sidecar: unknown, and stale when older than the commit.
    let unknown = judge(None, &commit, sha, Some(30), 20, None);
    assert_eq!(
        (unknown.evidence, unknown.stale.clone()),
        (Evidence::Unknown, None)
    );
    assert!(
        judge(None, &commit, sha, Some(10), 20, None)
            .stale
            .is_some()
    );
    // The working tree: HEAD recorded and the edits older than the report.
    let work = judge(Some(sha), &Ask::WorkTree, sha, Some(30), 20, Some(false));
    assert_eq!(work.evidence, Evidence::Inferred);
    let edited = judge(Some(sha), &Ask::WorkTree, sha, Some(30), 20, Some(true));
    assert_eq!(edited.evidence, Evidence::Unknown);
    assert!(edited.stale.is_some());
    assert_eq!(
        judge(Some(sha), &Ask::WorkTree, sha, None, 20, None).evidence,
        Evidence::Unknown
    );
    assert!(
        judge(None, &Ask::WorkTree, sha, Some(30), 20, Some(true))
            .stale
            .is_some()
    );
    assert_eq!(
        read_sidecar("ABCDEF1234 extra\n").as_deref(),
        Some("abcdef1234")
    );
    assert_eq!(read_sidecar("main\n"), None);
}

#[test]
fn changed_lines_are_classified_only_when_the_report_is_trusted() -> Result<(), String> {
    let report = lcov::parse(LCOV)?;
    let hunk = Hunk {
        old_start: 1,
        old_count: 0,
        new_start: 1,
        new_count: 4,
    };
    let mapped = map_file(
        "src/lib.rs",
        &[hunk],
        Some(&report),
        "/repo",
        Evidence::Observed,
    );
    let covs: Vec<LineCov> = mapped.hunks[0].lines.iter().map(|l| l.cov).collect();
    assert_eq!(
        covs,
        [
            LineCov::NotInstrumented,
            LineCov::Uncovered,
            LineCov::Covered(1),
            LineCov::NotInstrumented
        ]
    );
    assert_eq!(mapped.hunks[0].tests, [1]);
    assert_eq!(mapped.in_report, Some(true));
    let unknown = map_file(
        "src/lib.rs",
        &[hunk],
        Some(&report),
        "/repo",
        Evidence::Unknown,
    );
    assert!(
        unknown.hunks[0]
            .lines
            .iter()
            .all(|l| l.cov == LineCov::Unknown)
    );
    // What the report says is kept for the tooltip, not taken.
    assert_eq!(unknown.hunks[0].lines[1].hits, Some(0));
    assert!(unknown.hunks[0].tests.is_empty());
    let absent = map_file(
        "README.md",
        &[hunk],
        Some(&report),
        "/repo",
        Evidence::Observed,
    );
    assert_eq!(absent.in_report, Some(false));
    assert_eq!(absent.hunks[0].counts().not_instrumented, 4);
    let none = map_file("src/lib.rs", &[hunk], None, "/repo", Evidence::Observed);
    assert_eq!(none.in_report, None);
    assert_eq!(none.hunks[0].counts().unknown, 4);
    Ok(())
}

fn write(path: &Path, text: &str) -> Result<(), String> {
    std::fs::write(path, text).map_err(|e| e.to_string())
}

/// Sets when `path` was last modified to `ago` before now.
fn age(path: &Path, ago: Duration) -> Result<(), String> {
    std::fs::File::options()
        .write(true)
        .open(path)
        .and_then(|f| f.set_modified(SystemTime::now() - ago))
        .map_err(|e| e.to_string())
}

const SRC: &str = "fn a() {\n    1\n}\n";

#[test]
fn a_real_commit_is_mapped_onto_a_matching_report_and_goes_stale_after_a_commit()
-> Result<(), String> {
    let repo = crate::git::fixture::Repo::new("coverage")?;
    repo.git(&["config", "core.excludesFile", "/dev/null"])?;
    repo.commit(
        "one",
        &[
            ("src/lib.rs", Some("fn a() {}\n")),
            (".gitignore", Some("lcov.info*\n")),
        ],
    )?;
    let two = repo.commit(
        "two",
        &[("src/lib.rs", Some(SRC)), ("notes.txt", Some("x\n"))],
    )?;
    write(
        &repo.path().join("lcov.info"),
        "TN:t1\nSF:src/lib.rs\nDA:1,1\nDA:2,1\nend_of_record\n\
         TN:t2\nSF:src/lib.rs\nDA:1,1\nDA:2,0\nDA:3,0\nend_of_record\n",
    )?;
    write(&repo.path().join("lcov.info.sha"), &format!("{two}\n"))?;
    let cov = analyze(repo.path(), &Ask::Commit("HEAD".to_owned()), None)?;
    let report = cov.report.as_ref().ok_or("no report")?;
    assert_eq!(report.path, "lcov.info");
    assert_eq!(report.freshness.evidence, Evidence::Observed);
    assert!(report.per_test);
    let lib = cov
        .files
        .iter()
        .find(|f| f.path == "src/lib.rs")
        .ok_or("no lib")?;
    let covs: Vec<LineCov> = lib
        .hunks
        .iter()
        .flat_map(|h| &h.lines)
        .map(|l| l.cov)
        .collect();
    assert_eq!(
        covs,
        [LineCov::Covered(2), LineCov::Covered(1), LineCov::Uncovered]
    );
    assert_eq!(lib.hunks[0].tests, [0, 1]);
    let notes = cov
        .files
        .iter()
        .find(|f| f.path == "notes.txt")
        .ok_or("no notes")?;
    assert_eq!(notes.in_report, Some(false));
    assert_eq!(cov.provenance().stale, None);
    // A test lane result for the commit is found by its tree.
    crate::git::write_result(
        repo.path(),
        "default",
        &repo.tree("HEAD")?,
        Verdict::Good,
        false,
    )?;
    let with_lane = analyze(repo.path(), &Ask::Commit(two.clone()), None)?;
    assert_eq!(
        with_lane.lane,
        [LaneVerdict {
            name: "default".to_owned(),
            verdict: Verdict::Good
        }]
    );

    // A new commit: the report is of the one before, so it is stale and says nothing.
    repo.commit("three", &[("src/lib.rs", Some("fn a() {\n    2\n}\n"))])?;
    let after = analyze(repo.path(), &Ask::Commit("HEAD".to_owned()), None)?;
    let report = after.report.as_ref().ok_or("no report")?;
    assert_eq!(report.freshness.evidence, Evidence::Unknown);
    assert!(after.provenance().stale.is_some());
    assert!(
        after
            .files
            .iter()
            .flat_map(|f| &f.hunks)
            .flat_map(|h| &h.lines)
            .all(|l| l.cov == LineCov::Unknown)
    );
    // Without its sidecar and older than the commit: stale too.
    std::fs::remove_file(repo.path().join("lcov.info.sha")).map_err(|e| e.to_string())?;
    age(&repo.path().join("lcov.info"), Duration::from_secs(3600))?;
    let old = analyze(repo.path(), &Ask::Commit("HEAD".to_owned()), None)?;
    let report = old.report.as_ref().ok_or("no report")?;
    assert!(report.freshness.said.contains("revision unknown"));
    assert!(report.freshness.stale.is_some());
    Ok(())
}

#[test]
fn the_working_tree_is_mapped_when_its_edits_predate_a_report_of_head() -> Result<(), String> {
    let repo = crate::git::fixture::Repo::new("coverage-work")?;
    let head = repo.commit(
        "one",
        &[
            ("a.rs", Some("fn a() {}\n")),
            (".gitignore", Some("coverage/\n")),
        ],
    )?;
    write(&repo.path().join("a.rs"), "fn a() {}\nfn b() {}\n")?;
    write(&repo.path().join("new.rs"), "fn c() {}\n")?;
    age(&repo.path().join("a.rs"), Duration::from_secs(600))?;
    age(&repo.path().join("new.rs"), Duration::from_secs(600))?;
    std::fs::create_dir_all(repo.path().join("coverage")).map_err(|e| e.to_string())?;
    write(
        &repo.path().join("coverage/lcov.info"),
        "SF:a.rs\nDA:1,1\nDA:2,0\nend_of_record\nSF:new.rs\nDA:1,3\nend_of_record\n",
    )?;
    write(&repo.path().join("coverage/lcov.info.sha"), &head)?;
    age(
        &repo.path().join("coverage/lcov.info"),
        Duration::from_secs(300),
    )?;
    let cov = analyze(repo.path(), &Ask::WorkTree, None)?;
    let report = cov.report.as_ref().ok_or("no report")?;
    assert_eq!(report.path, "coverage/lcov.info");
    assert_eq!(report.freshness.evidence, Evidence::Inferred);
    let counts = cov.counts();
    assert_eq!((counts.covered, counts.uncovered), (1, 1));
    assert!(cov.files.iter().any(|f| f.path == "new.rs" && f.added));
    // Edited after the report: unknown and stale.
    write(
        &repo.path().join("a.rs"),
        "fn a() {}\nfn b() {}\nfn d() {}\n",
    )?;
    let edited = analyze(repo.path(), &Ask::WorkTree, None)?;
    let report = edited.report.as_ref().ok_or("no report")?;
    assert_eq!(report.freshness.evidence, Evidence::Unknown);
    assert!(report.freshness.stale.is_some());
    assert_eq!(edited.counts().covered, 0);
    Ok(())
}

#[test]
fn without_a_report_every_changed_line_is_unknown_and_where_it_looked_is_said() -> Result<(), String>
{
    let repo = crate::git::fixture::Repo::new("coverage-none")?;
    repo.commit("one", &[("a.rs", Some("fn a() {}\n"))])?;
    repo.commit("two", &[("a.rs", Some("fn a() {}\nfn b() {}\n"))])?;
    let cov = analyze(repo.path(), &Ask::Commit("HEAD".to_owned()), None)?;
    assert!(cov.report.is_none());
    assert!(cov.searched.iter().any(|s| s == "lcov.info"));
    assert_eq!(cov.counts().unknown, 1);
    assert_eq!(cov.counts().uncovered, 0);
    assert_eq!(cov.provenance().source, "no coverage report");
    // A path typed that isn't there is an error, not an empty report.
    assert!(
        analyze(
            repo.path(),
            &Ask::Commit("HEAD".to_owned()),
            Some("nope.info")
        )
        .is_err()
    );
    // A root commit is compared with nothing.
    let root = analyze(repo.path(), &Ask::Commit("HEAD~1".to_owned()), None)?;
    assert_eq!(root.files.len(), 1);
    Ok(())
}
