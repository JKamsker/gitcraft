use super::*;

fn commit(hash: &str, parents: &[&str]) -> Commit {
    Commit {
        hash: hash.to_owned(),
        parents: parents.iter().map(|p| (*p).to_owned()).collect(),
        ..Commit::default()
    }
}

fn rebase_commit(hash: &str, merge: bool) -> RebaseCommit {
    RebaseCommit {
        hash: hash.to_owned(),
        summary: format!("summary {hash}"),
        merge,
        ..RebaseCommit::default()
    }
}

fn read(target: &str, hashes: &[&str]) -> FoldRead {
    FoldRead {
        commits: hashes.iter().map(|h| rebase_commit(h, false)).collect(),
        target: target.to_owned(),
        ancestor: true,
        head: hashes.last().copied().unwrap_or_default().to_owned(),
        branch: Some("main".to_owned()),
        unpushed: hashes.iter().map(|h| (*h).to_owned()).collect(),
        staged: 1,
        ..FoldRead::default()
    }
}

#[test]
fn nothing_folds_without_staged_changes_a_branch_or_while_something_is_going_on() {
    assert!(blocked(1, Some("main"), None).is_none());
    let none_staged = blocked(0, Some("main"), None);
    assert!(none_staged.is_some_and(|w| w.contains("Nothing is staged")));
    assert!(blocked(1, None, None).is_some_and(|w| w.contains("detached")));
    let rebasing = blocked(1, Some("main"), Some(InProgress::Rebase));
    assert!(rebasing.is_some_and(|w| w.contains("rebase is in progress")));
    let merging = blocked(1, Some("main"), Some(InProgress::Merge));
    assert!(merging.is_some_and(|w| w.contains("merge is in progress")));
}

#[test]
fn what_the_menu_allows() {
    // HEAD e, a merge d of x into c, then c, b and the root a.
    let commits = [
        commit("e", &["d"]),
        commit("d", &["c", "x"]),
        commit("x", &["b"]),
        commit("c", &["b"]),
        commit("b", &["a"]),
        commit("a", &[]),
    ];
    let at = |h: &str| commits.iter().find(|c| c.hash == h);
    let line: Vec<&Commit> = ["e", "d", "c", "b", "a"]
        .iter()
        .filter_map(|h| at(h))
        .collect();
    assert_eq!(refusal(&line, "e"), None);
    assert_eq!(refusal(&line, "d"), Some(crate::i18n::text(MERGE_TARGET)));
    assert_eq!(refusal(&line, "c"), Some(crate::i18n::text(MERGES_AFTER)));
    assert!(
        refusal(&line, "x").is_some(),
        "not on the first-parent line"
    );
    let linear = &line[2..];
    assert_eq!(refusal(linear, "a"), None, "the root commit");
    assert_eq!(refusal(linear, "b"), None);
}

#[test]
fn a_plan_from_what_was_read() {
    let mut found = read("b", &["b", "c", "d"]);
    found.unpushed.remove("b");
    let plan = Plan::from_read(found);
    assert!(plan.is_ok(), "{plan:?}");
    if let Ok(plan) = plan {
        assert_eq!(plan.target.hash, "b");
        let after: Vec<&str> = plan.after.iter().map(|c| c.hash.as_str()).collect();
        assert_eq!(after, ["c", "d"]);
        assert_eq!(plan.pushed, ["b"]);
        assert!(plan.rewrites_pushed());
        assert_eq!(plan.head, "d");
        let kept = Plan {
            keep_fixup: true,
            ..plan
        };
        assert!(
            !kept.rewrites_pushed(),
            "a fixup commit on top rewrites nothing"
        );
    }
}

#[test]
fn plans_that_cant_be() {
    let refused = |r: FoldRead| Plan::from_read(r).err().unwrap_or_default();
    let mut not_ancestor = read("x", &[]);
    not_ancestor.ancestor = false;
    assert!(refused(not_ancestor).contains("not on the checked-out branch"));
    let mut cut = read("a", &["a", "b"]);
    (cut.root, cut.shallow) = (true, true);
    assert!(refused(cut).contains("shallow"));
    let mut detached = read("b", &["b", "c"]);
    detached.branch = None;
    assert!(refused(detached).contains("detached"));
    let mut nothing = read("b", &["b", "c"]);
    nothing.staged = 0;
    assert!(refused(nothing).contains("Nothing is staged"));
    let mut rebasing = read("b", &["b", "c"]);
    rebasing.in_progress = Some(InProgress::Rebase);
    assert!(refused(rebasing).contains("in progress"));
    let mut merged = read("b", &["b", "c", "d"]);
    merged.commits[1].merge = true;
    assert_eq!(refused(merged), crate::i18n::text(MERGES_AFTER));
    let mut merge = read("b", &["b", "c"]);
    merge.commits[0].merge = true;
    assert_eq!(refused(merge), crate::i18n::text(MERGE_TARGET));
    // The commits read don't start at the target.
    assert!(!refused(read("b", &["a", "b"])).is_empty());
}

