#[cfg(unix)]
#[test]
fn review19_first_checkout_accepts_canonical_https_to_ssh_rewrite() {
    use crate::forge::{Forge, Hosting};
    use std::os::unix::fs::PermissionsExt;
    let w = World::new("rewrite-first-checkout");
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
    // SSH is an owned local upload-pack fixture: it cannot connect to a forge.
    let quoted = |p: &Path| format!("'{}'", arg(p).replace('\'', "'\\''"));
    let ssh = w.root.join("local ssh");
    let log = w.root.join("ssh-recording");
    let script = format!(
        "#!/bin/sh\nallowed=no\nfor arg do\n  [ \"$arg\" = \"git-upload-pack 'ann/app.git'\" ] && allowed=yes\ndone\n[ \"$allowed\" = yes ] || exit 97\nprintf fetch >> {}\nexec git-upload-pack {}\n",
        quoted(&log),
        quoted(&w.fork)
    );
    assert!(std::fs::write(&ssh, script).is_ok());
    assert!(std::fs::set_permissions(&ssh, std::fs::Permissions::from_mode(0o700)).is_ok());
    run(&w.me, &["config", "core.sshCommand", &quoted(&ssh)]);
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
    );
    assert!(p.is_ok(), "{p:?}");
    let Ok(p) = p else { return };
    assert!(p.new_remote);
    assert_eq!(p.url, info.clone_https);
    let fork_hosting = Hosting {
        path: info.repo_path.clone(),
        ..h.clone()
    };
    let scope = super::guard::Scope::new(1, h, primary).with_fork(
        p.remote.clone(),
        p.url.clone(),
        fork_hosting,
        true,
    );
    let (result, _, _) = crate::repo::operations::run_steps_guarded(
        &w.me,
        &p.steps,
        normal(p.undo, p.steps.clone()),
        None,
        false,
        None,
        Some(&scope),
    );
    assert!(result.is_ok(), "{result:?}");
    assert_eq!(run(&w.me, &["branch", "--show-current"]).trim(), "fix");
    assert_eq!(
        run(&w.me, &["config", "branch.fix.pushRemote"]).trim(),
        "ann"
    );
    assert_eq!(
        run(&w.me, &["config", "remote.ann.gitcraftFork"]).trim(),
        "true"
    );
    assert_eq!(
        run(&w.me, &["remote", "get-url", "ann"]).trim(),
        "git@github.com:ann/app.git"
    );
    assert_eq!(std::fs::read_to_string(log).unwrap_or_default(), "fetch");
    assert_eq!(git::head(&w.me), w.info().sha);
}

// A real refresh must rediscover the same forge while these fixtures wait for the
// separate push checklist. The SSH spelling supplies that identity; this strict
// helper executes only the owned upstream's upload-pack, never a network client.
fn refresh_stable_rebase(
    w: &World,
    ctx: &egui::Context,
) -> Result<(plan::Rebase, crate::repo::RepoTab), Box<dyn std::error::Error>> {
    let _ = rebase_ready(w);
    let quoted = |p: &Path| format!("'{}'", arg(p).replace('\'', "'\\''"));
    let ssh = w.root.join("owned upstream ssh");
    let script = format!(
        "#!/bin/sh\nhost=no\ncommand=no\nfor arg do\n  [ \"$arg\" = \"git@forge.example\" ] && host=yes\n  [ \"$arg\" = \"git-upload-pack 'origin/app'\" ] && command=yes\ndone\n[ \"$host:$command\" = yes:yes ] || exit 97\nexec git-upload-pack {}\n",
        quoted(&w.upstream)
    );
    assert!(std::fs::write(&ssh, script).is_ok());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert!(std::fs::set_permissions(&ssh, std::fs::Permissions::from_mode(0o700)).is_ok());
    }
    run(&w.me, &["config", "core.sshCommand", &quoted(&ssh)]);
    run(&w.me, &["config", "ssh.variant", "ssh"]);
    run(&w.me, &["config", "gitcraft.forge", "forgejo"]);
    run(
        &w.me,
        &[
            "remote",
            "set-url",
            "origin",
            "git@forge.example:origin/app",
        ],
    );
    let remotes = git::remotes(&w.me)?;
    let primary = remotes.iter().find(|r| r.name == "origin").cloned();
    assert!(primary.is_some());
    let primary = primary.ok_or("owned origin is configured above")?;
    let p = plan::plan_rebase(&w.me, &w.info(), &primary, &w.setup(), None, None, None);
    assert!(p.is_ok(), "{p:?}");
    let p = p?;
    let mut tab = crate::repo::RepoTab::new_deferred(w.me.clone(), ctx);
    tab.forge_remotes = crate::forge::detect_remotes(&w.me, &remotes);
    tab.remotes = remotes;
    tab.take_forge_selection(false, ctx);
    let hosting = tab.hosting.clone();
    assert!(hosting.is_some());
    if let Some(hosting) = hosting {
        tab.forks.scope = Some(super::guard::Scope::new(
            tab.forge_generation,
            hosting,
            primary,
        ));
    }
    tab.forks.info.insert(7, w.info());
    let label = "Rebase #7 onto main and push".to_owned();
    tab.forks.scope_label = Some(label.clone());
    tab.forks.running = Some((label, p.clone()));
    Ok((p, tab))
}

