use super::measure::Reading;
use super::*;
use crate::git;
use crate::insights::history::{self, FileChange, HistCommit};

fn change(path: &str, added: u32, deleted: u32) -> FileChange {
    FileChange {
        path: path.to_owned(),
        old_path: None,
        added: Some(added),
        deleted: Some(deleted),
    }
}

fn commit(hash: &str, files: Vec<FileChange>) -> HistCommit {
    HistCommit {
        hash: hash.to_owned(),
        parents: vec!["p".to_owned()],
        time: 0,
        author: "Ann".to_owned(),
        subject: hash.to_owned(),
        files,
    }
}

/// c1…c6, oldest first in the text, newest first in the history; c3 renames old.rs.
fn sample() -> History {
    let mut rename = change("new.rs", 1, 0);
    rename.old_path = Some("old.rs".to_owned());
    let commits = vec![
        commit(
            "c6",
            vec![change("src/y.rs", 1, 0), change("src/x.rs", 1, 1)],
        ),
        commit("c5", vec![change("new.rs", 2, 2)]),
        commit("c4", vec![change("src/x.rs", 3, 0)]),
        commit("c3", vec![rename]),
        commit("c2", vec![change("README.md", 1, 1)]),
        commit(
            "c1",
            vec![change("old.rs", 5, 0), change("README.md", 1, 0)],
        ),
    ];
    History {
        rev: "HEAD".to_owned(),
        tip: "c6".to_owned(),
        limit: 10,
        commits,
        truncated: false,
    }
}

fn opts(samples: usize) -> Options {
    Options {
        samples,
        ..Options::default()
    }
}

#[test]
fn samples_spread_evenly_and_end_at_the_tip() {
    assert_eq!(sample_points(10, 4), [2, 4, 7, 9]);
    assert_eq!(sample_points(3, 16), [0, 1, 2]);
    assert_eq!(sample_points(16, 16), (0..16).collect::<Vec<_>>());
    assert!(sample_points(0, 16).is_empty());
    let p = plan(&sample(), &opts(3));
    let hashes: Vec<&str> = p.samples.iter().map(|s| s.hash.as_str()).collect();
    assert_eq!(hashes, ["c2", "c4", "c6"]);
    // History indices of c1 and c2, oldest first.
    assert_eq!(p.windows[0], [5, 4]);
    assert_eq!(p.windows.iter().map(Vec::len).sum::<usize>(), 6);
}

#[test]
fn window_churn_follows_a_file_through_its_rename() {
    let p = plan(&sample(), &opts(3));
    let names: Vec<&str> = p.rows.iter().map(|r| r.name.as_str()).collect();
    assert_eq!(names, ["new.rs", "src/x.rs", "README.md", "src/y.rs"]);
    let renamed = &p.rows[0];
    assert_eq!(renamed.churn, [5, 1, 4]);
    assert_eq!(renamed.total, 10);
    // Before c3 it was old.rs: that is what the first sample reads.
    assert_eq!(renamed.paths, ["old.rs", "new.rs", "new.rs"]);
    assert_eq!(renamed.changes[0], [(5, "old.rs".to_owned())]);
    assert_eq!(p.rows[1].churn, [0, 3, 2]);
}

#[test]
fn folders_add_up_their_files_and_the_filter_and_limit_cut_rows() {
    let p = plan(
        &sample(),
        &Options {
            group: Some(1),
            ..opts(3)
        },
    );
    let names: Vec<&str> = p.rows.iter().map(|r| r.shown()).collect();
    assert_eq!(names, ["(root)", "src"]);
    assert_eq!(p.rows[0].churn, [8, 1, 4]);
    assert_eq!(p.rows[1].paths, ["src", "src", "src"]);
    let filtered = plan(
        &sample(),
        &Options {
            filter: "SRC/".to_owned(),
            max_rows: 1,
            ..opts(3)
        },
    );
    assert_eq!(filtered.rows.len(), 1);
    assert_eq!(filtered.rows[0].name, "src/x.rs");
    assert_eq!(filtered.rows_cut, 1);
    assert!(plan(&History::default(), &opts(3)).samples.is_empty());
}

fn point(complexity: Option<u64>, churn: u64) -> Point {
    Point {
        complexity,
        churn,
        complete: true,
        ..Point::default()
    }
}

fn points(cx: [u64; 4], churn: [u64; 4]) -> Vec<Point> {
    (0..4).map(|i| point(Some(cx[i]), churn[i])).collect()
}

#[test]
fn a_hotspot_rises_in_complexity_and_churn_both() {
    assert!(hotspot(&points([1, 2, 2, 3], [1, 1, 3, 3])));
    // Earlier points don't matter.
    let mut longer = vec![point(None, 100)];
    longer.extend(points([1, 2, 2, 3], [1, 1, 3, 3]));
    assert!(hotspot(&longer));
    assert!(!hotspot(&points([3, 2, 2, 4], [1, 1, 3, 3])));
    assert!(!hotspot(&points([2, 2, 2, 2], [1, 1, 3, 3])));
    assert!(!hotspot(&points([1, 2, 3, 4], [5, 5, 1, 1])));
    let mut unmeasured = points([1, 2, 3, 4], [1, 1, 3, 3]);
    unmeasured[2].complexity = None;
    assert!(!hotspot(&unmeasured));
    let mut partial = points([1, 2, 3, 4], [1, 1, 3, 3]);
    partial[3].complete = false;
    assert!(!hotspot(&partial));
    assert!(!hotspot(&points([1, 2, 3, 4], [1, 1, 3, 3])[1..]));
}

