#[cfg(unix)]
fn web_checkout(
    w: &World,
) -> Result<(plan::Checkout, super::guard::Scope), Box<dyn std::error::Error>> {
    use crate::forge::{Forge, Hosting};
    use std::os::unix::fs::PermissionsExt;
    let primary = git::Remote {
        name: "origin".into(),
        url: "https://github.com/acme/app.git".into(),
    };
    run(&w.me, &["remote", "set-url", "origin", &primary.url]);
    run(
        &w.me,
        &[
            "config",
            "url.git@github.com:ann/.insteadOf",
            "https://github.com/ann/",
        ],
    );
    let quote = |p: &Path| format!("'{}'", arg(p).replace('\'', "'\\''"));
    let ssh = w.root.join("checkout-owned-ssh");
    std::fs::write(
        &ssh,
        format!(
            "#!/bin/sh\nallowed=no\nfor arg do\n [ \"$arg\" = \"git-upload-pack 'ann/app.git'\" ] && allowed=yes\ndone\n[ \"$allowed\" = yes ] || exit 97\nexec git-upload-pack {}\n",
            quote(&w.fork)
        ),
    )?;
    std::fs::set_permissions(&ssh, std::fs::Permissions::from_mode(0o700))?;
    run(&w.me, &["config", "core.sshCommand", &quote(&ssh)]);
    run(&w.me, &["config", "ssh.variant", "ssh"]);
    let h = Hosting {
        forge: Forge::GitHub,
        host: "github.com".into(),
        web: "https://github.com".into(),
        path: "acme/app".into(),
    };
    let mut info = w.info();
    info.clone_https = "https://github.com/ann/app.git".into();
    info.clone_ssh = "git@github.com:ann/app.git".into();
    let p = plan::checkout(
        &info,
        std::slice::from_ref(&primary),
        &w.setup(),
        &["main".into()],
        "main",
        Some(&h),
    )?;
    let fork = Hosting {
        path: "ann/app".into(),
        ..h.clone()
    };
    let scope = super::guard::Scope::new(1, h, primary).with_fork(
        p.remote.clone(),
        p.url.clone(),
        fork,
        p.new_remote,
    );
    Ok((p, scope))
}
#[cfg(unix)]
#[test]
fn review20_checkout_redo_rejects_a_new_wrong_repository_rewrite_without_scope()
-> Result<(), Box<dyn std::error::Error>> {
    let w = World::new("review20-redo-checkout");
    let evil = World::new("review20-other-repository");
    let (p, scope) = web_checkout(&w)?;
    let (result, done, _) = crate::repo::operations::run_steps_guarded(
        &w.me,
        &p.steps,
        normal(p.undo, p.steps.clone()),
        None,
        false,
        None,
        Some(&scope),
    );
    assert!(result.is_ok(), "{result:?}");
    drop(scope);
    let entry = persisted(custom(done).ok_or("missing checkout undo")?)?;
    let (result, done) = run_steps(&w.me, &entry.undo.clone(), OpKind::Undo(entry), None);
    assert!(result.is_ok(), "{result:?}");
    let OpKind::Undo(entry) = done else {
        return Err("missing undone checkout".into());
    };
    let entry = persisted(entry)?;
    run(
        &w.me,
        &["config", "--unset", "url.git@github.com:ann/.insteadOf"],
    );
    run(
        &w.me,
        &[
            "config",
            &format!("url.{}.insteadOf", arg(&evil.fork)),
            "https://github.com/ann/app.git",
        ],
    );
    let before = git::head(&w.me);
    let result = run_steps(&w.me, &entry.redo.clone(), OpKind::Redo(entry), None).0;
    assert!(
        result.is_err(),
        "checkout redo accepted a wrong repository: {result:?}"
    );
    assert_eq!(git::head(&w.me), before);
    assert!(git::resolve(&w.me, "refs/heads/fix").is_err());
    assert!(git::resolve(&w.me, "refs/remotes/ann/fix").is_err());
    assert!(git::run(Some(&w.me), &["config", "--get", "remote.ann.gitcraftFork"]).is_err());
    Ok(())
}

#[cfg(unix)]
#[test]
fn review20_checkout_canonical_rewrite_roundtrips_without_scope()
-> Result<(), Box<dyn std::error::Error>> {
    let w = World::new("review20-valid-replay");
    let (p, scope) = web_checkout(&w)?;
    let (result, done, _) = crate::repo::operations::run_steps_guarded(
        &w.me,
        &p.steps,
        normal(p.undo, p.steps.clone()),
        None,
        false,
        None,
        Some(&scope),
    );
    assert!(result.is_ok(), "{result:?}");
    drop(scope);
    let mut entry = persisted(custom(done).ok_or("missing checkout undo")?)?;
    for _ in 0..2 {
        let (result, done) = run_steps(&w.me, &entry.undo.clone(), OpKind::Undo(entry), None);
        assert!(result.is_ok(), "{result:?}");
        assert_eq!(run(&w.me, &["branch", "--show-current"]).trim(), "main");
        assert!(git::remotes(&w.me)?.iter().all(|r| r.name != "ann"));
        let OpKind::Undo(undone) = done else {
            return Err("missing checkout undo result".into());
        };
        entry = persisted(undone)?;
        let (result, done) = run_steps(&w.me, &entry.redo.clone(), OpKind::Redo(entry), None);
        assert!(result.is_ok(), "{result:?}");
        assert_eq!(git::head(&w.me), w.info().sha);
        assert_eq!(
            run(&w.me, &["remote", "get-url", "ann"]).trim(),
            "git@github.com:ann/app.git"
        );
        let OpKind::Redo(redone) = done else {
            return Err("missing checkout redo result".into());
        };
        entry = persisted(redone)?;
    }
    Ok(())
}

