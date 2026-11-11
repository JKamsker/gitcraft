#[test]
fn a_push_always_asks_and_says_it_cant_be_undone() {
    let dir = repo("push");
    let nowhere = plan(&dir, &WriteOp::Push { force: false });
    let remote = dir.with_extension("remote");
    let _ = std::fs::remove_dir_all(&remote);
    assert!(git::run(None, &["init", "-q", "--bare", &remote.to_string_lossy()]).is_ok());
    assert!(
        git::run(
            Some(&dir),
            &["remote", "add", "origin", &remote.to_string_lossy()]
        )
        .is_ok()
    );
    let publish = plan(&dir, &WriteOp::Push { force: false });
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_dir_all(&remote);
    assert!(nowhere.is_err_and(|e| e.contains("no remote")));
    let publish = publish.ok();
    assert!(publish.as_ref().is_some_and(|p| p.undo == Undo::None));
    assert!(publish.as_ref().is_some_and(|p| {
        p.ask
            .as_ref()
            .is_some_and(|a| a.title == "Publish main on origin" && a.undo_note.contains("can't"))
    }));
    assert!(publish.is_some_and(|p| {
        p.ops
            .last()
            .is_some_and(|op| op.contains(&"refs/heads/main:refs/heads/main".to_owned()))
    }));
}

/// A tracked repository: `main` pushed to a bare `origin`.
fn tracked(name: &str) -> (PathBuf, PathBuf) {
    let dir = repo(name);
    let remote = dir.with_extension("remote");
    let _ = std::fs::remove_dir_all(&remote);
    assert!(git::run(None, &["init", "-q", "--bare", &remote.to_string_lossy()]).is_ok());
    assert!(
        git::run(
            Some(&dir),
            &["remote", "add", "origin", &remote.to_string_lossy()]
        )
        .is_ok()
    );
    assert!(git::run(Some(&dir), &["push", "-q", "-u", "origin", "main"]).is_ok());
    (dir, remote)
}

#[test]
fn a_push_sends_only_what_the_card_showed() {
    let (dir, remote) = tracked("push-pinned");
    let level = plan(&dir, &WriteOp::Push { force: false });
    let before = git::resolve(&remote, "refs/heads/main").unwrap_or_default();
    write(&dir, "a.txt", "two\n");
    commit_all(&dir, "two");
    let p = planned(&dir, WriteOp::Push { force: false });
    // The agent commits again while the card waits.
    write(&dir, "a.txt", "three\n");
    commit_all(&dir, "three");
    let ran = run(&dir, &p);
    let there = git::resolve(&remote, "refs/heads/main").unwrap_or_default();
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_dir_all(&remote);
    assert!(level.is_err_and(|e| e.contains("nothing to push")));
    assert!(
        p.ask
            .as_ref()
            .is_some_and(|a| a.body.contains("1 new commit "))
    );
    assert!(ran.is_err_and(|e| e.contains("main has moved")));
    assert_eq!(there, before, "the remote keeps what it had");
}

#[test]
fn a_force_push_is_leased_to_what_the_card_showed() {
    let (dir, remote) = tracked("push-lease");
    let seen = git::resolve(&dir, "refs/remotes/origin/main").unwrap_or_default();
    let p = planned(&dir, WriteOp::Push { force: true });
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_dir_all(&remote);
    let lease = format!("--force-with-lease=refs/heads/main:{seen}");
    assert!(p.ops.last().is_some_and(|op| op.contains(&lease)));
    let ask = p.ask.unwrap_or_else(|| super::Ask {
        title: String::new(),
        body: String::new(),
        undo_note: String::new(),
        presentation: None,
    });
    assert_eq!(ask.title, "Force push main to origin/main");
    assert!(ask.body.contains("has no commits that main lacks"));
}

#[test]
fn staging_a_conflicted_file_asks_first() {
    let dir = repo("conflict");
    assert!(git::run(Some(&dir), &["switch", "-q", "-c", "side"]).is_ok());
    write(&dir, "a.txt", "side\n");
    commit_all(&dir, "side");
    assert!(git::run(Some(&dir), &["switch", "-q", "main"]).is_ok());
    write(&dir, "a.txt", "main\n");
    commit_all(&dir, "main");
    let _ = git::run(Some(&dir), &["merge", "side"]);
    let stage = plan(
        &dir,
        &WriteOp::Stage {
            paths: vec![".".into()],
        },
    );
    let unstage = plan(
        &dir,
        &WriteOp::Unstage {
            paths: vec!["a.txt".into()],
        },
    );
    let _ = std::fs::remove_dir_all(&dir);
    let stage = stage.ok();
    assert!(stage.as_ref().is_some_and(|p| p.undo == Undo::None));
    assert!(stage.is_some_and(|p| p.ask.is_some_and(|a| a.body.contains("a.txt"))));
    assert!(unstage.is_err_and(|e| e.contains("conflicted")));
}