#[test]
fn assembling_says_what_was_not_read_and_what_is_not_measured() {
    let p = plan(&sample(), &opts(3));
    let rows = p.rows.len();
    let reading = |files, read, binary, measured, lines| Reading {
        files,
        read,
        binary,
        measured,
        lines,
        branches: 7,
    };
    let mut readings = vec![vec![reading(1, 1, 0, 1, 10); 3]; rows];
    // README.md: there, read, not a language measured; then a binary; then not read.
    readings[2] = vec![
        reading(1, 1, 0, 0, 2),
        reading(1, 1, 1, 0, 0),
        reading(1, 0, 0, 0, 0),
    ];
    let t = assemble(
        p,
        Measured {
            readings,
            tests: vec![None, Some(Verdict::Bad), None],
            tests_error: None,
            cut: vec!["budget".to_owned()],
        },
    );
    assert_eq!(t.rows[0].points[0].complexity, Some(7));
    let readme = &t.rows[2].points;
    assert_eq!((readme[0].size, readme[0].complexity), (Some(2), None));
    assert_eq!(readme[1].size, None);
    assert!(!readme[2].complete);
    assert_eq!(t.samples[1].verdict, Some(Verdict::Bad));
    assert_eq!(t.samples[0].verdict, None);
    assert_eq!(t.notes, ["budget"]);
    let commits = t.rows[0].commits_in(0);
    assert_eq!(commits, [(5, vec!["old.rs".to_owned()])]);
}

#[test]
fn a_real_repository_is_measured_at_its_samples() -> Result<(), String> {
    let repo = git::fixture::Repo::new("insights-churn")?;
    repo.commit(
        "one",
        &[
            ("src/lib.rs", Some("fn a() {}\n")),
            ("README.md", Some("hi\n")),
        ],
    )?;
    let second = repo.commit("two", &[("src/lib.rs", Some("fn a() { if x {} }\n"))])?;
    repo.commit(
        "three",
        &[
            (
                "src/lib.rs",
                Some("fn a() { if x && y {} }\n// if, in a comment\n"),
            ),
            ("README.md", Some("hi\nthere\n")),
        ],
    )?;
    repo.commit(
        "four",
        &[(
            "src/lib.rs",
            Some("fn a() { if x && y { for i in z {} } }\n"),
        )],
    )?;
    git::write_result(
        repo.path(),
        git::DEFAULT_TEST,
        &repo.tree(&second)?,
        Verdict::Good,
        false,
    )?;
    let h = history::read(repo.path(), "HEAD", 50)?;
    let p = plan(&h, &opts(4));
    let measured = measure(repo.path(), &p)?;
    let t = assemble(p, measured);
    assert!(t.notes.is_empty(), "{:?}", t.notes);
    assert_eq!(t.rows[0].name, "src/lib.rs");
    let lib = &t.rows[0];
    let sizes: Vec<Option<u64>> = lib.points.iter().map(|p| p.size).collect();
    assert_eq!(sizes, [Some(1), Some(1), Some(2), Some(1)]);
    let cx: Vec<Option<u64>> = lib.points.iter().map(|p| p.complexity).collect();
    assert_eq!(cx, [Some(0), Some(1), Some(2), Some(3)]);
    assert!(lib.hotspot);
    let readme = t
        .rows
        .iter()
        .find(|r| r.name == "README.md")
        .ok_or("a README row")?;
    assert!(readme.points.iter().all(|p| p.complexity.is_none()));
    assert_eq!(readme.points[3].size, Some(2));
    let verdicts: Vec<Option<Verdict>> = t.samples.iter().map(|s| s.verdict).collect();
    assert_eq!(verdicts, [None, Some(Verdict::Good), None, None]);

    // Folders: the root's own files apart from src's.
    let folders = plan(
        &h,
        &Options {
            group: Some(1),
            ..opts(4)
        },
    );
    let ft = assemble(folders.clone(), measure(repo.path(), &folders)?);
    let root = ft
        .rows
        .iter()
        .find(|r| r.name.is_empty())
        .ok_or("a root row")?;
    assert_eq!(root.points[0].size, Some(1));
    assert_eq!(root.points[0].files, 1);

    // A new commit: the last sample moves to it, and what it holds is read there.
    let fifth = repo.commit("five", &[("src/lib.rs", Some("fn a() {}\n"))])?;
    let h2 = history::read(repo.path(), "HEAD", 50)?;
    let p2 = plan(&h2, &opts(4));
    assert_eq!(
        p2.samples.last().map(|s| s.hash.as_str()),
        Some(fifth.as_str())
    );
    let t2 = assemble(p2.clone(), measure(repo.path(), &p2)?);
    assert_ne!(t, t2);
    assert_eq!(t2.rows[0].points.last().and_then(|p| p.complexity), Some(0));
    assert!(!t2.rows[0].hotspot);

    // Without the repository: an error, not an empty timeline.
    let missing = std::path::Path::new("/nonexistent/insights-churn");
    assert!(measure(missing, &p2).is_err());
    Ok(())
}

#[test]
fn without_test_results_every_sample_is_unknown() -> Result<(), String> {
    let repo = git::fixture::Repo::new("insights-churn-untested")?;
    repo.commit("one", &[("a.c", Some("int main() { return 0; }\n"))])?;
    let h = history::read(repo.path(), "HEAD", 50)?;
    let p = plan(&h, &opts(4));
    let t = assemble(p.clone(), measure(repo.path(), &p)?);
    assert!(t.samples.iter().all(|s| s.verdict.is_none()));
    assert!(t.tests_error.is_none());
    assert_eq!(t.rows[0].points[0].complexity, Some(0));
    Ok(())
}