#[cfg(unix)]
#[test]
fn review20_checkout_undo_rejects_push_url_changes_and_missing_remotes_before_checkout()
-> Result<(), Box<dyn std::error::Error>> {
    let w = World::new("review20-undo-destinations");
    let (p, scope) = web_checkout(&w)?;
    let (result, done, _) = crate::repo::operations::run_steps_guarded(
        &w.me,
        &p.steps,
        normal(p.undo, p.steps.clone()),
        None,
        false,
        None,
        Some(&scope),
    );
    assert!(result.is_ok(), "{result:?}");
    drop(scope);
    let entry = persisted(custom(done).ok_or("missing checkout undo")?)?;
    let head = git::head(&w.me);
    run(
        &w.me,
        &[
            "config",
            "remote.ann.pushurl",
            "https://github.com:444/ann/app.git",
        ],
    );
    let result = run_steps(&w.me, &entry.undo, OpKind::Undo(entry.clone()), None).0;
    assert!(result.is_err(), "{result:?}");
    assert_eq!(run(&w.me, &["branch", "--show-current"]).trim(), "fix");
    assert_eq!(git::head(&w.me), head);
    run(&w.me, &["remote", "remove", "ann"]);
    let result = run_steps(
        &w.me,
        &entry.undo.clone(),
        OpKind::Undo(persisted(entry)?),
        None,
    )
    .0;
    assert!(result.is_err(), "{result:?}");
    assert_eq!(run(&w.me, &["branch", "--show-current"]).trim(), "fix");
    assert_eq!(git::head(&w.me), head);
    Ok(())
}

#[cfg(unix)]
#[test]
fn review20_checkout_redo_retry_retains_the_initial_effective_destination_pin()
-> Result<(), Box<dyn std::error::Error>> {
    use std::os::unix::fs::PermissionsExt;
    let w = World::new("review20-replay-retry");
    let (p, scope) = web_checkout(&w)?;
    let (result, done, _) = crate::repo::operations::run_steps_guarded(
        &w.me,
        &p.steps,
        normal(p.undo, p.steps.clone()),
        None,
        false,
        None,
        Some(&scope),
    );
    assert!(result.is_ok(), "{result:?}");
    drop(scope);
    let entry = persisted(custom(done).ok_or("missing checkout undo")?)?;
    let (result, done) = run_steps(&w.me, &entry.undo.clone(), OpKind::Undo(entry), None);
    assert!(result.is_ok(), "{result:?}");
    let OpKind::Undo(entry) = done else {
        return Err("missing undone checkout".into());
    };
    let entry = persisted(entry)?;
    let original_ssh = run(&w.me, &["config", "core.sshCommand"]);
    let failure = w.root.join("owned-auth-failure");
    let recording = w.root.join("retry-recording");
    let quote = |p: &Path| format!("'{}'", arg(p).replace('\'', "'\\''"));
    std::fs::write(
        &failure,
        format!(
            "#!/bin/sh\nprintf failed >> {}\necho 'fatal: Authentication failed' >&2\nexit 1\n",
            quote(&recording)
        ),
    )?;
    std::fs::set_permissions(&failure, std::fs::Permissions::from_mode(0o700))?;
    run(&w.me, &["config", "core.sshCommand", &quote(&failure)]);
    let ops = entry.redo.clone();
    let (result, kind, stopped) = crate::repo::operations::run_steps_counted(
        &w.me,
        &ops,
        OpKind::Redo(entry),
        None,
        false,
        None,
    );
    assert!(
        result.as_ref().is_err_and(|e| git::is_auth_error(e)),
        "{result:?}"
    );
    assert_eq!(std::fs::read_to_string(&recording)?, "failed");
    let ctx = egui::Context::default();
    let mut tab = crate::repo::RepoTab::new_deferred(w.me.clone(), &ctx);
    tab.last_op = Some(crate::repo::PendingOp {
        label: "Retry checkout".into(),
        ops,
        refresh: crate::repo::Refresh::All,
        kind,
        quiet: false,
    });
    tab.keep_unfinished(stopped);
    let retry = tab.last_op.take().ok_or("missing checkout retry")?;
    // Canonical SSH spelling, but changed after the initial effective URLs were pinned.
    run(
        &w.me,
        &[
            "config",
            "remote.ann.pushurl",
            "ssh://git@github.com/ann/app.git",
        ],
    );
    let result = run_steps(&w.me, &retry.ops, retry.kind.clone(), None).0;
    assert!(result.is_err(), "{result:?}");
    assert_eq!(
        std::fs::read_to_string(&recording)?,
        "failed",
        "changed destination was refused before retrying fetch"
    );
    assert!(git::resolve(&w.me, "refs/heads/fix").is_err());
    run(&w.me, &["config", "--unset", "remote.ann.pushurl"]);
    run(&w.me, &["remote", "remove", "ann"]);
    let result = run_steps(&w.me, &retry.ops, retry.kind.clone(), None).0;
    assert!(
        result.is_err(),
        "a retry may not silently replace a removed remote: {result:?}"
    );
    run(
        &w.me,
        &["remote", "add", "ann", "https://github.com/ann/app.git"],
    );
    run(&w.me, &["config", "core.sshCommand", original_ssh.trim()]);
    let result = run_steps(&w.me, &retry.ops, retry.kind, None).0;
    assert!(result.is_ok(), "{result:?}");
    assert_eq!(git::head(&w.me), w.info().sha);
    Ok(())
}
