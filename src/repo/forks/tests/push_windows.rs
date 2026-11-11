#[cfg(unix)]
#[test]
fn review19_auth_retry_pushes_only_the_recorded_rewrite_with_an_explicit_lease()
-> Result<(), Box<dyn std::error::Error>> {
    use std::os::unix::fs::PermissionsExt;
    let _pause = crate::network::test_pause::set(Some(crate::network::Pause::Metered));
    let w = World::new("auth-retry-owned");
    let p = rebase_ready(&w);
    let hook = w.fork.join("hooks/pre-receive");
    std::fs::write(
        &hook,
        "#!/bin/sh\necho 'fatal: Authentication failed' >&2\nexit 1\n",
    )?;
    std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o700))?;
    let ctx = egui::Context::default();
    let mut tab = execution_tab(&w, &p, &ctx);
    let scope = tab.forks.scope.clone().ok_or("missing fork scope")?;
    let steps = p.steps();
    let kind = normal(p.undo_steps(), p.redo_steps());
    let result = crate::repo::operations::run_steps_guarded(
        &w.me,
        &steps,
        kind.clone(),
        None,
        false,
        None,
        Some(&scope),
    )
    .0;
    assert!(
        result.as_ref().is_err_and(|e| git::is_auth_error(e)),
        "{result:?}"
    );
    // Record the pending operation just as run_ops does. Keep owned-local forge metadata
    // until the completion callback is inspected, before reload redetects local transports.
    tab.last_op = Some(crate::repo::PendingOp {
        label: "Rebase #7 onto main and push".into(),
        ops: steps,
        refresh: crate::repo::Refresh::All,
        kind,
        quiet: false,
    });
    tab.forks_op_done("Rebase #7 onto main and push", &result, &ctx);
    let rebased = git::head(&w.me);
    assert_ne!(rebased, p.head);
    let retry = tab.last_op.take().ok_or("missing authentication retry")?;
    assert!(retry.label.starts_with("Push "));
    assert!(retry.ops.iter().any(|op| {
        op.iter()
            .any(|arg| arg == &format!("--force-with-lease=refs/heads/fix:{}", p.seen))
    }));
    std::fs::remove_file(&hook)?;
    // A later local commit must not be included by retrying the original push.
    commit(&w.me, "after-auth-error.txt");
    let external = git::head(&w.me);
    let refused = run_steps(&w.me, &retry.ops, retry.kind.clone(), None).0;
    assert!(refused.is_err(), "{refused:?}");
    assert_eq!(git::resolve(&w.fork, "fix")?, p.seen);
    assert_eq!(git::head(&w.me), external);
    run(&w.me, &["reset", "--hard", &rebased]);
    let (result, kind) = run_steps(&w.me, &retry.ops, retry.kind, None);
    assert!(result.is_ok(), "{result:?}");
    assert_eq!(git::resolve(&w.fork, "fix")?, rebased);
    let entry = persisted(custom(kind).ok_or("missing undo entry")?)?;
    assert!(entry.undo.iter().any(|op| {
        op.iter()
            .any(|arg| arg == &format!("--force-with-lease=refs/heads/fix:{rebased}"))
    }));
    let result = run_steps(&w.me, &entry.undo.clone(), OpKind::Undo(entry), None).0;
    assert!(result.is_ok(), "{result:?}");
    assert_eq!(
        git::head(&w.me),
        rebased,
        "fork-only undo preserves the completed local rewrite"
    );
    assert_eq!(git::resolve(&w.fork, "fix")?, p.head);
    Ok(())
}

