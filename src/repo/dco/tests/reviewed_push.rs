// Reviewed push regression coverage.

#[test]
fn recording_local_signoff_queues_review_only_after_success_and_keeps_its_original_lease()
-> Result<(), String> {
    use crate::repo::timeline::{Awaiting, Ended};
    let plan = signoff::Plan {
        branch: "feat".into(),
        head: "a".repeat(40),
        base: "b".repeat(40),
        commits: Vec::new(),
        push: Some(signoff::PushTarget {
            remote: "selected".into(),
            urls: vec!["/private/recorded/selected.git".into()],
            remote_ref: "refs/heads/review-target".into(),
            tracking: Some("refs/remotes/selected/review-target".into()),
            seen: Some("c".repeat(40)),
        }),
    };
    let label = "Sign off the commits of feat";
    assert_eq!(super::fix_label(&plan), label);
    let Dialog::Confirm {
        title,
        body,
        button,
        ops,
        undo,
        ..
    } = super::fix_dialog(
        &plan,
        "Fixture <fixture@example.org>",
        "fixture@example.org",
    )
    else {
        return Err("No local sign-off confirmation".into());
    };
    assert_eq!(title, label);
    assert_eq!(button, "Sign off, then review push");
    assert!(body.contains("Review and approve that push separately"));
    assert!(body.contains("Ctrl+Z restores the local commits"));
    assert!(!crate::push_preflight::has_push(&ops));
    assert!(ops.contains(&crate::repo::operations::expect_ref_at(
        "refs/heads/feat",
        &plan.head
    )));
    assert!(matches!(undo, Undo::HeadMove { mode: "--keep" }));

    for result in [
        Ok(String::new()),
        Err("local rewrite failed".into()),
        Err(git::CANCELLED.into()),
        Err("Authentication failed in local hook".into()),
    ] {
        let ctx = egui::Context::default();
        let mut tab = RepoTab::new_deferred("/nonexistent/dco-completion-recording".into(), &ctx);
        tab.tx.cancel.cancel(); // no queued reader, command or process can execute
        tab.dco.running = Some(plan.clone());
        let stale_calls = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        if result.is_err() {
            let recorded = stale_calls.clone();
            tab.dco.review_after = Some(super::ReviewAfter {
                label: label.into(),
                heads: (plan.head.clone(), "d".repeat(40)),
                timeline_kept: true,
                rewrite_kept: false,
                then: Box::new(move |_, _| {
                    recorded.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                }),
            });
        }
        tab.timeline.awaiting = Awaiting::Ended {
            after_read: 0,
            then: Ended::New {
                label: label.into(),
                undo: Some(UndoEntry {
                    id: 0,
                    agent: None,
                    label: label.into(),
                    undo: Vec::new(),
                    redo: Vec::new(),
                    heads: Some((plan.head.clone(), "d".repeat(40))),
                }),
            },
        };
        tab.dco_op_done(label, &result, &ctx);
        assert_eq!(stale_calls.load(std::sync::atomic::Ordering::SeqCst), 0);
        assert!(tab.dco.running.is_none());
        assert!(tab.dialog.is_none() && tab.busy_op.is_none() && tab.running.is_none());
        assert!(tab.preflight_pending.is_none() && tab.last_op.is_none());
        if result.is_ok() {
            let pending = tab
                .dco
                .review_after
                .as_ref()
                .ok_or("Successful local rewrite lost its continuation")?;
            assert_eq!(pending.label, label);
            assert_eq!(pending.heads, (plan.head.clone(), "d".repeat(40)));
            assert_original_push(&plan)?;
            let Awaiting::Ended {
                then:
                    Ended::New {
                        label: raw,
                        undo: Some(undo),
                    },
                ..
            } = &tab.timeline.awaiting
            else {
                return Err("Local undo lost".into());
            };
            assert_eq!(raw, label);
            assert_eq!(undo.label, format!("{label} (here only)"));
            assert_eq!(undo.heads, Some((plan.head.clone(), "d".repeat(40))));
            let local_undo = undo.clone();
            let id = tab.timeline.push(crate::repo::timeline::Entry {
                id: 0,
                parent: None,
                time: 0,
                label: label.into(),
                origin: crate::repo::timeline::Origin::Gitcraft,
                before: Default::default(),
                after: Default::default(),
                files: Vec::new(),
                files_total: 0,
                undo: Some(local_undo),
                redo_child: None,
                receipt: None,
            });
            let mut unrelated_ids = Vec::new();
            for field in ["label", "before", "after"] {
                let mut other = tab.timeline.get(id).ok_or("Local entry missing")?.clone();
                match field {
                    "label" => other.label = "Sign off the commits of another".into(),
                    "before" => {
                        other
                            .undo
                            .as_mut()
                            .and_then(|undo| undo.heads.as_mut())
                            .ok_or("Local entry lost its undo heads")?
                            .0 = "e".repeat(40)
                    }
                    _ => {
                        other
                            .undo
                            .as_mut()
                            .and_then(|undo| undo.heads.as_mut())
                            .ok_or("Local entry lost its undo heads")?
                            .1 = "e".repeat(40)
                    }
                }
                unrelated_ids.push(tab.timeline.push(other));
            }
            retention_orders(&mut tab, &ctx, &plan, label, id, &unrelated_ids);
        } else {
            assert!(
                tab.dco.review_after.is_none() && tab.preflight_after.is_none(),
                "local failure/cancel must queue zero pushes"
            );
        }
    }
    Ok(())
}

