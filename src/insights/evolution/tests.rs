use super::*;
use crate::git;
use crate::insights::history::{self, FileChange, HistCommit};

const DAY: i64 = 86_400;

fn change(path: &str, added: Option<u32>, deleted: Option<u32>) -> FileChange {
    FileChange {
        path: path.to_owned(),
        old_path: None,
        added,
        deleted,
    }
}

fn commit(hash: &str, time: i64, files: Vec<FileChange>) -> HistCommit {
    HistCommit {
        hash: hash.to_owned(),
        parents: vec!["p".to_owned()],
        time,
        author: "Ann".to_owned(),
        subject: format!("subject {hash}"),
        files,
    }
}

/// Newest first, as the history lists them.
fn history(commits: Vec<HistCommit>, truncated: bool) -> History {
    History {
        rev: "HEAD".to_owned(),
        tip: commits.first().map(|c| c.hash.clone()).unwrap_or_default(),
        limit: commits.len(),
        commits,
        truncated,
    }
}

fn sample() -> History {
    let mut merge = commit("m", 10 * DAY + 5, Vec::new());
    merge.parents.push("q".to_owned());
    history(
        vec![
            merge,
            commit(
                "c4",
                10 * DAY,
                vec![
                    change("src/a.rs", Some(1), Some(9)),
                    change("src/sub/b.rs", Some(2), Some(0)),
                ],
            ),
            commit("c3", 3 * DAY + 100, vec![change("img.png", None, None)]),
            commit(
                "c2",
                3 * DAY,
                vec![
                    change("src/a.rs", Some(5), Some(0)),
                    change("src/sub/b.rs", Some(1), Some(1)),
                ],
            ),
            commit("c1", DAY, vec![change("src/a.rs", Some(10), Some(0))]),
        ],
        false,
    )
}

#[test]
fn commits_run_oldest_to_newest_and_merges_make_no_column() {
    let h = sample();
    let m = build(&h, &Options::default());
    let labels: Vec<&str> = m.columns.iter().map(|c| &c.label[..2]).collect();
    assert_eq!(labels, ["c1", "c2", "c3", "c4"]);
    // Sorted by lines changed: a.rs 25, b.rs 4, img.png 1.
    let names: Vec<&str> = m.rows.iter().map(|r| r.name.as_str()).collect();
    assert_eq!(names, ["src/a.rs", "src/sub/b.rs", "img.png"]);
    assert_eq!(m.rows[0].commits, 3);
    assert_eq!(m.cells[&(0, 3)].lines, 10);
    assert!(m.cells[&(2, 2)].binary_only());
    assert!(!m.boundary);
    assert_eq!(m.max_lines, 10);
}

#[test]
fn folders_group_their_files_and_count_each_commit_once() {
    let h = sample();
    let m = build(
        &h,
        &Options {
            group: Some(1),
            ..Options::default()
        },
    );
    let names: Vec<&str> = m.rows.iter().map(|r| r.shown()).collect();
    assert_eq!(names, ["src", "(root)"]);
    // c2 changed two files of src: two changes, one commit.
    let cell = &m.cells[&(0, 1)];
    assert_eq!(cell.changes.len(), 2);
    assert_eq!(cell.commits().len(), 1);
    assert_eq!(m.rows[0].commits, 3);
    // Deeper is a zoom: src and src/sub apart.
    let deeper = build(
        &h,
        &Options {
            group: Some(2),
            ..Options::default()
        },
    );
    assert!(deeper.rows.iter().any(|r| r.name == "src/sub"));
    assert!(deeper.rows.iter().any(|r| r.name == "src"));
}

#[test]
fn time_buckets_hold_every_commit_of_their_span_once() {
    let h = sample();
    let m = build(
        &h,
        &Options {
            axis: Axis::Day,
            ..Options::default()
        },
    );
    // Day 1 to day 10, gaps included.
    assert_eq!(m.columns.len(), 10);
    assert_eq!(m.columns[0].label, "1970-01-02");
    assert_eq!(m.columns[2].commits.len(), 2);
    // Day 3 has two commits; a.rs changed in one of them.
    let a_day3 = &m.cells[&(0, 2)];
    assert_eq!(a_day3.commits().len(), 1);
    let weeks = build(
        &h,
        &Options {
            axis: Axis::Week,
            group: Some(1),
            ..Options::default()
        },
    );
    // Thursday 1 Jan 1970 starts the week of Monday 29 Dec 1969; day 10 is in the next week.
    assert_eq!(weeks.columns[0].label, "wk 1969-12-29");
    assert_eq!(weeks.columns.len(), 2);
    assert_eq!(weeks.cells[&(0, 0)].commits().len(), 2);
    assert_eq!(bucket_label(bucket(0, Axis::Month), Axis::Month), "1970-01");
    assert_eq!(
        bucket_label(bucket(1_762_560_000, Axis::Day), Axis::Day),
        "2025-11-08"
    );
}

#[test]
fn a_cut_history_marks_its_boundary_and_its_oldest_bucket() {
    let mut h = sample();
    h.truncated = true;
    let m = build(
        &h,
        &Options {
            axis: Axis::Month,
            ..Options::default()
        },
    );
    assert!(m.boundary);
    assert!(m.columns[0].cut);
    assert!(m.columns[0].label.ends_with("(cut)"));
}