/// The plan for `read(target, hashes)`, which these tests make valid.
fn plan(target: &str, hashes: &[&str]) -> Plan {
    let made = Plan::from_read(read(target, hashes));
    assert!(made.is_ok(), "{made:?}");
    made.unwrap_or_else(|_| Plan {
        target: RebaseCommit::default(),
        after: Vec::new(),
        pushed: Vec::new(),
        root: false,
        head: String::new(),
        branch: String::new(),
        staged: 0,
        keep_fixup: false,
    })
}

#[test]
fn the_rebase_puts_the_fixup_right_after_the_target() {
    let p = plan("b", &["b", "c", "d"]);
    let editor = p.editor();
    assert!(editor.is_ok());
    if let Ok(editor) = editor {
        assert_eq!(editor.onto, "b^");
        let steps: Vec<(&str, Step)> = editor
            .entries
            .iter()
            .map(|e| (e.commit.hash.as_str(), e.step))
            .collect();
        assert_eq!(
            steps,
            [
                ("b", Step::Pick),
                ("refs/heads/main", Step::Fixup),
                ("c", Step::Pick),
                ("d", Step::Pick),
            ]
        );
    }
    // The root commit: the rebase takes it in too (`--root`).
    let root = Plan {
        root: true,
        ..plan("a", &["a", "b"])
    };
    assert_eq!(root.editor().map(|e| e.onto), Ok(String::new()));
    // The latest commit: just the two.
    let latest = plan("d", &["d"]).editor().map(|e| e.entries.len());
    assert_eq!(latest, Ok(2));
}

#[test]
fn the_steps_check_the_branch_and_head_first() {
    let p = plan("b", &["b", "c"]);
    let (steps, undo) = p.fold_steps(vec!["rebase".to_owned()]);
    assert_eq!(steps[0], expect_symref("HEAD", "refs/heads/main"));
    assert_eq!(steps[1], expect_ref_at("HEAD", "c"));
    assert_eq!(steps[2], expect_ref_at("MERGE_HEAD", ""));
    assert_eq!(
        &steps[6..],
        &[
            vec![crate::repo::operations::EXPECT_FOLD_SAFE.into(), "b".into()],
            vec!["commit".into(), "--fixup=b".into()],
            vec![
                crate::repo::operations::EXPECT_FOLD_IGNORED.into(),
                "b".into()
            ],
            vec!["rebase".into()],
            VERIFY.map(str::to_owned).to_vec(),
        ]
    );
    assert_eq!(undo, Undo::HeadMove { mode: "--soft" });
    let failed = format!("git {} failed (exit status: 1)", VERIFY.join(" "));
    assert!(lost_in_rebase(&failed));
    assert!(!lost_in_rebase("CONFLICT (content): Merge conflict in f"));
    let kept = Plan {
        keep_fixup: true,
        ..p
    };
    let (steps, _) = kept.keep_steps();
    assert_eq!(steps.len(), 7);
    assert_eq!(kept.label(), "Commit fixup! for b");
}

#[test]
fn a_stopped_fold_is_over_once_a_later_status_has_no_rebase() {
    let mut stopped = plan("b", &["b", "c"]).stopped();
    // No read started since it stopped: nothing to go by yet.
    assert!(!stopped.over(5, None));
    stopped.read = Some(6);
    assert!(!stopped.over(5, None), "a read from before it stopped");
    assert!(!stopped.over(6, Some(InProgress::Rebase)));
    assert!(stopped.over(7, None));
    let mut never = plan("b", &["b", "c"]).stopped();
    never.read = Some(2);
    assert!(never.over(2, None), "the commit failed: no rebase ever ran");
}

#[test]
fn aborting_a_stopped_fold_puts_the_staged_changes_back() {
    let stopped = plan("b", &["b", "c"]).stopped();
    let steps = stopped.abort_steps("f");
    assert_eq!(steps[0], expect_ref_at("refs/heads/main~1", "c"));
    assert_eq!(steps[2], expect_ref_at("ORIG_HEAD", "f"));
    assert_eq!(steps[3], ["rebase", "--abort"]);
    let after = stopped.unfixup_steps();
    assert_eq!(after[0], expect_symref("HEAD", "refs/heads/main"));
    assert_eq!(after[2], ["reset", "--soft", "c"]);
    let recorded = match stopped.continue_kind() {
        OpKind::Normal(Undo::Custom { undo, redo }, _) => Some((undo, redo)),
        _ => None,
    };
    assert!(recorded.is_some(), "continuing records the fold");
    if let Some((undo, redo)) = recorded {
        assert_eq!(undo.last(), Some(&soft_reset("c")));
        assert_eq!(redo.last(), Some(&soft_reset("ORIG_HEAD")));
    }
}
