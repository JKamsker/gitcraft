#[test]
fn review19_new_remote_validates_both_rewrites_and_pins_the_initial_effective_urls()
-> Result<(), Box<dyn std::error::Error>> {
    use crate::forge::{Forge, Hosting};
    for (name, rule, destination, valid) in [
        ("push-valid", "pushInsteadOf", "git@github.com:ann/", true),
        ("fetch-wrong", "insteadOf", "git@github.com:mallory/", false),
        (
            "push-wrong",
            "pushInsteadOf",
            "git@github.com:mallory/",
            false,
        ),
    ] {
        let w = World::new(name);
        run(
            &w.me,
            &[
                "remote",
                "set-url",
                "origin",
                "https://github.com/acme/app.git",
            ],
        );
        run(
            &w.me,
            &[
                "config",
                &format!("url.{destination}.{rule}"),
                "https://github.com/ann/",
            ],
        );
        let h = Hosting {
            forge: Forge::GitHub,
            host: "github.com".into(),
            web: "https://github.com".into(),
            path: "acme/app".into(),
        };
        let primary = git::remotes(&w.me)?.remove(0);
        let mut fork = h.clone();
        fork.path = "ann/app".into();
        let scope = super::guard::Scope::new(1, h, primary).with_fork(
            "ann".into(),
            "https://github.com/ann/app.git".into(),
            fork,
            true,
        );
        let steps = vec![
            vec![
                "remote",
                "add",
                "--",
                "ann",
                "https://github.com/ann/app.git",
            ]
            .into_iter()
            .map(str::to_owned)
            .collect(),
            vec!["config", "remote.ann.gitcraftFork", "true"]
                .into_iter()
                .map(str::to_owned)
                .collect(),
        ];
        let result = crate::repo::operations::run_steps_guarded(
            &w.me,
            &steps,
            OpKind::plain(),
            None,
            false,
            None,
            Some(&scope),
        )
        .0;
        assert_eq!(result.is_ok(), valid, "{name}: {result:?}");
        let marker = git::run(Some(&w.me), &["config", "--get", "remote.ann.gitcraftFork"]);
        assert_eq!(marker.is_ok(), valid);
        if valid {
            assert!(scope.check_remote(&w.me).is_ok());
            // Same canonical fork, but a destination edit after its initial capture.
            run(
                &w.me,
                &[
                    "config",
                    "remote.ann.pushurl",
                    "ssh://git@github.com/ann/app.git",
                ],
            );
            assert!(scope.check_remote(&w.me).is_err());
            run(&w.me, &["config", "--unset", "remote.ann.pushurl"]);
            assert!(scope.check_remote(&w.me).is_ok());
            run(&w.me, &["remote", "remove", "ann"]);
            assert!(scope.check_remote(&w.me).is_err());
        }
    }
    Ok(())
}

#[test]
fn review19_recorded_rewrite_recovery_cannot_reset_a_later_external_commit()
-> Result<(), Box<dyn std::error::Error>> {
    let _pause = crate::network::test_pause::set(Some(crate::network::Pause::Metered));
    let w = World::new("owned-rewrite-later-commit");
    let p = rebase_ready(&w);
    let ctx = egui::Context::default();
    let mut tab = execution_tab(&w, &p, &ctx);
    let scope = tab.forks.scope.clone().ok_or("missing fork scope")?;
    let me = w.me.clone();
    BEFORE_STEP.with(|slot| {
        *slot.borrow_mut() = Some(Box::new(move |op| {
            if op.first().is_some_and(|s| s == "push") {
                commit(&me, "external-after-rebase.txt");
            }
        }))
    });
    let result = crate::repo::operations::run_steps_guarded(
        &w.me,
        &p.steps(),
        normal(p.undo_steps(), p.redo_steps()),
        None,
        false,
        None,
        Some(&scope),
    )
    .0;
    BEFORE_STEP.with(|slot| *slot.borrow_mut() = None);
    assert!(result.is_err(), "{result:?}");
    let external = git::head(&w.me);
    let owned = scope.rewrite().ok_or("missing worker rewrite record")?;
    assert_ne!(owned.tip, p.head);
    assert_ne!(owned.tip, external);
    assert_eq!(git::resolve(&w.fork, "fix")?, p.seen);
    tab.forks_op_done("Rebase #7 onto main and push", &result, &ctx);
    drain_until(&mut tab, &ctx, |t| {
        t.timeline.entries.iter().any(|e| {
            e.undo
                .as_ref()
                .is_some_and(|u| u.label.ends_with("(here only)"))
        })
    });
    let entry = tab
        .timeline
        .entries
        .iter()
        .find_map(|e| e.undo.clone().filter(|u| u.label.ends_with("(here only)")))
        .ok_or("missing local recovery entry")?;
    let undo = run_steps(&w.me, &entry.undo.clone(), OpKind::Undo(entry), None).0;
    assert!(undo.is_err(), "{undo:?}");
    assert_eq!(git::head(&w.me), external);
    Ok(())
}

