use super::*;
use crate::insights::history::{FileChange, HistCommit};

fn commit(hash: &str, time: i64, files: &[&str]) -> HistCommit {
    HistCommit {
        hash: hash.to_owned(),
        parents: vec!["p".to_owned()],
        time,
        author: "Ann".to_owned(),
        subject: hash.to_owned(),
        files: files
            .iter()
            .map(|p| FileChange {
                path: (*p).to_owned(),
                old_path: None,
                added: Some(1),
                deleted: Some(0),
            })
            .collect(),
    }
}

fn history(commits: Vec<HistCommit>) -> History {
    History {
        rev: "HEAD".to_owned(),
        tip: "t".to_owned(),
        limit: 100,
        truncated: false,
        commits,
    }
}

fn opts(min_support: u32) -> Options {
    Options {
        min_support,
        ..Options::default()
    }
}

fn pair<'a>(c: &'a Coupling, a: &str, b: &str) -> Option<&'a Pair> {
    let (x, y) = (c.unit_index(a)?, c.unit_index(b)?);
    c.pairs.iter().find(|p| (p.a, p.b) == (x.min(y), x.max(y)))
}

#[test]
fn support_counts_commits_changing_both_and_confidence_divides_by_changes()
-> Result<(), &'static str> {
    let h = history(vec![
        commit("c4", 4, &["a.rs", "b.rs"]),
        commit("c3", 3, &["a.rs", "b.rs", "c.rs"]),
        commit("c2", 2, &["a.rs"]),
        commit("c1", 1, &["a.rs", "c.rs"]),
    ]);
    let c = analyze(&h, &opts(1));
    assert_eq!(c.counted, 4);
    let ab = pair(&c, "a.rs", "b.rs").ok_or("a-b")?;
    assert_eq!(ab.support, 2);
    assert_eq!(ab.commits, [0, 1]);
    let a = c.unit_index("a.rs").ok_or("a")?;
    let b = c.unit_index("b.rs").ok_or("b")?;
    assert_eq!(c.units[a].changes, 4);
    assert!((c.confidence(ab, a) - 0.5).abs() < 1e-6);
    assert!((c.confidence(ab, b) - 1.0).abs() < 1e-6);
    // Strongest first.
    assert!(c.pairs[0].support >= c.pairs[c.pairs.len() - 1].support);
    assert_eq!(c.shown, c.pairs.len());
    assert_eq!(c.cut(), None);
    Ok(())
}

#[test]
fn bulk_commits_and_merges_are_left_out_and_counted_as_such() {
    let many: Vec<String> = (0..50).map(|i| format!("gen/{i}.rs")).collect();
    let many: Vec<&str> = many.iter().map(String::as_str).collect();
    let mut merge = commit("m", 5, &[]);
    merge.parents.push("q".to_owned());
    let h = history(vec![
        merge,
        commit("bulk", 4, &many),
        commit("c2", 2, &["a.rs", "b.rs"]),
    ]);
    let c = analyze(&h, &opts(1));
    assert_eq!(c.bulk, 1);
    assert_eq!(c.skipped, 1);
    assert_eq!(c.counted, 1);
    assert_eq!(c.pairs.len(), 1);
    assert!(c.unit_index("gen/1.rs").is_none());
}

#[test]
fn pairs_below_the_minimum_support_are_dropped_and_the_top_cut_is_said() {
    let h = history(vec![
        commit("c3", 3, &["a", "b"]),
        commit("c2", 2, &["a", "b"]),
        commit("c1", 1, &["a", "c"]),
    ]);
    let c = analyze(&h, &opts(2));
    assert_eq!(c.pairs.len(), 1);
    assert!(pair(&c, "a", "c").is_none());
    let top = analyze(&h, &Options { top: 1, ..opts(1) });
    assert_eq!(top.pairs.len(), 2);
    assert_eq!(top.shown, 1);
    assert!(top.cut().is_some());
}

