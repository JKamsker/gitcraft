//! The diff's coverage gutter: the mapping's answer turned into marks, the chip a report of
//! another revision gets, and the real diff view painting marks from a fixture report.

use super::diff_gutter::{gutter_of, side_of, trust_of};
use crate::insights::Evidence;
use crate::insights::coverage::{Format, Found, Freshness, LineCov, LinesCoverage, Side};
use crate::repo::DiffKey;
use crate::repo::diff::coverage::{Mark, Trust};

const SHA: &str = "0123456789abcdef0123456789abcdef01234567";

fn found(recorded: Option<&str>, evidence: Evidence, stale: bool) -> Found {
    Found {
        path: "lcov.info".into(),
        format: Format::Lcov,
        tests: Vec::new(),
        per_test: false,
        files: 1,
        recorded: recorded.map(str::to_owned),
        freshness: Freshness {
            evidence,
            said: String::new(),
            stale: stale.then(|| "stale".to_owned()),
        },
        written: None,
    }
}

fn lines(found: Option<Found>, in_report: bool, lines: &[(u32, LineCov)]) -> LinesCoverage {
    LinesCoverage {
        found,
        sha: SHA.into(),
        in_report,
        index_differs: false,
        lines: lines.to_vec(),
    }
}

#[test]
fn trusted_lines_become_marks_and_lines_without_data_stay_distinct() -> Result<(), String> {
    let cov = lines(
        Some(found(Some(SHA), Evidence::Observed, false)),
        true,
        &[
            (1, LineCov::Covered(3)),
            (2, LineCov::Uncovered),
            (3, LineCov::NotInstrumented),
        ],
    );
    let g = gutter_of(&cov).ok_or("no report")?;
    assert_eq!(g.trust, Trust::Current);
    assert_eq!(g.marks.get(&1), Some(&Mark::Covered(3)));
    assert_eq!(g.marks.get(&2), Some(&Mark::Uncovered));
    // In the report but not instrumented: not executable, so no mark at all.
    assert_eq!(g.marks.get(&3), None);
    // A file the report doesn't have: no data, so unknown, never uncovered.
    let absent = lines(
        Some(found(Some(SHA), Evidence::Observed, false)),
        false,
        &[(1, LineCov::NotInstrumented)],
    );
    let g = gutter_of(&absent).ok_or("no report")?;
    assert_eq!(g.marks.get(&1), Some(&Mark::Unknown));
    // No report at all: no gutter, so no marks.
    assert_eq!(
        gutter_of(&lines(None, false, &[(1, LineCov::Unknown)])),
        None
    );
    Ok(())
}

#[test]
fn an_untrusted_report_says_why_and_marks_only_unknown() -> Result<(), String> {
    let unknown = [(1, LineCov::Unknown), (2, LineCov::Unknown)];
    let other = "fedcba9876543210";
    for (found, trust) in [
        (
            found(Some(other), Evidence::Unknown, true),
            Trust::Other(other.into()),
        ),
        (
            found(Some(SHA), Evidence::Unknown, true),
            Trust::EditedAfter,
        ),
        (
            found(Some(SHA), Evidence::Unknown, false),
            Trust::Undetermined,
        ),
        (found(None, Evidence::Unknown, false), Trust::NoRevision),
    ] {
        let g = gutter_of(&lines(Some(found), true, &unknown)).ok_or("no report")?;
        assert_eq!(g.trust, trust);
        assert!(g.marks.values().all(|m| *m == Mark::Unknown));
        assert_eq!(g.marks.len(), 2);
    }
    let mut staged = lines(
        Some(found(Some(SHA), Evidence::Inferred, false)),
        true,
        &unknown,
    );
    staged.index_differs = true;
    assert_eq!(trust_of(&staged), Some(Trust::IndexDiffers));
    Ok(())
}

#[test]
fn the_diffs_a_report_can_be_about_and_their_side() {
    let work = |area| {
        DiffKey::Work(crate::git::StatusFile {
            path: "a.rs".into(),
            old_path: None,
            code: 'M',
            area,
        })
    };
    assert_eq!(side_of(&work(crate::git::Area::Staged)), Some(Side::Index));
    assert_eq!(
        side_of(&work(crate::git::Area::Unstaged)),
        Some(Side::WorkTree)
    );
    assert_eq!(side_of(&work(crate::git::Area::Conflicted)), None);
    let range = DiffKey::Range {
        from: "a".into(),
        to: Some("b".into()),
        path: "a.rs".into(),
    };
    assert_eq!(side_of(&range), Some(Side::Commit("b".into())));
}