fn persisted(entry: UndoEntry) -> Result<UndoEntry, serde_json::Error> {
    serde_json::from_str(&serde_json::to_string(&entry)?)
}
fn advanced_fork(w: &World, file: &str) -> String {
    run(&w.ann, &["fetch", "-q", "fork"]);
    run(&w.ann, &["checkout", "-q", "-B", "fix", "fork/fix"]);
    commit(&w.ann, file);
    run(&w.ann, &["push", "-q", "fork", "fix"]);
    run(&w.me, &["fetch", "-q", "ann"]);
    git::head(&w.ann)
}
#[test]
fn review19_persisted_redo_lease_rejects_a_fetched_contributor_advance()
-> Result<(), Box<dyn std::error::Error>> {
    let w = World::new("redo-explicit-lease");
    let p = rebase_ready(&w);
    let (result, kind) = run_steps(
        &w.me,
        &p.steps(),
        normal(p.undo_steps(), p.redo_steps()),
        None,
    );
    assert!(result.is_ok(), "{result:?}");
    let entry = persisted(custom(kind).ok_or("missing undo entry")?)?;
    let (result, kind) = run_steps(&w.me, &entry.undo.clone(), OpKind::Undo(entry), None);
    assert!(result.is_ok(), "{result:?}");
    let OpKind::Undo(entry) = kind else {
        return Err("expected undo entry".into());
    };
    let entry = persisted(entry)?;
    assert!(entry.redo.iter().any(|op| {
        op.iter()
            .any(|arg| arg == &format!("--force-with-lease=refs/heads/fix:{}", p.head))
    }));
    run(
        &w.me,
        &[
            "config",
            "remote.ann.fetch",
            "+refs/heads/*:refs/remotes/ann-new/*",
        ],
    );
    let theirs = advanced_fork(&w, "contributor-before-redo.txt");
    let result = run_steps(&w.me, &entry.redo.clone(), OpKind::Redo(entry), None).0;
    assert!(result.is_err(), "{result:?}");
    assert_eq!(git::resolve(&w.fork, "fix")?, theirs);
    Ok(())
}

#[test]
fn review19_persisted_undo_redo_roundtrips_rebind_the_exact_pushed_tip()
-> Result<(), Box<dyn std::error::Error>> {
    let w = World::new("repeat-undo-redo");
    let p = rebase_ready(&w);
    let mut steps = p.steps();
    steps
        .iter_mut()
        .filter(|op| op.first().is_some_and(|s| s == "push"))
        .for_each(|op| op.insert(1, "--no-verify".into()));
    let (result, kind) = run_steps(&w.me, &steps, normal(p.undo_steps(), p.redo_steps()), None);
    assert!(result.is_ok(), "{result:?}");
    let mut entry = persisted(custom(kind).ok_or("missing undo entry")?)?;
    for _ in 0..2 {
        let tip = git::head(&w.me);
        assert!(entry.undo.iter().any(|op| {
            op.iter()
                .any(|arg| arg == &format!("--force-with-lease=refs/heads/fix:{tip}"))
        }));
        let (result, kind) = run_steps(&w.me, &entry.undo.clone(), OpKind::Undo(entry), None);
        assert!(result.is_ok(), "{result:?}");
        assert_eq!(git::head(&w.me), p.head);
        assert_eq!(git::resolve(&w.fork, "fix")?, p.head);
        let OpKind::Undo(undone) = kind else {
            return Err("expected undo entry".into());
        };
        entry = persisted(undone)?;
        let (result, kind) = run_steps(&w.me, &entry.redo.clone(), OpKind::Redo(entry), None);
        assert!(result.is_ok(), "{result:?}");
        let OpKind::Redo(redone) = kind else {
            return Err("expected redo entry".into());
        };
        entry = persisted(redone)?;
        assert_ne!(git::head(&w.me), p.head);
        assert_eq!(git::resolve(&w.fork, "fix")?, git::head(&w.me));
    }
    Ok(())
}

