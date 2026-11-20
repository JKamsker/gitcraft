//! [`of_lines`] against real repositories: the diff gutter's view of a report.
use std::time::{Duration, SystemTime};

use super::*;

fn write(path: &Path, text: &str) -> Result<(), String> {
    std::fs::write(path, text).map_err(|e| e.to_string())
}

fn age(path: &Path, ago: Duration) -> Result<(), String> {
    std::fs::File::options()
        .write(true)
        .open(path)
        .and_then(|f| f.set_modified(SystemTime::now() - ago))
        .map_err(|e| e.to_string())
}

const LCOV: &str = "SF:src/lib.rs\nDA:1,3\nDA:2,0\nend_of_record\n";

#[test]
fn a_commit_s_lines_are_classified_only_by_a_report_of_that_commit() -> Result<(), String> {
    let repo = crate::git::fixture::Repo::new("coverage-lines")?;
    repo.commit("one", &[(".gitignore", Some("lcov.info*\n"))])?;
    let two = repo.commit("two", &[("src/lib.rs", Some("fn a() {\n    1\n}\n"))])?;
    // No report: every line unknown, and nothing found.
    let none = of_lines(
        repo.path(),
        &Side::Commit(two.clone()),
        "src/lib.rs",
        &[1, 2],
    )?;
    assert_eq!(none.found, None);
    assert!(none.lines.iter().all(|(_, c)| *c == LineCov::Unknown));

    write(&repo.path().join("lcov.info"), LCOV)?;
    write(&repo.path().join("lcov.info.sha"), &two)?;
    let side = Side::Commit(two.clone());
    let cov = of_lines(repo.path(), &side, "src/lib.rs", &[1, 2, 3])?;
    assert_eq!(cov.evidence(), Evidence::Observed);
    assert!(cov.in_report);
    assert_eq!(
        cov.lines,
        [
            (1, LineCov::Covered(3)),
            (2, LineCov::Uncovered),
            (3, LineCov::NotInstrumented)
        ]
    );
    // A file the report doesn't have.
    let other = of_lines(repo.path(), &side, "README.md", &[1])?;
    assert!(!other.in_report);

    // The next commit: the same report is of another revision, so it says nothing.
    let three = repo.commit("three", &[("src/lib.rs", Some("fn a() {\n    2\n}\n"))])?;
    let stale = of_lines(repo.path(), &Side::Commit(three), "src/lib.rs", &[1, 2])?;
    let found = stale.found.as_ref().ok_or("no report")?;
    assert_eq!(found.recorded.as_deref(), Some(two.as_str()));
    assert!(found.freshness.stale.is_some());
    assert_eq!(stale.evidence(), Evidence::Unknown);
    assert!(stale.lines.iter().all(|(_, c)| *c == LineCov::Unknown));
    Ok(())
}

#[test]
fn staged_lines_count_only_when_the_working_tree_holds_the_same_file() -> Result<(), String> {
    let repo = crate::git::fixture::Repo::new("coverage-index")?;
    let head = repo.commit(
        "one",
        &[
            ("src/lib.rs", Some("fn a() {}\n")),
            (".gitignore", Some("lcov.info*\n")),
        ],
    )?;
    let lib = repo.path().join("src/lib.rs");
    write(&lib, "fn a() {\n    1\n}\n")?;
    repo.git(&["add", "src/lib.rs"])?;
    age(&lib, Duration::from_secs(600))?;
    write(&repo.path().join("lcov.info"), LCOV)?;
    write(&repo.path().join("lcov.info.sha"), &head)?;
    age(&repo.path().join("lcov.info"), Duration::from_secs(300))?;
    let staged = of_lines(repo.path(), &Side::Index, "src/lib.rs", &[1, 2])?;
    assert!(!staged.index_differs);
    assert_eq!(staged.evidence(), Evidence::Inferred);
    assert_eq!(staged.lines[1], (2, LineCov::Uncovered));
    let work = of_lines(repo.path(), &Side::WorkTree, "src/lib.rs", &[1])?;
    assert_eq!(work.lines, [(1, LineCov::Covered(3))]);

    // Edited again after staging: the index isn't what the working tree (and the report) has.
    write(&lib, "fn a() {\n    1\n}\nfn b() {}\n")?;
    age(&lib, Duration::from_secs(400))?;
    let differs = of_lines(repo.path(), &Side::Index, "src/lib.rs", &[1, 2])?;
    assert!(differs.index_differs);
    assert_eq!(differs.evidence(), Evidence::Unknown);
    assert!(differs.lines.iter().all(|(_, c)| *c == LineCov::Unknown));
    // The working tree's own lines are still measured.
    let work = of_lines(repo.path(), &Side::WorkTree, "src/lib.rs", &[2])?;
    assert_eq!(work.lines, [(2, LineCov::Uncovered)]);
    Ok(())
}
