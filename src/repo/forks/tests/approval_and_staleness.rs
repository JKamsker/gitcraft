type ApprovalRecording = Box<dyn FnMut(u64) -> Result<(), String> + Send>;
thread_local! {
    static RECORD_APPROVAL: std::cell::RefCell<Option<ApprovalRecording>> = const { std::cell::RefCell::new(None) };
}
pub(super) fn take_approval_recording() -> Option<ApprovalRecording> {
    RECORD_APPROVAL.with(|slot| slot.borrow_mut().take())
}
#[test]
fn review14_approval_blocks_selection_and_external_rebind_stops_remaining_runs() {
    let _pause = crate::network::test_pause::set(Some(crate::network::Pause::Switch));
    let w = World::new("approval");
    let ctx = egui::Context::default();
    let mut tab = recorded_tab(&w, &ctx);
    run(&w.me, &["remote", "set-url", "origin", &tab.remotes[0].url]);
    run(&w.me, &["remote", "add", "secondary", &tab.remotes[1].url]);
    tab.forks.waiting.insert(w.info().sha, vec![11, 12]);
    let calls = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let seen = calls.clone();
    let (arrived, first) = std::sync::mpsc::channel();
    let (gate, wait) = std::sync::mpsc::channel();
    RECORD_APPROVAL.with(|slot| {
        *slot.borrow_mut() = Some(Box::new(move |run| {
            seen.lock().unwrap_or_else(|e| e.into_inner()).push(run);
            arrived.send(()).map_err(|e| e.to_string())?;
            wait.recv().map_err(|e| e.to_string())?;
            Ok(())
        }))
    });
    tab.approve_runs(7, &ctx);
    tab.approve_runs(7, &ctx);
    assert!(
        first
            .recv_timeout(std::time::Duration::from_secs(5))
            .is_ok()
    );
    assert!(tab.busy_op.is_some());
    tab.set_primary_forge(Some("secondary".into()), &ctx);
    assert_eq!(
        tab.primary_forge_remote(),
        Some("origin"),
        "UI switching must wait for mutation"
    );
    // External config/catalog changes still invalidate the remaining captured work.
    tab.remotes[0].url = "https://forge.example:4000/origin/app".into();
    tab.forge_remotes[0].1.web = "https://forge.example:4000".into();
    tab.take_forge_selection(true, &ctx);
    let _ = gate.send(());
    let msg = tab.rx.recv_timeout(std::time::Duration::from_secs(5));
    assert!(msg.is_ok());
    if let Ok(msg) = msg {
        tab.handle(msg, &ctx);
    }
    assert_eq!(*calls.lock().unwrap_or_else(|e| e.into_inner()), [11]);
    assert!(tab.busy_op.is_none());
    assert!(!tab.forks.approving);
    assert!(tab.forks.scope.is_none());
    assert!(tab.forks.running.is_none());
}
#[test]
fn review14_queued_fork_actions_and_replaced_confirmations_cannot_run() {
    let _pause = crate::network::test_pause::set(Some(crate::network::Pause::Switch));
    let w = World::new("queued-actions");
    let ctx = egui::Context::default();
    let mut tab = recorded_tab(&w, &ctx);
    let old = tab.forge_generation;
    tab.set_primary_forge(Some("secondary".into()), &ctx);
    tab.forks.info.insert(7, w.info());
    for act in [
        super::ForkAct::Checkout(7, old),
        super::ForkAct::Rebase(7, old),
        super::ForkAct::Approve(7, old),
    ] {
        tab.fork_act(act, &ctx);
        assert!(tab.busy_op.is_none());
        assert!(tab.forks.scope.is_none());
        assert!(tab.forks.approve_armed.is_none());
    }
    let (_, gate) = record_plan(&w);
    tab.plan_fork_rebase(7, &ctx);
    let first = confirmation_id(&tab);
    let _ = gate.send(());
    drain_until(&mut tab, &ctx, |t| t.dialog.is_some());
    let (_, gate) = record_plan(&w);
    tab.plan_fork_rebase(7, &ctx);
    assert_ne!(first, confirmation_id(&tab));
    tab.fork_act(super::ForkAct::ConfirmRebase(first), &ctx);
    assert!(
        tab.busy_op.is_none(),
        "old confirmation must not execute a replacement plan"
    );
    let _ = gate.send(());
    drain_until(&mut tab, &ctx, |t| t.dialog.is_some());
    // Simulate Cancel; the next frame clears pending running and invalidates its token.
    tab.dialog = None;
    tab.forks_frame(&ctx);
    assert!(tab.forks.running.is_none());
    assert!(tab.forks.scope.is_none());
}
#[test]
fn review14_effective_destination_validation_uses_shared_canonical_identity() {
    use crate::forge::{Forge, Hosting};
    for forge in Forge::ALL {
        let h = Hosting {
            forge,
            host: "forge.example".into(),
            web: "https://forge.example:4000/proxy".into(),
            path: "Group/Project".into(),
        };
        for (url, valid) in [
            ("https://forge.example:4000/proxy/Group/Project.git", true),
            ("ssh://git@forge.example:2222/Group/Project.git", true),
            ("https://forge.example:3000/proxy/Group/Project.git", false),
            ("https://forge.example:4000/Group/Project.git", false),
            ("https://forge.example:4000/PROXY/Group/Project.git", false),
            (
                "https://forge.example:4000/proxy/group/project.git",
                !matches!(forge, Forge::GitLab | Forge::Azure | Forge::Bitbucket),
            ),
        ] {
            let d = super::guard::Destination {
                name: "selected".into(),
                fetch: vec![url.into()],
                push: vec![url.into()],
            };
            assert_eq!(
                d.validate(Some(&h), false).is_ok(),
                valid,
                "{forge:?} / {url}"
            );
            assert_eq!(
                d.validate(Some(&h), true).is_ok(),
                valid,
                "{forge:?} / {url}"
            );
        }
    }
}