#[test]
fn the_filter_and_the_row_limit_cut_rows_and_say_how_many() {
    let h = sample();
    let m = build(
        &h,
        &Options {
            max_rows: 1,
            ..Options::default()
        },
    );
    assert_eq!(m.rows.len(), 1);
    assert_eq!(m.rows_cut, 2);
    // Only the commits that changed the row kept make columns.
    assert_eq!(m.columns.len(), 3);
    let filtered = build(
        &h,
        &Options {
            filter: "SUB".to_owned(),
            ..Options::default()
        },
    );
    assert_eq!(filtered.rows.len(), 1);
    assert_eq!(filtered.rows[0].name, "src/sub/b.rs");
    assert_eq!(filtered.columns.len(), 2);
}

#[test]
fn frequency_orders_by_commits_and_balance_tells_additions_from_deletions() {
    let h = sample();
    let m = build(
        &h,
        &Options {
            measure: Measure::Frequency,
            ..Options::default()
        },
    );
    assert_eq!(m.rows[0].name, "src/a.rs");
    assert!((m.value(&m.cells[&(0, 0)], Measure::Frequency) - 1.0).abs() < 1e-6);
    // c4: a.rs +1 -9, mostly deleted; c1: +10, all added.
    assert!(m.cells[&(0, 3)].balance() > 0.5);
    assert!((m.cells[&(0, 0)].balance() + 1.0).abs() < 1e-6);
    assert!((m.value(&m.cells[&(0, 3)], Measure::Lines) - 1.0).abs() < 1e-6);
}

#[test]
fn a_file_cell_opens_its_diff_and_a_folder_or_bucket_its_commits() -> Result<(), String> {
    let h = sample();
    let m = build(&h, &Options::default());
    assert_eq!(
        m.open(&h, 0, 0),
        Open::Diff {
            hash: "c1".to_owned(),
            path: "src/a.rs".to_owned()
        }
    );
    assert_eq!(m.open(&h, 2, 0), Open::Nothing);
    let folders = build(
        &h,
        &Options {
            group: Some(1),
            ..Options::default()
        },
    );
    let Open::Commits(list) = folders.open(&h, 0, 1) else {
        return Err("a folder lists its commits".to_owned());
    };
    assert_eq!(
        list,
        [(
            "c2".to_owned(),
            vec!["src/a.rs".to_owned(), "src/sub/b.rs".to_owned()]
        )]
    );
    let weeks = build(
        &h,
        &Options {
            axis: Axis::Week,
            ..Options::default()
        },
    );
    let Open::Commits(list) = weeks.open(&h, 0, 0) else {
        return Err("a week with two commits of a file lists them".to_owned());
    };
    assert_eq!(list.len(), 2);
    Ok(())
}

#[test]
fn a_commit_listed_twice_counts_once_and_nothing_read_is_empty() {
    let mut h = sample();
    let again = h.commits[1].clone();
    h.commits.insert(2, again);
    let m = build(&h, &Options::default());
    assert_eq!(m.columns.len(), 4);
    assert_eq!(m.rows[0].lines, 25);
    let empty = build(&History::default(), &Options::default());
    assert!(empty.rows.is_empty() && empty.columns.is_empty());
    let days = build(
        &History::default(),
        &Options {
            axis: Axis::Day,
            ..Options::default()
        },
    );
    assert!(days.columns.is_empty());
}

#[test]
fn a_real_history_makes_the_matrix_and_a_new_commit_changes_it() -> Result<(), String> {
    let repo = git::fixture::Repo::new("insights-evolution")?;
    repo.commit(
        "one",
        &[("src/a.rs", Some("a\n")), ("README.md", Some("r\n"))],
    )?;
    repo.commit("two", &[("src/a.rs", Some("a\nb\nc\n"))])?;
    repo.git(&["mv", "README.md", "docs.md"])?;
    let third = repo.commit("three", &[])?;
    let read = history::read(repo.path(), "HEAD", 10)?;
    let m = build(&read, &Options::default());
    assert_eq!(m.columns.len(), 3);
    let docs = m
        .rows
        .iter()
        .position(|r| r.name == "docs.md")
        .ok_or("the renamed file is a row")?;
    assert_eq!(m.cells[&(docs, 2)].renamed, 1);
    let mut provenance = read.provenance();
    assert_eq!(provenance.tips, [("HEAD".to_owned(), third.clone())]);

    let fourth = repo.commit("four", &[("src/a.rs", Some("a\n"))])?;
    let again = history::read(repo.path(), "HEAD", 10)?;
    let m2 = build(&again, &Options::default());
    assert_ne!(m, m2);
    assert_eq!(m2.columns.len(), 4);
    assert_eq!(again.tip, fourth);
    // What was read before is stale now that HEAD moved.
    provenance.check_tips(|_| repo.rev("HEAD").ok());
    assert!(provenance.stale.is_some());

    // Read with a limit: the boundary is marked.
    let cut = history::read(repo.path(), "HEAD", 2)?;
    assert!(build(&cut, &Options::default()).boundary);
    Ok(())
}

#[test]
fn a_history_that_cannot_be_read_says_why() {
    let missing = std::path::Path::new("/nonexistent/insights-evolution");
    assert!(history::read(missing, "HEAD", 10).is_err());
}