#[cfg(unix)]
fn push_window(mode: &str) -> Result<(), Box<dyn std::error::Error>> {
    use std::os::unix::fs::PermissionsExt;
    let w = World::new(&format!("review20-{mode}-push-window"));
    let p = rebase_ready(&w);
    let mut kind = normal(p.undo_steps(), p.redo_steps());
    let mut steps = p.steps();
    if mode != "normal" {
        let (result, done) = run_steps(&w.me, &steps, kind, None);
        assert!(result.is_ok(), "{result:?}");
        let entry = persisted(custom(done).ok_or("missing completed rebase")?)?;
        if mode == "redo" {
            let (result, undone) = run_steps(&w.me, &entry.undo.clone(), OpKind::Undo(entry), None);
            assert!(result.is_ok(), "{result:?}");
            let OpKind::Undo(entry) = undone else {
                return Err("missing undone entry".into());
            };
            let entry = persisted(entry)?;
            steps = entry.redo.clone();
            kind = OpKind::Redo(entry);
        } else {
            steps = entry.undo.clone();
            kind = OpKind::Undo(entry);
        }
    }
    let ready = w.root.join("hook-ready");
    let release = w.root.join("hook-release");
    let quote = |p: &Path| format!("'{}'", arg(p).replace('\'', "'\\''"));
    let hook = format!(
        "#!/bin/sh\n: > {}\ni=0\nwhile [ ! -f {} ] && [ \"$i\" -lt 1500 ]; do\n i=$((i+1))\n sleep 0.01\ndone\n[ -f {} ]\n",
        quote(&ready),
        quote(&release),
        quote(&release)
    );
    let hook_path = w.me.join(".git/hooks/pre-push");
    std::fs::write(&hook_path, hook)?;
    std::fs::set_permissions(&hook_path, std::fs::Permissions::from_mode(0o700))?;
    let me = w.me.clone();
    let worker = std::thread::spawn(move || run_steps(&me, &steps, kind, None));
    for _ in 0..1000 {
        if ready.exists() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    // Release even on setup failure: only this owned Git child can wait on these files.
    if !ready.exists() {
        std::fs::write(&release, "release")?;
        let _ = worker.join();
        return Err("owned pre-push hook never became ready".into());
    }
    let rewritten = git::head(&w.me);
    commit(&w.me, "external-during-push.txt");
    let external = git::head(&w.me);
    std::fs::write(&release, "release")?;
    let (result, done) = worker.join().map_err(|_| "owned push worker panicked")?;
    std::fs::remove_file(hook_path)?;
    if mode == "undo" {
        assert!(
            result.is_err(),
            "undo reset a branch changed while pushing: {result:?}"
        );
        assert_eq!(git::head(&w.me), external);
        assert_eq!(git::resolve(&w.fork, "fix")?, p.head);
        return Ok(());
    }
    assert!(result.is_ok(), "{result:?}");
    assert_eq!(
        git::resolve(&w.fork, "fix")?,
        rewritten,
        "the push itself uses pinned R"
    );
    assert_eq!(git::head(&w.me), external);
    let entry = if mode == "normal" {
        custom(done).ok_or("missing normal undo")?
    } else {
        let OpKind::Redo(entry) = done else {
            return Err("missing redo result".into());
        };
        entry
    };
    let entry = persisted(entry)?;
    let result = run_steps(&w.me, &entry.undo.clone(), OpKind::Undo(entry), None).0;
    assert!(
        result.is_err(),
        "undo accepted external C after a pinned R push: {result:?}"
    );
    assert_eq!(git::head(&w.me), external);
    assert_eq!(git::resolve(&w.fork, "fix")?, rewritten);
    Ok(())
}
#[cfg(unix)]
#[test]
fn review20_normal_push_cannot_bind_local_undo_to_a_commit_created_in_its_hook()
-> Result<(), Box<dyn std::error::Error>> {
    push_window("normal")
}
#[cfg(unix)]
#[test]
fn review20_redo_push_cannot_bind_local_undo_to_a_commit_created_in_its_hook()
-> Result<(), Box<dyn std::error::Error>> {
    push_window("redo")
}
#[cfg(unix)]
#[test]
fn review20_undo_cannot_reset_a_commit_created_in_its_push_hook()
-> Result<(), Box<dyn std::error::Error>> {
    push_window("undo")
}