#[test]
fn review14_confirmed_owned_rebase_executes_and_cleans_running()
-> Result<(), Box<dyn std::error::Error>> {
    let _pause = crate::network::test_pause::set(Some(crate::network::Pause::Metered));
    let w = World::new("confirm-execution");
    let ctx = egui::Context::default();
    let (p, mut tab) = refresh_stable_rebase(&w, &ctx)?;
    let id = confirmation_id(&tab);
    tab.fork_act(super::ForkAct::ConfirmRebase(id), &ctx);
    assert!(tab.busy_op.is_some());
    drain_until(&mut tab, &ctx, |t| {
        t.busy_op.is_none() && !t.loading && !t.timeline.entries.is_empty()
    });
    // Rebase is a local operation. Its undo is retained before the distinct push
    // checklist is approved; a completed rewrite alone must not publish anything.
    assert_eq!(git::resolve(&w.fork, "fix").unwrap_or_default(), p.seen);
    assert_ne!(git::head(&w.me), p.head);
    assert!(tab.timeline.entries.iter().any(|e| e.undo.is_some()));
    let approved = crate::repo::push_preflight::test_support::approve(&mut tab, &ctx);
    assert!(approved.is_ok(), "{approved:?}");
    drain_until(&mut tab, &ctx, |t| t.busy_op.is_none() && !t.loading);
    assert!(tab.forks.running.is_none());
    assert_eq!(
        git::resolve(&w.fork, "fix").unwrap_or_default(),
        git::head(&w.me)
    );
    assert_ne!(git::head(&w.me), p.head);
    assert!(tab.timeline.entries.iter().any(|e| e.undo.is_some()));
    Ok(())
}

#[test]
fn review14_checkout_rejects_other_instance_pushurl_before_fetch_or_config_changes() {
    let _pause = crate::network::test_pause::set(Some(crate::network::Pause::Metered));
    let w = World::new("checkout-pushurl");
    let ctx = egui::Context::default();
    let mut tab = recorded_tab(&w, &ctx);
    run(&w.me, &["remote", "set-url", "origin", &tab.remotes[0].url]);
    let url = "https://forge.example/ann/app.git";
    run(&w.me, &["remote", "add", "ann", url]);
    run(
        &w.me,
        &[
            "config",
            "remote.ann.pushurl",
            "https://forge.example:4000/ann/app.git",
        ],
    );
    tab.remotes = git::remotes(&w.me).unwrap_or_default();
    let mut info = w.info();
    info.clone_https = url.into();
    tab.forks.info.insert(7, info);
    let head = git::head(&w.me);
    let config = std::fs::read(w.me.join(".git/config")).unwrap_or_default();
    tab.checkout_fork(7, &ctx);
    drain_until(&mut tab, &ctx, |t| t.busy_op.is_none() && !t.loading);
    assert_eq!(git::head(&w.me), head);
    assert_eq!(
        std::fs::read(w.me.join(".git/config")).unwrap_or_default(),
        config
    );
    assert_eq!(run(&w.me, &["branch", "--show-current"]).trim(), "main");
    assert!(
        tab.status
            .as_ref()
            .is_some_and(|n| n.0.contains("matching push destination")),
        "{:?}",
        tab.status
    );
}

