use super::*;
use crate::git::RebaseCommit;

fn entry(hash: &str) -> Entry {
    Entry {
        commit: RebaseCommit {
            hash: hash.to_owned(),
            summary: hash.to_owned(),
            ..RebaseCommit::default()
        },
        step: Step::Pick,
        message: hash.to_owned(),
    }
}

fn line(entries: &[Entry]) -> Vec<(String, Step)> {
    entries
        .iter()
        .map(|e| (e.commit.hash.clone(), e.step))
        .collect()
}

#[test]
fn the_moved_changes_meld_into_their_commit_older_or_newer() {
    let all = || ["b", "x", "a", "y"].map(entry).to_vec();
    let pick = |h: &str| (h.to_owned(), Step::Pick);
    let fix = |h: &str| (h.to_owned(), Step::Fixup);
    // Into an older one: right after it, the old commit replaced.
    let older = todo(all(), "a", Some(entry("a2")), Some("b"), entry("m"));
    assert_eq!(
        older.map(|t| line(&t)),
        Ok(vec![pick("b"), fix("m"), pick("x"), pick("a2"), pick("y")])
    );
    // Into a newer one, nothing left of the old one.
    let newer = todo(all(), "b", None, Some("y"), entry("m"));
    assert_eq!(
        newer.map(|t| line(&t)),
        Ok(vec![pick("x"), pick("a"), pick("y"), fix("m")])
    );
    // To the working changes: picked last.
    let work = todo(all(), "x", Some(entry("x2")), None, entry("m"));
    assert_eq!(
        work.map(|t| line(&t)),
        Ok(vec![pick("b"), pick("x2"), pick("a"), pick("y"), pick("m")])
    );
    assert!(todo(all(), "gone", None, None, entry("m")).is_err());
    assert!(todo(all(), "a", None, Some("gone"), entry("m")).is_err());
}

#[test]
fn targets_stop_at_a_merge_and_leave_the_commit_out() {
    let commit = |h: &str, parents: usize| Commit {
        hash: h.to_owned(),
        summary: format!("s{h}"),
        parents: (0..parents).map(|p| format!("p{p}")).collect(),
        ..Commit::default()
    };
    let commits = [
        commit("h", 1),
        commit("a", 1),
        commit("b", 1),
        commit("m", 2),
        commit("o", 1),
    ];
    let line: Vec<&Commit> = commits.iter().collect();
    let found: Vec<(String, usize)> = targets(&line, 1)
        .into_iter()
        .map(|t| (t.hash, t.depth))
        .collect();
    assert_eq!(found, [("h".to_owned(), 0), ("b".to_owned(), 2)]);
    let root = [commit("h", 1), commit("r", 0)];
    let line: Vec<&Commit> = root.iter().collect();
    assert!(targets(&line, 0).is_empty());
}

#[test]
fn a_move_needs_something_to_move_and_a_place() {
    let mut m = Move {
        targets: vec![],
        depth: 0,
        dest: None,
        truncated: false,
    };
    assert!(
        m.problem(&[3, 0])
            .is_some_and(|(p, hint)| p.contains("Click") && hint)
    );
    assert!(m.problem(&[3, 1]).is_some_and(|(p, _)| p.contains("where")));
    m.dest = Some(Dest::Work);
    assert_eq!(m.problem(&[0, 1]), None);
}