#[test]
fn folders_the_filter_and_the_time_range_change_what_is_counted() -> Result<(), &'static str> {
    let h = history(vec![
        commit("c3", 30, &["src/a/x.rs", "src/b/y.rs", "docs/z.md"]),
        commit("c2", 20, &["src/a/x.rs", "src/a/w.rs", "src/b/y.rs"]),
        commit("c1", 10, &["README.md", "src/a/x.rs"]),
    ]);
    let folders = analyze(
        &h,
        &Options {
            depth: Some(2),
            ..opts(1)
        },
    );
    let ab = pair(&folders, "src/a", "src/b").ok_or("folders")?;
    assert_eq!(ab.support, 2);
    // A commit changing two files of a folder counts the folder once.
    let a = folders.unit_index("src/a").ok_or("src/a")?;
    assert_eq!(folders.units[a].changes, 3);
    assert!(folders.unit_index("/").is_some());
    let filtered = analyze(
        &h,
        &Options {
            filter: "src/".to_owned(),
            ..opts(1)
        },
    );
    assert!(filtered.unit_index("docs/z.md").is_none());
    assert!(filtered.unit_index("README.md").is_none());
    // `src2/` isn't under `src`.
    assert!(!under("src", "src2/x.rs"));
    let recent = analyze(
        &h,
        &Options {
            since: Some(20),
            ..opts(1)
        },
    );
    assert_eq!(recent.older, 1);
    assert_eq!(recent.counted, 2);
    Ok(())
}

#[test]
fn left_out_files_are_ranked_by_how_often_they_changed_alongside() {
    let h = history(vec![
        commit("c5", 5, &["api.rs", "client.rs", "docs.md"]),
        commit("c4", 4, &["api.rs", "client.rs"]),
        commit("c3", 3, &["api.rs", "client.rs"]),
        commit("c2", 2, &["api.rs", "docs.md"]),
        commit("c1", 1, &["api.rs", "other.rs"]),
    ]);
    let c = analyze(&h, &opts(1));
    let list = left_out(&c, &["api.rs".to_owned()]);
    let names: Vec<&str> = list.iter().map(|l| c.units[l.unit].name.as_str()).collect();
    assert_eq!(names, ["client.rs", "docs.md", "other.rs"]);
    assert_eq!(list[0].support, 3);
    assert!((list[0].confidence - 0.6).abs() < 1e-6);
    // What the change touches isn't left out; a file the history doesn't know adds nothing.
    let both = left_out(&c, &["api.rs".to_owned(), "client.rs".to_owned()]);
    assert!(both.iter().all(|l| c.units[l.unit].name != "client.rs"));
    assert_eq!(c.units[both[0].unit].name, "docs.md");
    assert!(left_out(&c, &["new.rs".to_owned()]).is_empty());
}

#[test]
fn a_real_history_couples_the_files_changed_together() -> Result<(), String> {
    let repo = crate::git::fixture::Repo::new("insights-coupling")?;
    repo.commit("one", &[("a.rs", Some("1\n")), ("b.rs", Some("1\n"))])?;
    repo.commit("two", &[("a.rs", Some("2\n")), ("b.rs", Some("2\n"))])?;
    repo.commit("three", &[("c.rs", Some("1\n"))])?;
    let h = crate::insights::history::read(repo.path(), "HEAD", 100)?;
    let c = analyze(&h, &opts(2));
    let ab = pair(&c, "a.rs", "b.rs").ok_or("a-b")?;
    assert_eq!(ab.support, 2);
    assert_eq!(h.commits[ab.commits[0]].subject, "two");
    assert_eq!(c.pairs.len(), 1);
    // A new commit moves the tip: what was read is stale, and read again it counts more.
    let mut p = h.provenance();
    repo.commit("four", &[("a.rs", Some("3\n")), ("b.rs", Some("3\n"))])?;
    let head = repo.rev("HEAD")?;
    p.check_tips(|_| Some(head.clone()));
    assert!(p.stale.is_some());
    let again = analyze(
        &crate::insights::history::read(repo.path(), "HEAD", 100)?,
        &opts(2),
    );
    assert_eq!(pair(&again, "a.rs", "b.rs").map(|p| p.support), Some(3));
    Ok(())
}

#[test]
fn no_repository_is_an_error_and_an_empty_history_couples_nothing() {
    let missing = std::path::Path::new("/nonexistent/insights-coupling");
    assert!(crate::insights::history::read(missing, "HEAD", 10).is_err());
    let c = analyze(&history(Vec::new()), &Options::default());
    assert_eq!(c.counted, 0);
    assert!(c.pairs.is_empty());
    assert!(left_out(&c, &["a".to_owned()]).is_empty());
}
