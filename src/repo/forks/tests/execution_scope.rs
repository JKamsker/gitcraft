type StepRecording = Box<dyn FnMut(&[String])>;
thread_local! {
    static BEFORE_STEP: std::cell::RefCell<Option<StepRecording>> = const { std::cell::RefCell::new(None) };
}
pub(super) fn before_step(op: &[String]) {
    BEFORE_STEP.with(|slot| {
        if let Some(record) = slot.borrow_mut().as_mut() {
            record(op);
        }
    });
}
fn rebase_ready(w: &World) -> plan::Rebase {
    run(&w.ann, &["checkout", "-q", "-B", "up", "origin/main"]);
    commit(&w.ann, "upstream.txt");
    run(&w.ann, &["push", "-q", "origin", "up:main"]);
    let steps = w
        .checkout(&["main".into()])
        .map(|p| p.steps)
        .unwrap_or_default();
    assert!(run_steps(&w.me, &steps, OpKind::plain(), None).0.is_ok());
    let p = plan::plan_rebase(
        &w.me,
        &w.info(),
        &git::Remote {
            name: "origin".into(),
            url: arg(&w.upstream),
        },
        &w.setup(),
        None,
        None,
        None,
    );
    assert!(p.is_ok(), "{p:?}");
    p.unwrap_or_else(|_| synthetic_plan(w))
}
fn confirmation_id(tab: &crate::repo::RepoTab) -> u64 {
    tab.forks.scope.as_ref().map_or(0, |s| s.id)
}
#[test]
fn review14_divergent_primary_base_reaches_only_the_owned_fork() {
    let w = World::new("divergent");
    let secondary = w.root.join("secondary.git");
    run(
        &w.root,
        &["clone", "--bare", "-q", &arg(&w.upstream), &arg(&secondary)],
    );
    // Origin and the selected repository advance independently from the fork point.
    run(&w.ann, &["checkout", "-q", "main"]);
    commit(&w.ann, "origin-only.txt");
    run(&w.ann, &["push", "-q", "origin", "main"]);
    run(&w.ann, &["checkout", "-q", "-B", "selected", "HEAD~1"]);
    commit(&w.ann, "selected-only.txt");
    run(&w.ann, &["push", "-q", &arg(&secondary), "HEAD:main"]);
    run(&w.me, &["remote", "add", "secondary", &arg(&secondary)]);
    let steps = w
        .checkout(&["main".into()])
        .map(|p| p.steps)
        .unwrap_or_default();
    assert!(run_steps(&w.me, &steps, OpKind::plain(), None).0.is_ok());
    let primary = git::Remote {
        name: "secondary".into(),
        url: arg(&secondary),
    };
    let p = plan::plan_rebase(&w.me, &w.info(), &primary, &w.setup(), None, None, None);
    assert!(p.is_ok(), "{p:?}");
    let Ok(p) = p else { return };
    let origin = git::head(&w.upstream);
    let config = std::fs::read(w.me.join(".git/config")).unwrap_or_default();
    assert_eq!(p.onto, "refs/remotes/secondary/main");
    assert!(
        run_steps(
            &w.me,
            &p.steps(),
            normal(p.undo_steps(), p.redo_steps()),
            None
        )
        .0
        .is_ok()
    );
    assert!(w.me.join("selected-only.txt").exists());
    assert!(!w.me.join("origin-only.txt").exists());
    assert_eq!(
        git::head(&w.me),
        git::resolve(&w.fork, "refs/heads/fix").unwrap_or_default()
    );
    assert_eq!(git::head(&w.upstream), origin);
    assert_eq!(
        std::fs::read(w.me.join(".git/config")).unwrap_or_default(),
        config
    );
}
#[test]
fn review14_pending_and_confirmed_plans_expire_on_rename_url_edit_removal_and_aba() {
    let _pause = crate::network::test_pause::set(Some(crate::network::Pause::Switch));
    for pending in [true, false] {
        for change in ["rename", "url", "remove", "aba", "unrelated"] {
            let w = World::new(&format!("expire-{pending}-{change}"));
            let ctx = egui::Context::default();
            let mut tab = recorded_tab(&w, &ctx);
            let (_, gate) = record_plan(&w);
            tab.plan_fork_rebase(7, &ctx);
            let id = confirmation_id(&tab);
            if !pending {
                let _ = gate.send(());
                drain_until(&mut tab, &ctx, |t| t.dialog.is_some());
            }
            match change {
                "rename" => {
                    tab.remotes[0].name = "renamed".into();
                    tab.forge_remotes[0].0 = "renamed".into();
                    tab.take_forge_selection(true, &ctx);
                }
                "url" => {
                    tab.remotes[0].url = "https://forge.example:4000/origin/app".into();
                    tab.forge_remotes[0].1.web = "https://forge.example:4000".into();
                    tab.take_forge_selection(true, &ctx);
                }
                "remove" => {
                    tab.remotes.remove(0);
                    tab.forge_remotes.remove(0);
                    tab.take_forge_selection(true, &ctx);
                }
                "aba" => {
                    tab.set_primary_forge(Some("secondary".into()), &ctx);
                    tab.set_primary_forge(Some("origin".into()), &ctx);
                }
                _ => {
                    tab.remotes.push(git::Remote {
                        name: "unrelated".into(),
                        url: "/owned/local".into(),
                    });
                    tab.take_forge_selection(true, &ctx);
                }
            }
            if pending {
                let _ = gate.send(());
                let msg = tab.rx.recv_timeout(std::time::Duration::from_secs(5));
                assert!(msg.is_ok(), "planner did not deliver");
                if let Ok(msg) = msg {
                    tab.handle(msg, &ctx);
                }
            }
            assert!(tab.dialog.is_none(), "{pending}/{change}");
            assert!(tab.forks.running.is_none(), "{pending}/{change}");
            tab.fork_act(super::ForkAct::ConfirmRebase(id), &ctx);
            assert!(
                tab.busy_op.is_none(),
                "queued confirmation survived {change}"
            );
            assert_eq!(git::head(&w.me), git::head(&w.upstream));
        }
    }
}
#[test]
fn review14_disk_remote_changes_refuse_before_any_rewrite_or_push() {
    for change in ["base-url", "fork-url", "pushurl", "remove", "rename"] {
        let w = World::new(&format!("disk-{change}"));
        let p = rebase_ready(&w);
        let before = git::head(&w.me);
        let fork_before = git::resolve(&w.fork, "fix").unwrap_or_default();
        match change {
            "base-url" => {
                run(&w.me, &["remote", "set-url", "origin", &arg(&w.fork)]);
            }
            "fork-url" => {
                run(&w.me, &["remote", "set-url", "ann", &arg(&w.upstream)]);
            }
            "pushurl" => {
                run(&w.me, &["config", "remote.ann.pushurl", &arg(&w.upstream)]);
            }
            "remove" => {
                run(&w.me, &["remote", "remove", "origin"]);
            }
            _ => {
                run(&w.me, &["remote", "rename", "origin", "renamed"]);
            }
        }
        let result = run_steps(&w.me, &p.steps(), OpKind::plain(), None).0;
        assert!(result.is_err(), "{change}: {result:?}");
        assert_eq!(git::head(&w.me), before, "{change}");
        assert_eq!(
            git::resolve(&w.fork, "fix").unwrap_or_default(),
            fork_before,
            "{change}"
        );
    }
}
#[test]
fn review14_running_rebase_stops_before_push_and_records_local_undo() {
    let _pause = crate::network::test_pause::set(Some(crate::network::Pause::Switch));
    let w = World::new("running-switch");
    let p = rebase_ready(&w);
    let ctx = egui::Context::default();
    let mut tab = recorded_tab(&w, &ctx);
    let scope = super::guard::Scope::new(
        tab.forge_generation,
        tab.hosting.clone().unwrap_or(crate::forge::Hosting {
            forge: crate::forge::Forge::Forgejo,
            host: "forge.example".into(),
            web: "https://forge.example".into(),
            path: "origin/app".into(),
        }),
        git::Remote {
            name: "origin".into(),
            url: arg(&w.upstream),
        },
    );
    let invalidated = scope.clone();
    BEFORE_STEP.with(|slot| {
        *slot.borrow_mut() = Some(Box::new(move |op| {
            if op.first().is_some_and(|s| s == "push") {
                invalidated.invalidate();
            }
        }))
    });
    let (result, kind, _) = crate::repo::operations::run_steps_guarded(
        &w.me,
        &p.steps(),
        normal(p.undo_steps(), p.redo_steps()),
        None,
        false,
        None,
        Some(&scope),
    );
    BEFORE_STEP.with(|slot| *slot.borrow_mut() = None);
    assert!(
        result
            .as_ref()
            .is_err_and(|e| e.contains("primary forge changed")),
        "{result:?}"
    );
    assert_ne!(git::head(&w.me), p.head);
    assert_eq!(git::resolve(&w.fork, "fix").unwrap_or_default(), p.seen);
    let label = "Rebase #7 onto main and push".to_owned();
    tab.busy_op = Some(label.clone());
    tab.forks.scope = Some(scope);
    tab.forks.scope_label = Some(label.clone());
    tab.forks.running = Some((label.clone(), p.clone()));
    tab.clear_forge_forks();
    assert!(
        tab.forks.running.is_some(),
        "retain active execution for completion cleanup"
    );
    tab.handle(
        crate::repo::Msg::OpDone {
            label,
            result,
            kind,
            refresh: crate::repo::Refresh::All,
            quiet: false,
            stopped: 3,
        },
        &ctx,
    );
    drain_until(&mut tab, &ctx, |t| {
        t.timeline
            .entries
            .iter()
            .any(|e| e.label.ends_with("(here only)"))
    });
    assert!(tab.busy_op.is_none());
    assert!(tab.forks.running.is_none());
    assert!(tab.forks.after_conflict.is_none());
    assert!(tab.last_op.is_none());
    let (undo, _) = p.local_undo(&git::head(&w.me));
    assert!(run_steps(&w.me, &undo, OpKind::plain(), None).0.is_ok());
    assert_eq!(git::head(&w.me), p.head);
}
#[test]
fn review14_disk_edit_between_rebase_and_push_stops_push() {
    let w = World::new("running-edit");
    let p = rebase_ready(&w);
    let repo = w.me.clone();
    let other = arg(&w.upstream);
    BEFORE_STEP.with(|slot| {
        *slot.borrow_mut() = Some(Box::new(move |op| {
            if op.first().is_some_and(|s| s == "push") {
                run(&repo, &["config", "remote.ann.pushurl", &other]);
            }
        }))
    });
    let result = run_steps(&w.me, &p.steps(), OpKind::plain(), None).0;
    BEFORE_STEP.with(|slot| *slot.borrow_mut() = None);
    assert!(result.is_err(), "{result:?}");
    assert_ne!(git::head(&w.me), p.head);
    assert_eq!(git::resolve(&w.fork, "fix").unwrap_or_default(), p.seen);
    assert!(git::resolve(&w.upstream, "fix").is_err());
}
