use super::*;
#[test]
fn selected_continuation_recording_retains_actual_route_receipt_and_expect_retry()
-> Result<(), String> {
    let route = git::repository_tools::routes::PushRoute {
        branch: "topic".into(),
        commit: "a".repeat(40),
        remote: "publish-host".into(),
        destination: "refs/heads/release".into(),
        urls: vec!["https://publish.invalid/team/repo".into()],
        lease: Some("b".repeat(40)),
    };
    let mut plan = crate::push_preflight::selected_continuation_recording::plan(&route);
    plan.retain_selection(git::repository_tools::routes::PreparedPush {
        snapshot: git::repository_tools::guard::Snapshot::recording(),
        route: route.clone(),
    })?;
    let original = crate::push_preflight::push_args(&plan)?;
    let binding: git::RouteBinding = serde_json::from_value(serde_json::json!({ "args":original, "routes":route.urls, "configuration":"recorded original route" })).map_err(|e| e.to_string())?;
    let control = git::OpControl::new(|| {});
    let receipt = crate::push_preflight::remember_auth_endpoint(&route.urls[0]);
    let failed = crate::repo::operations::run_reviewed_push(
        &original,
        |_| Ok(binding.clone()),
        || Err(receipt.clone()),
        Some(&control),
    );
    assert_eq!(failed, Err(receipt));
    let failure = control
        .auth_failure()
        .ok_or("typed reviewed receipt missing")?;
    assert_eq!(failure.binding, Some(binding.clone()));
    assert_eq!(failure.endpoints, route.urls);
    let mut ops = crate::push_preflight::approved_ops(plan);
    let guard = ops[0].clone();
    ops.insert(
        0,
        vec![
            crate::repo::operations::EXPECT_HTTPS_ROUTE.into(),
            serde_json::to_string(&binding).map_err(|e| e.to_string())?,
        ],
    );
    crate::repo::operations::retain_unfinished(&mut ops, 1);
    assert!(ops.contains(&guard));
    let mut checked = 0;
    crate::repo::operations::check_https_routes(&ops, |actual| {
        checked += 1;
        assert_eq!(actual, &binding);
        Ok(())
    })?;
    assert_eq!(checked, 1);
    let approved = ops.last().ok_or("APPROVED continuation missing")?;
    crate::repo::operations::reviewed_operands(approved)?;
    Ok(())
}
#[test]
fn recording_reviewed_route_is_captured_before_dispatch_and_proves_one_shot_receipt()
-> Result<(), String> {
    use std::cell::Cell;
    let binding = recorded_route()?;
    let original = binding.args.clone();
    let control = git::OpControl::new(|| {});
    let captured = Cell::new(false);
    let mut context = crate::connections::Context::default();
    context.choices.github.user = "initiating-account".into();
    let receipt = crate::push_preflight::remember_auth_endpoint("https://push.invalid/team/repo");
    let result = context.run(|| {
        crate::repo::operations::run_reviewed_push(
            &original,
            |args| {
                assert_eq!(
                    args,
                    original.iter().map(String::as_str).collect::<Vec<_>>()
                );
                assert!(!args.contains(&crate::push_preflight::APPROVED));
                assert!(!args.iter().any(|word| word.contains("private-alias")));
                captured.set(true);
                Ok(binding.clone())
            },
            || {
                assert!(captured.get());
                Err(receipt.clone())
            },
            Some(&control),
        )
    });
    assert_eq!(result, Err(receipt.clone())); // human diagnostics stay opaque
    let failure = control
        .auth_failure()
        .ok_or("missing authentication failure")?;
    assert_eq!(failure.context, context);
    assert!(failure.confirmed);
    assert_eq!(failure.endpoints, ["https://push.invalid/team/repo"]);
    assert_eq!(failure.binding, Some(binding.clone()));
    assert!(crate::push_preflight::take_auth_endpoint(&receipt).is_none());
    context.run(|| {
        let ctx = egui::Context::default();
        let mut tab = crate::repo::operations::RepoTab::new_deferred(
            "/nonexistent/reviewed-signin-recording".into(),
            &ctx,
        );
        tab.offer_failed_https_signin(failure.clone(), true, &ctx);
        let ticket = tab
            .https_challenge
            .as_ref()
            .ok_or("missing sign-in ticket")?;
        assert!(ticket.authorizes_endpoint("https://push.invalid/team/repo"));
        assert!(!ticket.authorizes_endpoint("https://fetch.invalid/team/repo"));
        assert_eq!(ticket.binding, Some(binding));
        assert!(matches!(
            tab.dialog,
            Some(crate::repo::operations::Dialog::Credentials { .. })
        ));
        Ok::<(), String>(())
    })?;
    let mut changed = context.clone();
    changed.choices.github.user = "later-account".into();
    changed.run(|| {
        let ctx = egui::Context::default();
        let mut tab = crate::repo::operations::RepoTab::new_deferred(
            "/nonexistent/stale-signin-recording".into(),
            &ctx,
        );
        tab.offer_failed_https_signin(failure, true, &ctx);
        assert!(tab.https_challenge.is_none());
        assert!(tab.dialog.is_none());
    });
    Ok(())
}