fn edit_base(repo: &Path, text: &str) -> Result<(), std::io::Error> {
    std::fs::write(repo.join("base.txt"), text)?;
    run(repo, &["add", "base.txt"]);
    run(repo, &["commit", "-q", "-m", text]);
    Ok(())
}
#[test]
fn review19_conflict_continuation_pins_its_tip_and_original_explicit_lease()
-> Result<(), Box<dyn std::error::Error>> {
    let _pause = crate::network::test_pause::set(Some(crate::network::Pause::Metered));
    for mode in ["push", "local-moved", "contributor-moved"] {
        let w = World::new(&format!("continued-{mode}"));
        edit_base(&w.ann, "contributor version")?;
        run(&w.ann, &["push", "-q", "fork", "fix"]);
        run(&w.ann, &["checkout", "-q", "-B", "up", "origin/main"]);
        edit_base(&w.ann, "upstream version")?;
        run(&w.ann, &["push", "-q", "origin", "up:main"]);
        let ctx = egui::Context::default();
        let (p, mut tab) = refresh_stable_rebase(&w, &ctx)?;
        let scope = tab.forks.scope.clone().ok_or("missing fork scope")?;
        let result = crate::repo::operations::run_steps_guarded(
            &w.me,
            &p.steps(),
            normal(p.undo_steps(), p.redo_steps()),
            None,
            false,
            None,
            Some(&scope),
        )
        .0;
        assert!(
            result.as_ref().is_err_and(|e| e.contains("CONFLICT")),
            "{result:?}"
        );
        assert!(
            scope
                .rewrite()
                .ok_or("missing worker rewrite record")?
                .conflicts
        );
        tab.forks_op_done("Rebase #7 onto main and push", &result, &ctx);
        assert!(tab.forks.after_conflict.is_some());
        std::fs::write(w.me.join("base.txt"), "resolved version")?;
        run(&w.me, &["add", "base.txt"]);
        run(&w.me, &["-c", "core.editor=true", "rebase", "--continue"]);
        assert_eq!(git::in_progress(&w.me), Ok(None));
        let rebased = git::head(&w.me);
        tab.refs = git::refs(&w.me)?;
        tab.offer_push_after_conflict();
        assert!(tab.dialog.is_some());
        assert_eq!(tab.forks.push_tip.as_deref(), Some(rebased.as_str()));
        let expected_fork = match mode {
            "local-moved" => {
                commit(&w.me, "after-continuation.txt");
                p.seen.clone()
            }
            "contributor-moved" => {
                run(
                    &w.me,
                    &[
                        "config",
                        "remote.ann.fetch",
                        "+refs/heads/*:refs/remotes/ann-new/*",
                    ],
                );
                advanced_fork(&w, "after-conflict-contributor.txt")
            }
            _ => rebased.clone(),
        };
        let expected_local = git::head(&w.me);
        tab.dialog = None;
        tab.fork_act(super::ForkAct::ConfirmPush(scope.id), &ctx);
        // The conflict confirmation leads to the same deliberate push checklist.
        // A stale contributor lease may refuse during inspection; a local tip
        // change must still refuse if its checklist reaches actual dispatch.
        assert_eq!(
            git::resolve(&w.fork, "fix")?,
            if mode == "contributor-moved" {
                expected_fork.clone()
            } else {
                p.seen.clone()
            }
        );
        let approved = crate::repo::push_preflight::test_support::approve(&mut tab, &ctx);
        if mode == "push" {
            assert!(approved.is_ok(), "{approved:?}");
        } else if approved.is_err() {
            assert!(
                matches!(
                    tab.dialog,
                    Some(
                        crate::repo::Dialog::PushError(_)
                            | crate::repo::Dialog::PushPreflight { .. }
                    )
                ),
                "{mode}: {approved:?}; status: {:?}",
                tab.status
            );
            // A declined checklist has no OpDone. Dismissing it and processing
            // the next ordinary frame retires the pending fork action.
            tab.dialog = None;
            tab.forks_frame(&ctx);
        }
        drain_until(&mut tab, &ctx, |t| t.busy_op.is_none() && !t.loading);
        assert_eq!(git::head(&w.me), expected_local);
        assert_eq!(git::resolve(&w.fork, "fix")?, expected_fork);
        assert!(tab.forks.running.is_none(), "{mode}: {approved:?}");
        if mode != "push" && approved.is_ok() {
            assert!(
                tab.status.as_ref().is_some_and(|notice| notice.1),
                "{mode}: the dispatched stale-tip guard must report failure"
            );
        }
        if mode == "push" {
            assert!(
                tab.timeline
                    .entries
                    .iter()
                    .filter_map(|e| e.undo.as_ref())
                    .any(|entry| entry.undo.iter().any(|op| {
                        op.iter().any(|arg| {
                            arg == &format!("--force-with-lease=refs/heads/fix:{rebased}")
                        })
                    }))
            );
        }
    }
    Ok(())
}