#[test]
fn paths_from_the_request_are_checked_again() {
    let dir = repo("paths");
    let outside = plan(
        &dir,
        &WriteOp::Stage {
            paths: vec!["../x".into()],
        },
    );
    let absolute = plan(
        &dir,
        &WriteOp::Stage {
            paths: vec!["/etc/passwd".into()],
        },
    );
    let none = plan(&dir, &WriteOp::Stage { paths: vec![] });
    let _ = std::fs::remove_dir_all(&dir);
    assert!(outside.is_err() && absolute.is_err() && none.is_err());
}

/// A merge of `side` into `main` stopped on a conflict in `a.txt`.
fn conflicted(name: &str) -> PathBuf {
    let dir = repo(name);
    assert!(git::run(Some(&dir), &["switch", "-q", "-c", "side"]).is_ok());
    write(&dir, "a.txt", "side\n");
    commit_all(&dir, "side");
    assert!(git::run(Some(&dir), &["switch", "-q", "main"]).is_ok());
    write(&dir, "a.txt", "main\n");
    commit_all(&dir, "main");
    let _ = git::run(Some(&dir), &["merge", "side"]);
    dir
}

#[test]
fn a_conflict_is_found_however_its_path_is_written() {
    let dir = conflicted("conflict-spelling");
    let dotted = plan(
        &dir,
        &WriteOp::Stage {
            paths: vec!["./a.txt".into()],
        },
    );
    let slashes = plan(
        &dir,
        &WriteOp::Unstage {
            paths: vec!["a.txt/".into()],
        },
    );
    let unmerged = git_out(&dir, &["ls-files", "-u"]);
    let _ = std::fs::remove_dir_all(&dir);
    assert!(
        dotted.is_err(),
        "`./a.txt` is refused, never planned without its card"
    );
    // A folder-only pathspec: it names no file to git, in the check or the reset.
    assert!(slashes.is_err());
    assert!(!unmerged.is_empty(), "the conflict is still there");
}

#[test]
fn what_would_end_a_merge_in_progress_is_refused() {
    let dir = conflicted("in-progress");
    let refused = [
        plan(
            &dir,
            &WriteOp::Reset {
                commit: "HEAD~1".into(),
                mode: ResetMode::Mixed,
            },
        ),
        plan(
            &dir,
            &WriteOp::Stash {
                message: String::new(),
            },
        ),
        plan(
            &dir,
            &WriteOp::Checkout {
                branch: "side".into(),
            },
        ),
    ];
    let merging = git::resolve(&dir, "MERGE_HEAD").is_ok();
    let _ = std::fs::remove_dir_all(&dir);
    for (i, r) in refused.iter().enumerate() {
        assert!(
            r.as_ref().is_err_and(|e| e.contains("in progress")),
            "refusal {i}: {r:?}"
        );
    }
    assert!(merging);
}

#[test]
fn a_reset_that_moves_nothing_or_unstages_work_isnt_done_quietly() {
    let dir = repo("reset-quiet");
    write(&dir, "a.txt", "two\n");
    commit_all(&dir, "two");
    let nowhere = plan(
        &dir,
        &WriteOp::Reset {
            commit: "HEAD".into(),
            mode: ResetMode::Mixed,
        },
    );
    let clean = plan(
        &dir,
        &WriteOp::Reset {
            commit: "HEAD~1".into(),
            mode: ResetMode::Mixed,
        },
    );
    write(&dir, "a.txt", "staged\n");
    assert!(git::run(Some(&dir), &["add", "a.txt"]).is_ok());
    let staged = plan(
        &dir,
        &WriteOp::Reset {
            commit: "HEAD~1".into(),
            mode: ResetMode::Mixed,
        },
    );
    let soft = plan(
        &dir,
        &WriteOp::Reset {
            commit: "HEAD~1".into(),
            mode: ResetMode::Soft,
        },
    );
    let _ = std::fs::remove_dir_all(&dir);
    assert!(nowhere.is_err_and(|e| e.contains("already")));
    assert!(clean.is_ok_and(|p| p.ask.is_none()));
    assert!(staged.is_ok_and(|p| p.ask.is_some_and(|a| a.undo_note.contains("doesn't stage"))));
    assert!(soft.is_ok_and(|p| p.ask.is_none()));
}