#[test]
fn review14_stale_conflict_and_auth_completion_cannot_offer_a_fork_push() {
    let _pause = crate::network::test_pause::set(Some(crate::network::Pause::Switch));
    for error in ["CONFLICT: could not apply", "fatal: Authentication failed"] {
        let w = World::new(if error.starts_with("CONFLICT") {
            "stale-conflict"
        } else {
            "stale-auth"
        });
        let p = rebase_ready(&w);
        let ctx = egui::Context::default();
        let mut tab = recorded_tab(&w, &ctx);
        assert!(tab.hosting.is_some());
        let Some(hosting) = tab.hosting.clone() else {
            return;
        };
        let scope = super::guard::Scope::new(tab.forge_generation, hosting, tab.remotes[0].clone());
        let label = "Rebase #7 onto main and push".to_owned();
        tab.forks.scope = Some(scope);
        tab.forks.scope_label = Some(label.clone());
        tab.forks.running = Some((label.clone(), p.clone()));
        tab.busy_op = Some(label.clone());
        tab.clear_forge_forks();
        tab.handle(
            crate::repo::Msg::OpDone {
                label,
                result: Err(error.into()),
                refresh: crate::repo::Refresh::All,
                kind: normal(p.undo_steps(), p.redo_steps()),
                quiet: false,
                stopped: 2,
            },
            &ctx,
        );
        drain_until(&mut tab, &ctx, |t| !t.loading);
        assert!(tab.busy_op.is_none());
        assert!(tab.forks.running.is_none());
        assert!(tab.forks.after_conflict.is_none());
        assert!(tab.last_op.is_none());
        assert!(!matches!(
            tab.dialog,
            Some(crate::repo::Dialog::Credentials { .. })
        ));
        assert_eq!(git::resolve(&w.fork, "fix").unwrap_or_default(), p.seen);
    }
}

#[test]
fn review14_other_tab_preference_aba_stops_worker_before_first_tab_polls() {
    let _pause = crate::network::test_pause::set(Some(crate::network::Pause::Switch));
    let w = World::new("cross-tab-worker");
    let p = rebase_ready(&w);
    let ctx = egui::Context::default();
    let first = recorded_tab(&w, &ctx);
    let mut second = recorded_tab(&w, &ctx);
    assert!(first.hosting.is_some());
    let Some(hosting) = first.hosting.clone() else {
        return;
    };
    let scope = super::guard::Scope::new(
        first.forge_generation,
        hosting,
        git::Remote {
            name: "origin".into(),
            url: arg(&w.upstream),
        },
    )
    .with_forge_lifetime(&ctx, first.repo_key());
    assert!(
        scope.check().is_ok(),
        "another tab reading the same preference must preserve the lifetime"
    );
    let generation = first.forge_generation;
    second.set_primary_forge(Some("secondary".into()), &ctx);
    second.set_primary_forge(Some("origin".into()), &ctx);
    assert_eq!(
        first.forge_generation, generation,
        "the running tab has not polled/rebound"
    );
    let before = git::head(&w.me);
    let result = crate::repo::operations::run_steps_guarded(
        &w.me,
        &p.steps(),
        OpKind::plain(),
        None,
        false,
        None,
        Some(&scope),
    )
    .0;
    assert!(
        result
            .as_ref()
            .is_err_and(|e| e.contains("primary forge changed")),
        "{result:?}"
    );
    assert_eq!(git::head(&w.me), before);
    assert_eq!(git::resolve(&w.fork, "fix").unwrap_or_default(), p.seen);
}