#[test]
fn recording_reviewed_auth_missing_foreign_or_unproven_routes_refuse_without_dispatch()
-> Result<(), String> {
    use std::cell::Cell;
    let binding = recorded_route()?;
    let original = binding.args.clone();
    let dispatched = Cell::new(false);
    let control = git::OpControl::new(|| {});
    let failed = crate::repo::operations::run_reviewed_push(
        &original,
        |_| Err("route proof unavailable".into()),
        || {
            dispatched.set(true);
            Ok(String::new())
        },
        Some(&control),
    );
    assert!(failed.is_err());
    assert!(!dispatched.get());
    assert!(control.auth_failure().is_none());
    for endpoint in [
        "https://fetch.invalid/team/repo",
        "https://push.invalid/other/repo",
        "https://push.invalid/team/repo/info/lfs",
        "http://push.invalid/team/repo",
    ] {
        let receipt = crate::push_preflight::remember_auth_endpoint(endpoint);
        let failed = crate::repo::operations::run_reviewed_push(
            &original,
            |_| Ok(binding.clone()),
            || Err(receipt.clone()),
            Some(&control),
        );
        assert_eq!(failed, Err(receipt.clone()));
        assert!(control.auth_failure().is_none(), "{endpoint}");
        assert!(crate::push_preflight::take_auth_endpoint(&receipt).is_none());
    }
    let receipt = crate::push_preflight::remember_auth_endpoint("https://push.invalid/team/repo");
    assert!(crate::push_preflight::take_auth_endpoint(&receipt).is_some());
    assert!(
        crate::repo::operations::reviewed_auth_failure(
            &receipt,
            binding,
            crate::connections::Context::default()
        )
        .is_none()
    );
    Ok(())
}

#[test]
fn recording_https_route_proof_is_rechecked_after_draft_and_retained_for_retry()
-> Result<(), String> {
    use std::cell::Cell;
    let binding = recorded_route()?;
    let ops = vec![
        vec![
            crate::repo::operations::EXPECT_HTTPS_ROUTE.into(),
            serde_json::to_string(&binding).map_err(|e| e.to_string())?,
        ],
        vec!["checkout".into(), "topic".into()],
    ];
    let current = Cell::new(true);
    let checks = Cell::new(0);
    let result = crate::repo::operations::with_rechecked_preparation(
        || {
            crate::repo::operations::check_https_routes(&ops, |proof| {
                assert_eq!(proof, &binding);
                checks.set(checks.get() + 1);
                if current.get() {
                    Ok(())
                } else {
                    Err("push route changed during draft".into())
                }
            })
        },
        || {
            current.set(false);
            Ok(())
        },
    );
    assert!(result.is_err());
    assert_eq!(checks.get(), 2);
    let ctx = egui::Context::default();
    let mut tab = crate::repo::operations::RepoTab::new_deferred(
        "/nonexistent/route-retry-recording".into(),
        &ctx,
    );
    tab.last_op = Some(crate::repo::operations::PendingOp {
        label: "Retry".into(),
        ops: ops.clone(),
        refresh: crate::repo::operations::Refresh::All,
        kind: OpKind::plain(),
        quiet: false,
    });
    tab.keep_unfinished(1);
    assert_eq!(
        tab.last_op.as_ref().ok_or("missing retry operation")?.ops,
        ops
    );
    let malformed = vec![vec![
        crate::repo::operations::EXPECT_HTTPS_ROUTE.into(),
        "invalid-json".into(),
    ]];
    let called = Cell::new(false);
    assert!(
        crate::repo::operations::check_https_routes(&malformed, |_| {
            called.set(true);
            Err("invalid proof dispatched".into())
        })
        .is_err()
    );
    assert!(!called.get(), "invalid proof dispatched");
    Ok(())
}

#[test]
fn recording_safety_draft_cannot_authorize_an_operation_after_its_context_moves() {
    use std::cell::{Cell, RefCell};
    let head = Cell::new("approved-head");
    let branch = Cell::new("approved-branch");
    let checks = RefCell::new(Vec::new());
    for change_branch in [false, true] {
        head.set("approved-head");
        branch.set("approved-branch");
        checks.borrow_mut().clear();
        let mut dispatched = false;
        let result = crate::repo::operations::with_rechecked_preparation(
            || {
                checks.borrow_mut().push((head.get(), branch.get()));
                if head.get() == "approved-head" && branch.get() == "approved-branch" {
                    Ok(())
                } else {
                    Err("original operation context moved".into())
                }
            },
            || {
                if change_branch {
                    branch.set("another-branch");
                } else {
                    head.set("later-coherent-draft-head");
                }
                Ok(()) // a coherent successful snapshot of the *new* state
            },
        );
        if result.is_ok() {
            dispatched = true;
        }
        assert!(result.is_err());
        assert!(!dispatched);
        assert_eq!(checks.borrow().len(), 2);
    }
}