fn assert_original_push(plan: &signoff::Plan) -> Result<(), String> {
    let prepared = super::completed_push(
        plan,
        "d".repeat(40),
        crate::git::repository_tools::guard::Snapshot::recording(),
    )
    .ok_or("Missing reviewed continuation")?;
    let route = &prepared.route;
    assert_eq!(route.remote, "selected");
    assert_eq!(route.destination, "refs/heads/review-target");
    assert_eq!(route.urls, ["/private/recorded/selected.git"]);
    assert_eq!(route.commit, "d".repeat(40));
    assert_eq!(route.lease, Some("c".repeat(40)));
    let ops = route.operations();
    assert!(crate::push_preflight::has_push(&ops));
    assert!(
        !ops.iter()
            .flatten()
            .any(|arg| arg == crate::push_preflight::APPROVED)
    );
    let mut preview = crate::push_preflight::selected_continuation_recording::plan(route);
    preview.retain_selection(prepared.clone())?;
    for changed in ["endpoint", "lease", "source"] {
        let mut changed_route = route.clone();
        match changed {
            "endpoint" => changed_route.urls = vec!["/private/recorded/changed.git".into()],
            "lease" => changed_route.lease = Some("e".repeat(40)),
            _ => changed_route.commit = "f".repeat(40),
        }
        let mut changed_preview =
            crate::push_preflight::selected_continuation_recording::plan(&changed_route);
        assert!(changed_preview.retain_selection(prepared.clone()).is_err());
    }
    Ok(())
}