fn execution_tab(w: &World, p: &plan::Rebase, ctx: &egui::Context) -> crate::repo::RepoTab {
    let mut tab = recorded_tab(w, ctx);
    tab.remotes = git::remotes(&w.me).unwrap_or_default();
    tab.forge_remotes.truncate(1);
    tab.take_forge_selection(true, ctx);
    assert!(tab.hosting.is_some());
    if let Some(hosting) = tab.hosting.clone() {
        let primary = git::Remote {
            name: "origin".into(),
            url: arg(&w.upstream),
        };
        tab.forks.scope = Some(super::guard::Scope::new(
            tab.forge_generation,
            hosting,
            primary,
        ));
        let label = "Rebase #7 onto main and push".to_owned();
        tab.forks.scope_label = Some(label.clone());
        tab.forks.running = Some((label, p.clone()));
    }
    tab
}
#[test]
fn review19_rejected_rebase_does_not_offer_undo_of_an_external_commit() {
    let _pause = crate::network::test_pause::set(Some(crate::network::Pause::Metered));
    let w = World::new("external-before-confirm");
    let p = rebase_ready(&w);
    let ctx = egui::Context::default();
    let mut tab = execution_tab(&w, &p, &ctx);
    commit(&w.me, "external.txt");
    let external = git::head(&w.me);
    let id = confirmation_id(&tab);
    tab.fork_act(super::ForkAct::ConfirmRebase(id), &ctx);
    drain_until(&mut tab, &ctx, |t| t.busy_op.is_none() && !t.loading);
    for _ in 0..50 {
        tab.poll(&ctx);
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert_eq!(git::head(&w.me), external);
    assert_eq!(git::resolve(&w.fork, "fix").unwrap_or_default(), p.seen);
    assert!(
        !tab.timeline.entries.iter().any(|e| e.undo.is_some()),
        "unrelated commit was offered as our rewrite: {:?}",
        tab.timeline
            .entries
            .iter()
            .map(|e| &e.label)
            .collect::<Vec<_>>()
    );
    assert!(tab.forks.after_conflict.is_none());
}
#[test]
fn review19_undo_lease_is_independent_of_fetch_refspecs_and_persists() {
    let w = World::new("undo-explicit-lease");
    let p = rebase_ready(&w);
    let (result, kind) = run_steps(
        &w.me,
        &p.steps(),
        normal(p.undo_steps(), p.redo_steps()),
        None,
    );
    assert!(result.is_ok(), "{result:?}");
    let entry = custom(kind);
    assert!(entry.is_some());
    let Some(entry) = entry else { return };
    let encoded = serde_json::to_string(&entry).unwrap_or_default();
    let restored: Result<UndoEntry, _> = serde_json::from_str(&encoded);
    assert!(restored.is_ok());
    let Ok(entry) = restored else { return };
    let rebased = git::head(&w.me);
    run(
        &w.me,
        &[
            "config",
            "remote.ann.fetch",
            "+refs/heads/*:refs/remotes/ann-new/*",
        ],
    );
    run(&w.ann, &["fetch", "-q", "fork"]);
    run(&w.ann, &["checkout", "-q", "-B", "fix", "fork/fix"]);
    commit(&w.ann, "contributor-after-push.txt");
    run(&w.ann, &["push", "-q", "fork", "fix"]);
    let theirs = git::head(&w.ann);
    run(&w.me, &["fetch", "-q", "ann"]);
    assert_eq!(
        git::resolve(&w.me, &p.tracking).unwrap_or_default(),
        rebased
    );
    let result = run_steps(&w.me, &entry.undo.clone(), OpKind::Undo(entry), None).0;
    assert!(
        result.is_err(),
        "undo replaced the contributor's new commit: {result:?}"
    );
    assert_eq!(git::resolve(&w.fork, "fix").unwrap_or_default(), theirs);
    assert_eq!(git::head(&w.me), rebased);
}
