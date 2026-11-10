#[test]
fn cherry_picks_and_reverts_take_one_real_commit() {
    let dir = repo("pick");
    assert!(git::run(Some(&dir), &["switch", "-q", "-c", "side"]).is_ok());
    write(&dir, "b.txt", "b\n");
    commit_all(&dir, "add b");
    assert!(git::run(Some(&dir), &["switch", "-q", "main"]).is_ok());
    let pick = planned(
        &dir,
        WriteOp::CherryPick {
            commit: "side".into(),
        },
    );
    let picked = run(&dir, &pick);
    let has_b = dir.join("b.txt").exists();
    let revert = planned(
        &dir,
        WriteOp::Revert {
            commit: "HEAD".into(),
        },
    );
    let reverted = run(&dir, &revert);
    let gone_b = !dir.join("b.txt").exists();
    let refused = [
        plan(
            &dir,
            &WriteOp::CherryPick {
                commit: "--abort".into(),
            },
        ),
        plan(
            &dir,
            &WriteOp::CherryPick {
                commit: "main..side".into(),
            },
        ),
        plan(
            &dir,
            &WriteOp::Revert {
                commit: "nope".into(),
            },
        ),
    ];
    let _ = std::fs::remove_dir_all(&dir);
    assert!(pick.label.starts_with("Cherry-pick "));
    assert!(matches!(pick.undo, Undo::HeadMove { mode: "--keep" }));
    assert!(picked.is_ok() && has_b);
    assert!(reverted.is_ok() && gone_b);
    assert!(refused.iter().all(Result::is_err));
}

#[test]
fn only_a_hard_reset_asks_and_its_undo_keeps_later_work() {
    let dir = repo("reset");
    write(&dir, "a.txt", "two\n");
    commit_all(&dir, "two");
    let reset = |mode| {
        plan(
            &dir,
            &WriteOp::Reset {
                commit: "HEAD~1".into(),
                mode,
            },
        )
    };
    let soft = reset(ResetMode::Soft);
    let mixed = reset(ResetMode::Mixed);
    let hard = reset(ResetMode::Hard);
    let _ = std::fs::remove_dir_all(&dir);
    assert!(soft.as_ref().is_ok_and(|p| p.ask.is_none()));
    assert!(mixed.as_ref().is_ok_and(|p| p.ask.is_none()));
    let hard = hard.ok();
    assert!(hard.as_ref().is_some_and(|p| {
        p.ask
            .as_ref()
            .is_some_and(|a| a.undo_note.contains("stay lost"))
    }));
    assert!(hard.is_some_and(|p| matches!(p.undo, Undo::HeadMove { mode: "--keep" })));
}

#[test]
fn a_hard_reset_counts_only_what_it_throws_away() {
    let dir = repo("reset-count");
    write(&dir, "a.txt", "two\n");
    commit_all(&dir, "two");
    write(&dir, "a.txt", "dirty\n");
    write(&dir, "untracked.txt", "kept\n");
    let hard = plan(
        &dir,
        &WriteOp::Reset {
            commit: "HEAD~1".into(),
            mode: ResetMode::Hard,
        },
    );
    let _ = std::fs::remove_dir_all(&dir);
    let ask = hard.ok().and_then(|p| p.ask);
    assert!(
        ask.as_ref()
            .is_some_and(|a| a.title.starts_with("Hard reset main to "))
    );
    let body = ask.map(|a| a.body).unwrap_or_default();
    assert!(body.contains("1 tracked file "), "{body}");
    assert!(body.contains("The commit after"), "{body}");
    assert!(body.contains("Untracked files stay"), "{body}");
}

#[test]
fn a_reset_refuses_once_another_branch_is_checked_out() {
    let dir = repo("reset-guard");
    write(&dir, "a.txt", "two\n");
    commit_all(&dir, "two");
    let p = planned(
        &dir,
        WriteOp::Reset {
            commit: "HEAD~1".into(),
            mode: ResetMode::Mixed,
        },
    );
    // Checked out elsewhere (the terminal) before it ran.
    assert!(git::run(Some(&dir), &["switch", "-q", "-c", "other"]).is_ok());
    let ran = run(&dir, &p);
    let _ = std::fs::remove_dir_all(&dir);
    assert!(ran.is_err_and(|e| e.contains("checked out now")));
}

#[test]
fn moving_a_commit_runs_and_dropping_one_asks() {
    let dir = repo("rewrite");
    write(&dir, "b.txt", "b\n");
    commit_all(&dir, "add b");
    write(&dir, "c.txt", "c\n");
    commit_all(&dir, "add c");
    let up = planned(
        &dir,
        WriteOp::MoveCommit {
            commit: "HEAD~1".into(),
            up: true,
        },
    );
    let drop = plan(
        &dir,
        &WriteOp::DropCommit {
            commit: "HEAD~1".into(),
        },
    );
    let moved = run(&dir, &up);
    let order = git_out(&dir, &["log", "--format=%s"]);
    let _ = std::fs::remove_dir_all(&dir);
    assert!(up.ask.is_none());
    assert!(moved.is_ok(), "{moved:?}");
    assert_eq!(order, "add b\nadd c\nbase\n");
    let drop = drop.ok();
    assert!(
        drop.as_ref()
            .is_some_and(|p| p.ask.as_ref().is_some_and(|a| a.body.contains("add b")))
    );
    assert!(drop.is_some_and(|p| p.undo != Undo::None));
}

#[test]
fn deleting_a_branch_asks_and_can_be_undone() {
    let dir = repo("delete");
    assert!(git::run(Some(&dir), &["branch", "old"]).is_ok());
    let current = plan(
        &dir,
        &WriteOp::DeleteBranch {
            name: "main".into(),
            force: false,
        },
    );
    let p = planned(
        &dir,
        WriteOp::DeleteBranch {
            name: "old".into(),
            force: true,
        },
    );
    let entry = run(&dir, &p);
    let gone = git::resolve(&dir, "refs/heads/old").is_err();
    let undone = entry
        .as_ref()
        .ok()
        .and_then(|e| e.as_ref())
        .map(|e| undo(&dir, e));
    let back = git::resolve(&dir, "refs/heads/old").is_ok();
    let _ = std::fs::remove_dir_all(&dir);
    assert!(current.is_err_and(|e| e.contains("checked out")));
    assert!(p.ask.as_ref().is_some_and(|a| a.body.contains("“old”") && a.body.contains("even if it isn't merged")));
    assert!(gone);
    assert!(undone.is_some_and(|u| u.is_ok()));
    assert!(back);
}
