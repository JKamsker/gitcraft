use super::*;

#[test]
fn quick_rewrites_preserve_the_original_branch_guard_before_head_and_rebase() {
    let captured = super::super::operations::expect_symref("HEAD", "refs/heads/original");
    let command = vec!["rebase".into(), "-i".into(), "base".into()];
    let operations = guarded_commands(captured.clone(), "original-head", command.clone());
    assert_eq!(
        operations,
        vec![captured, expect_ref_at("HEAD", "original-head"), command]
    );
    assert_ne!(
        operations[0],
        super::super::operations::expect_symref("HEAD", "refs/heads/other")
    );
}

fn commit(hash: &str, parents: &[&str]) -> Commit {
    Commit {
        hash: hash.to_owned(),
        parents: parents.iter().map(|p| (*p).to_owned()).collect(),
        ..Commit::default()
    }
}

fn rebase_commit(hash: &str) -> RebaseCommit {
    RebaseCommit {
        hash: hash.to_owned(),
        ..RebaseCommit::default()
    }
}

type Planned = Result<(String, Vec<(String, Step)>), String>;

/// What the plan for `edit` rebases onto, and its hashes and steps, oldest first.
fn planned(edit: Edit, hash: &str, hashes: &[&str]) -> Planned {
    let commits = hashes.iter().map(|h| rebase_commit(h)).collect();
    plan(edit, hash, commits).map(|editor| {
        let steps = editor
            .entries
            .iter()
            .map(|e| (e.commit.hash.clone(), e.step))
            .collect();
        (editor.onto, steps)
    })
}

fn picks(hashes: &[&str]) -> Vec<(String, Step)> {
    hashes
        .iter()
        .map(|h| ((*h).to_owned(), Step::Pick))
        .collect()
}

#[test]
fn dropping_marks_the_commit() {
    let mut steps = picks(&["b", "c", "d"]);
    steps[1].1 = Step::Drop;
    assert_eq!(
        planned(Edit::Drop, "c", &["b", "c", "d"]),
        Ok(("c^".to_owned(), steps))
    );
}

#[test]
fn moving_swaps_with_a_neighbour() {
    assert_eq!(
        planned(Edit::MoveUp, "c", &["b", "c", "d"]),
        Ok(("c^".to_owned(), picks(&["b", "d", "c"])))
    );
    assert_eq!(
        planned(Edit::MoveDown, "c", &["b", "c", "d"]),
        Ok(("c~2".to_owned(), picks(&["c", "b", "d"])))
    );
}

#[test]
fn plans_that_cant_be() {
    let commits = ["c", "d"].map(rebase_commit).to_vec();
    assert!(plan(Edit::MoveUp, "d", commits.clone()).is_err());
    assert!(plan(Edit::MoveDown, "c", commits.clone()).is_err());
    assert!(plan(Edit::Drop, "x", commits.clone()).is_err());
    let mut merged = commits;
    merged[1].merge = true;
    assert!(plan(Edit::Drop, "c", merged).is_err());
}

#[test]
fn what_the_menu_allows() {
    // HEAD e, a merge d of x into c, then c, b and the root a.
    let commits = vec![
        commit("e", &["d"]),
        commit("d", &["c", "x"]),
        commit("x", &["b"]),
        commit("c", &["b"]),
        commit("b", &["a"]),
        commit("a", &[]),
    ];
    let index = commits
        .iter()
        .enumerate()
        .map(|(i, c)| (c.hash.clone(), i))
        .collect();
    let line = first_parents(&commits, &index, "e");
    let ok = |edit, hash| refusal(edit, &line, hash).is_none();
    assert!(ok(Edit::Message, "e"));
    assert!(ok(Edit::Drop, "e"));
    assert!(!ok(Edit::MoveUp, "e"));
    assert!(!ok(Edit::MoveDown, "e"), "the commit below is a merge");
    assert!(!ok(Edit::Drop, "d"), "a merge");
    assert!(!ok(Edit::Message, "c"), "merges after it");
    assert!(!ok(Edit::Drop, "x"), "not on the first-parent line");

    let line = first_parents(&commits, &index, "c");
    let ok = |edit, hash| refusal(edit, &line, hash).is_none();
    assert!(ok(Edit::MoveUp, "b"));
    assert!(!ok(Edit::MoveDown, "b"), "the root is below it");
    assert!(ok(Edit::MoveDown, "c"));
    assert!(!ok(Edit::Drop, "a"), "the root");
    assert!(ok(Edit::Message, "b"));
}
