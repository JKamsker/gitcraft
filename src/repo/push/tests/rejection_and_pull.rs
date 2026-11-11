#[test]
fn a_branch_only_behind_is_told_so() -> Result<(), String> {
    let w = world("behind")?;
    commit(&w.theirs, "theirs")?;
    run(&w.theirs, &["push", "-q"])?;
    // `mine` has nothing new, but is behind: pushing its main would take the remote back.
    let r = push_and_check(&w)?;
    assert_eq!((r.ahead, r.behind), (0, 1));
    assert_eq!(r.shape, Shape::Behind);
    assert_eq!(r.remote, remote_main(&w));
    assert_eq!(r.local, git::head(&w.mine));
    assert_eq!(r.missing.len(), 1);
    assert_eq!(r.missing[0].subject, "theirs");
    assert_eq!(r.target, origin("main"));
    Ok(())
}

#[test]
fn both_moving_on_is_diverged_not_rewritten() -> Result<(), String> {
    let w = world("diverged")?;
    commit(&w.theirs, "theirs")?;
    run(&w.theirs, &["push", "-q"])?;
    commit(&w.mine, "mine")?;
    let r = push_and_check(&w)?;
    assert_eq!((r.ahead, r.behind), (1, 1));
    assert_eq!(r.shape, Shape::Diverged);
    Ok(())
}

#[test]
fn an_amend_after_pushing_is_a_rewrite() -> Result<(), String> {
    let w = world("rewritten")?;
    commit(&w.mine, "first")?;
    run(&w.mine, &["push", "-q"])?;
    amend(&w.mine, "first, better")?;
    let r = push_and_check(&w)?;
    assert_eq!((r.ahead, r.behind), (1, 1));
    assert_eq!(r.shape, Shape::Rewritten);
    assert_eq!(r.missing[0].subject, "first");
    Ok(())
}

/// Pushed, then someone else pushed on top, then amended here: their commit was never here, so
/// it is not called a rewrite of mine (the force push would drop their work).
#[test]
fn a_commit_by_someone_else_on_top_is_not_a_rewrite() -> Result<(), String> {
    let w = world("rewritten-and-theirs")?;
    commit(&w.mine, "first")?;
    run(&w.mine, &["push", "-q"])?;
    run(&w.theirs, &["pull", "-q"])?;
    commit(&w.theirs, "theirs")?;
    run(&w.theirs, &["push", "-q"])?;
    amend(&w.mine, "first, better")?;
    let r = push_and_check(&w)?;
    assert_eq!(r.shape, Shape::Diverged);
    let subjects: Vec<&str> = r.missing.iter().map(|c| c.subject.as_str()).collect();
    assert_eq!(subjects, ["theirs", "first"]);
    Ok(())
}

/// Someone pushes after the check: the force push is refused and their commit stays; checked
/// again, it is named, and the force push goes.
#[test]
fn a_lease_that_moved_refuses_the_force_push() -> Result<(), String> {
    let w = world("lease")?;
    commit(&w.mine, "first")?;
    run(&w.mine, &["push", "-q"])?;
    amend(&w.mine, "amended")?;
    let r = push_and_check(&w)?;
    // Meanwhile.
    run(&w.theirs, &["pull", "-q"])?;
    let theirs = commit(&w.theirs, "theirs, after the check")?;
    run(&w.theirs, &["push", "-q"])?;
    let (forced, _) = run_steps(&w.mine, &force_push_ops(&r), OpKind::plain(), None);
    let error = forced.err().ok_or("the lease must refuse")?;
    let why = porcelain::refused(&error, "refs/heads/main", Some(&r.local)).map(|(_, why)| why);
    assert_eq!(why, Some(Refusal::Moved), "{error}");
    assert_eq!(remote_main(&w), theirs);
    let mut q = question(
        "main",
        Some(r.target.clone()),
        "refs/heads/main",
        &["origin"],
    );
    q.moved = true;
    let again = rejection(check::check(&w.mine, &q, None)?)?;
    assert!(again.moved);
    assert_eq!(again.remote, theirs);
    let named = again.missing.iter().map(|c| c.subject.as_str());
    assert!(named.into_iter().any(|s| s == "theirs, after the check"));
    let (forced, _) = run_steps(&w.mine, &force_push_ops(&again), OpKind::plain(), None);
    assert!(forced.is_ok(), "{forced:?}");
    assert_eq!(remote_main(&w), git::head(&w.mine));
    Ok(())
}

/// The branch here moves after the check: the force push pushes nothing.
#[test]
fn a_branch_that_moved_here_refuses_the_force_push() -> Result<(), String> {
    let w = world("local-moved")?;
    commit(&w.theirs, "theirs")?;
    run(&w.theirs, &["push", "-q"])?;
    commit(&w.mine, "mine")?;
    let r = push_and_check(&w)?;
    let before = remote_main(&w);
    commit(&w.mine, "mine, after the check")?;
    let (forced, _) = run_steps(&w.mine, &force_push_ops(&r), OpKind::plain(), None);
    assert!(forced.is_err());
    assert_eq!(remote_main(&w), before);
    Ok(())
}

#[test]
fn pull_then_push_joins_the_two() -> Result<(), String> {
    for mode in [PullMode::Rebase, PullMode::Merge] {
        let w = world(&format!("pull-push-{mode:?}"))?;
        let theirs = commit(&w.theirs, "theirs")?;
        run(&w.theirs, &["push", "-q"])?;
        commit(&w.mine, "mine")?;
        let r = push_and_check(&w)?;
        let steps = [
            pull_from(mode, &r.target),
            ops(&[&["push", "--porcelain"]]).remove(0),
        ];
        let (result, _) = run_steps(&w.mine, &steps, OpKind::plain(), None);
        assert!(result.is_ok(), "{mode:?}: {result:?}");
        let remote = remote_main(&w);
        assert_eq!(remote, git::head(&w.mine));
        run(&w.mine, &["merge-base", "--is-ancestor", &theirs, &remote])?;
        let subjects = run(&w.mine, &["log", "--format=%s", &remote])?;
        assert!(subjects.lines().any(|s| s == "mine"), "{mode:?}");
    }
    Ok(())
}

/// A pull that conflicts stops there: nothing is pushed.
#[test]
fn a_conflicting_pull_pushes_nothing() -> Result<(), String> {
    let w = world("conflict")?;
    commit_file(&w.theirs, "theirs\n")?;
    run(&w.theirs, &["push", "-q"])?;
    commit_file(&w.mine, "mine\n")?;
    let before = remote_main(&w);
    let steps = [
        pull_from(PullMode::Merge, &origin("main")),
        ops(&[&["push", "--porcelain"]]).remove(0),
    ];
    let (result, _) = run_steps(&w.mine, &steps, OpKind::plain(), None);
    let error = result.err().ok_or("the pull must conflict")?;
    assert!(error.contains("CONFLICT"), "{error}");
    assert_eq!(remote_main(&w), before);
    Ok(())
}
