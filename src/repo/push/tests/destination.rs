#[test]
fn a_branch_with_no_upstream_is_published_and_tracks_it() -> Result<(), String> {
    let w = world("publish")?;
    run(&w.mine, &["checkout", "-q", "-b", "feature"])?;
    let tip = commit(&w.mine, "feature")?;
    let steps = [publish_args("feature", "origin")];
    let (result, _) = run_steps(&w.mine, &steps, OpKind::plain(), None);
    assert!(result.is_ok(), "{result:?}");
    assert_eq!(git::resolve(&w.bare, "refs/heads/feature"), Ok(tip));
    let upstream = run(
        &w.mine,
        &["rev-parse", "--abbrev-ref", "feature@{upstream}"],
    )?;
    assert_eq!(upstream.trim(), "origin/feature");
    Ok(())
}

/// A first push refused because the remote has a branch of that name with other commits: it
/// is checked where it went, and the force push sets the upstream as the first push would have.
#[test]
fn a_refused_first_push_is_checked_where_it_went() -> Result<(), String> {
    let w = world("publish-refused")?;
    run(&w.theirs, &["checkout", "-q", "-b", "feature"])?;
    commit(&w.theirs, "their feature")?;
    run(&w.theirs, &["push", "-q", "-u", "origin", "feature"])?;
    run(&w.mine, &["checkout", "-q", "-b", "feature"])?;
    commit(&w.mine, "my feature")?;
    let steps = [publish_args("feature", "origin")];
    let (result, _) = run_steps(&w.mine, &steps, OpKind::plain(), None);
    let error = result.err().ok_or("the first push must be refused")?;
    let (line, why) =
        porcelain::refused(&error, "refs/heads/feature", None).ok_or("no ref line")?;
    assert_eq!(why, Refusal::Behind);
    let mut q = question("feature", Some(origin("feature")), &line.to, &["origin"]);
    q.publish = true;
    let r = rejection(check::check(&w.mine, &q, None)?)?;
    assert_eq!(r.shape, Shape::Diverged);
    assert!(r.publish);
    let (forced, _) = run_steps(&w.mine, &force_push_ops(&r), OpKind::plain(), None);
    assert!(forced.is_ok(), "{forced:?}");
    let upstream = run(
        &w.mine,
        &["rev-parse", "--abbrev-ref", "feature@{upstream}"],
    )?;
    assert_eq!(upstream.trim(), "origin/feature");
    Ok(())
}

/// A hook on the remote says no: the refusal is the remote's, with its words.
#[cfg(unix)]
#[test]
fn a_hook_refusal_is_the_remotes_and_keeps_its_message() -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    let w = world("hook")?;
    let hook = w.bare.join("hooks").join("pre-receive");
    std::fs::write(
        &hook,
        "#!/bin/sh\necho 'main is protected here' >&2\nexit 1\n",
    )
    .map_err(|e| e.to_string())?;
    std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o755))
        .map_err(|e| e.to_string())?;
    commit(&w.mine, "mine")?;
    let error = plain_push(&w.mine).ok_or("the hook must refuse")?;
    let (_, why) = refused_line(&error)?;
    assert!(matches!(why, Refusal::Remote(_)), "{error}");
    assert!(error.contains("main is protected here"), "{error}");
    Ok(())
}

#[test]
fn a_remote_branch_deleted_is_gone() -> Result<(), String> {
    let w = world("gone")?;
    let q = question(
        "main",
        Some(origin("elsewhere")),
        "refs/heads/elsewhere",
        &["origin"],
    );
    let checked = check::check(&w.mine, &q, None)?;
    let gone = Checked::Gone {
        branch: "main".to_owned(),
        target: origin("elsewhere"),
        publish: false,
    };
    assert_eq!(checked, gone);
    Ok(())
}

/// With two remotes, the check goes where git pushes: `branch.<b>.pushRemote` first.
#[test]
fn the_push_remote_is_the_one_checked() -> Result<(), String> {
    let w = world("push-remote")?;
    let fork = path(&w.root.join("fork.git"));
    run(&w.root, &["clone", "-q", "--bare", &path(&w.bare), &fork])?;
    run(&w.mine, &["remote", "add", "fork", &fork])?;
    run(&w.mine, &["config", "branch.main.pushRemote", "fork"])?;
    // Their commit on the fork only.
    run(&w.theirs, &["remote", "add", "fork", &fork])?;
    commit(&w.theirs, "on the fork")?;
    run(&w.theirs, &["push", "-q", "fork", "main"])?;
    commit(&w.mine, "mine")?;
    let error = plain_push(&w.mine).ok_or("the fork must refuse")?;
    let (line, _) = refused_line(&error)?;
    let q = question("main", None, &line.to, &["origin", "fork"]);
    let r = rejection(check::check(&w.mine, &q, None)?)?;
    assert_eq!(r.target.remote, "fork");
    assert_eq!(r.missing[0].subject, "on the fork");
    Ok(())
}

/// A commit pushed, then undone here: the branch is only behind, but what the remote has was
/// here, so it is told as a rewrite (with the force push), not "pull to catch up".
#[test]
fn an_undone_commit_after_pushing_is_a_rewrite() -> Result<(), String> {
    let w = world("undone")?;
    commit(&w.mine, "to undo")?;
    run(&w.mine, &["push", "-q"])?;
    run(&w.mine, &["reset", "-q", "--keep", "HEAD~1"])?;
    let r = push_and_check(&w)?;
    assert_eq!((r.ahead, r.behind), (0, 1));
    assert_eq!(r.shape, Shape::Rewritten);
    let (forced, _) = run_steps(&w.mine, &force_push_ops(&r), OpKind::plain(), None);
    assert!(forced.is_ok(), "{forced:?}");
    assert_eq!(remote_main(&w), git::head(&w.mine));
    Ok(())
}

/// Fetched from one repository, pushed to another (`pushurl`): what is checked is the
/// repository the push was refused by.
#[test]
fn the_push_url_is_the_one_checked() -> Result<(), String> {
    let w = world("push-url")?;
    let fork = path(&w.root.join("fork.git"));
    run(&w.root, &["clone", "-q", "--bare", &path(&w.bare), &fork])?;
    run(&w.mine, &["config", "remote.origin.pushurl", &fork])?;
    run(&w.theirs, &["remote", "add", "fork", &fork])?;
    commit(&w.theirs, "only on the push url")?;
    run(&w.theirs, &["push", "-q", "fork", "main"])?;
    commit(&w.mine, "mine")?;
    let r = push_and_check(&w)?;
    assert_eq!(r.target, origin("main"));
    assert_eq!(r.missing[0].subject, "only on the push url");
    assert_eq!(
        git::resolve(Path::new(&fork), "refs/heads/main"),
        Ok(r.remote.clone())
    );
    Ok(())
}

#[test]
fn a_refused_push_is_looked_at_where_the_line_says() {
    // A publish sent to `me/main` by the remote's push refspec: that is what is checked.
    assert_eq!(
        refused_target(Some(origin("main")), "refs/heads/me/main"),
        Some(Some(origin("me/main")))
    );
    // A plain push: the remote is left to git's config.
    assert_eq!(refused_target(None, "refs/heads/main"), Some(None));
    // Gerrit's review refs are no branch to look at or replace.
    assert_eq!(refused_target(Some(origin("main")), "refs/for/main"), None);
}