fn retention_orders(
    tab: &mut RepoTab,
    ctx: &egui::Context,
    plan: &signoff::Plan,
    label: &str,
    id: u64,
    unrelated_ids: &[u64],
) {
    let heads = (plan.head.clone(), "d".repeat(40));
    let rewrite = git::Rewrite {
        branch: plan.branch.clone(),
        time: 0,
        old: git::Series::new(&plan.base, &heads.0),
        new: git::Series::new(&plan.base, &heads.1),
    };
    // These are the production completion entry points. Replacing only the
    // continuation records the point where snapshot/preflight could first start.
    let deliver_rewrite =
        |tab: &mut RepoTab,
         label: &str,
         heads: &(String, String),
         kept: Result<Option<(git::Rewrite, Vec<git::Rewrite>)>, String>| {
            tab.rewrite_kept(
                label,
                &format!("{label}: done"),
                Some(heads.clone()),
                kept,
                ctx,
            );
        };
    let unrelated = |tab: &mut RepoTab| {
        tab.dco_timeline_kept(id + 100, true, ctx);
        for other_id in unrelated_ids {
            tab.dco_timeline_kept(*other_id, true, ctx);
            tab.dco_timeline_kept(*other_id, false, ctx);
        }
        deliver_rewrite(
            tab,
            "Sign off the commits of another",
            &heads,
            Ok(Some((rewrite.clone(), Vec::new()))),
        );
        for other_heads in [
            ("e".repeat(40), heads.1.clone()),
            (heads.0.clone(), "e".repeat(40)),
        ] {
            // A different writer's success or failure cannot satisfy or cancel us.
            deliver_rewrite(
                tab,
                label,
                &other_heads,
                Ok(Some((rewrite.clone(), Vec::new()))),
            );
            deliver_rewrite(tab, label, &other_heads, Err(git::CANCELLED.into()));
        }
    };
    for (events, starts) in [
        // Timeline wins, with rewrite deliberately held beyond repeated timeline
        // and unrelated completions; the reverse order must behave identically.
        (
            vec!["timeline", "timeline", "rewrite", "rewrite", "timeline"],
            1,
        ),
        (
            vec!["rewrite", "rewrite", "timeline", "timeline", "rewrite"],
            1,
        ),
        (vec!["timeline-fail", "rewrite", "timeline", "rewrite"], 0),
        (vec!["rewrite-fail", "timeline", "rewrite", "timeline"], 0),
        (vec!["timeline", "rewrite-fail", "rewrite", "timeline"], 0),
        (vec!["rewrite", "timeline-fail", "timeline", "rewrite"], 0),
        (vec!["rewrite-cancel", "timeline", "rewrite"], 0),
        (vec!["timeline", "rewrite-cancel", "rewrite", "timeline"], 0),
        (vec!["rewrite-none", "timeline", "rewrite"], 0),
        (vec!["rewrite-wrong-proof", "timeline", "rewrite"], 0),
        (vec!["rewrite-wrong-branch", "timeline", "rewrite"], 0),
    ] {
        let entered = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let record = entered.clone();
        tab.dco.review_after = Some(super::ReviewAfter {
            label: label.into(),
            heads: heads.clone(),
            timeline_kept: false,
            rewrite_kept: false,
            then: Box::new(move |_, _| {
                record.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            }),
        });
        unrelated(tab);
        assert!(tab.dco.review_after.is_some());
        let (mut timeline_done, mut rewrite_done, mut failed) = (false, false, false);
        for event in &events {
            let (timeline, rewrite, failure) =
                retention_event(tab, ctx, label, id, &heads, &rewrite, event);
            timeline_done |= timeline;
            rewrite_done |= rewrite;
            failed |= failure;
            let expected = usize::from(timeline_done && rewrite_done && !failed);
            assert_eq!(
                entered.load(std::sync::atomic::Ordering::SeqCst),
                expected,
                "no early/duplicate snapshot start: {events:?} after {event}"
            );
            if expected == 0 && !failed {
                assert!(tab.dco.review_after.is_some(), "first completion must wait");
            } else {
                assert!(
                    tab.dco.review_after.is_none(),
                    "completion/failure is single-use"
                );
            }
            unrelated(tab);
            assert!(
                tab.dialog.is_none() && tab.busy_op.is_none() && tab.preflight_pending.is_none()
            );
            assert!(
                tab.timeline
                    .get(id)
                    .is_some_and(|entry| entry.undo.is_some())
            );
        }
        assert_eq!(entered.load(std::sync::atomic::Ordering::SeqCst), starts);
    }
}

fn retention_event(
    tab: &mut RepoTab,
    ctx: &egui::Context,
    label: &str,
    id: u64,
    heads: &(String, String),
    rewrite: &git::Rewrite,
    event: &str,
) -> (bool, bool, bool) {
    let (mut timeline_done, mut rewrite_done, mut failed) = (false, false, false);
    let deliver_rewrite = |tab: &mut RepoTab, label: &str, heads: &(String, String), kept| {
        tab.rewrite_kept(
            label,
            &format!("{label}: done"),
            Some(heads.clone()),
            kept,
            ctx,
        );
    };
    match event {
        "timeline" => {
            timeline_done = true;
            tab.dco_timeline_kept(id, true, ctx);
        }
        "timeline-fail" => {
            failed = true;
            tab.dco_timeline_kept(id, false, ctx);
        }
        "rewrite" => {
            rewrite_done = true;
            deliver_rewrite(tab, label, &heads, Ok(Some((rewrite.clone(), Vec::new()))));
        }
        "rewrite-fail" => {
            failed = true;
            deliver_rewrite(tab, label, &heads, Err("retention failed".into()));
        }
        "rewrite-cancel" => {
            failed = true;
            deliver_rewrite(tab, label, &heads, Err(git::CANCELLED.into()));
        }
        "rewrite-none" => {
            failed = true;
            deliver_rewrite(tab, label, &heads, Ok(None));
        }
        "rewrite-wrong-proof" | "rewrite-wrong-branch" => {
            failed = true;
            let mut wrong = rewrite.clone();
            if event == "rewrite-wrong-proof" {
                wrong.new.tip = "e".repeat(40);
            } else {
                wrong.branch = "another".into();
            }
            deliver_rewrite(tab, label, &heads, Ok(Some((wrong, Vec::new()))));
        }
        _ => unreachable!(),
    }
    (timeline_done, rewrite_done, failed)
}
