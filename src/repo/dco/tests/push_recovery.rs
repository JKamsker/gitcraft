// Push recovery regression coverage.

/// Pushed meanwhile and not fetched: the lease refuses the push, the remote keeps their commit,
/// and the rewrite done here is undoable all the same.
#[test]
fn a_refused_push_leaves_the_rewrite_undoable_here() -> Result<(), String> {
    let (dir, bare) = pushed_branch("lease");
    let ctx = egui::Context::default();
    let mut tab = open(&dir, &ctx);
    let head = git::head(&dir);
    tab.dco_act(
        super::DcoAct::PlanFix {
            base: "main".into(),
            head: head.clone(),
        },
        &ctx,
    );
    wait(&mut tab, &ctx, |tab| tab.dialog.is_some());
    // As the dialog's button does.
    confirm_local_signoff(&mut tab, &ctx);
    let signed = git::head(&dir);
    assert_ne!(signed, head);
    if let Some(Dialog::PushError(error)) = &tab.dialog {
        return Err(format!(
            "DCO fresh checklist refused after local rewrite: {error}"
        ));
    }
    assert!(matches!(
        &tab.dialog,
        Some(Dialog::PushPreflight {
            acknowledge: false,
            ..
        })
    ));
    // Cancel as the dialog does: there was no consent and no send. The signed local
    // history and its undo remain available; retry uses the original retained action.
    let selection = match &tab.dialog {
        Some(Dialog::PushPreflight { plan, .. }) => plan.selection().cloned(),
        _ => None,
    }
    .ok_or("Original bound selection missing")?;
    tab.dialog = None;
    settle(&mut tab, &ctx);
    assert_eq!(git::head(&dir), signed);
    assert_eq!(git::resolve(&bare, "refs/heads/feat")?, head);
    assert!(tab.busy_op.is_none() && tab.preflight_after.is_none());
    assert!(
        tab.last_op
            .as_ref()
            .is_some_and(|op| !crate::push_preflight::has_push(&op.ops))
    );
    let pending = tab
        .preflight_pending
        .take()
        .ok_or("Original DCO push action was lost")?;
    tab.preflight_selection = Some(selection);
    tab.run_ops_ex(
        &pending.label,
        pending.ops,
        pending.refresh,
        pending.kind,
        pending.quiet,
        &ctx,
    );
    let plan = crate::repo::push_preflight::test_support::review(&mut tab, &ctx)
        .map_err(|e| format!("DCO push review before dispatch: {e}"))?;
    assert_ne!(
        git::head(&dir),
        head,
        "review must describe the rewritten source"
    );
    assert_eq!(missing_on(&dir, "HEAD"), 0);
    assert_eq!(
        git::resolve(&bare, "refs/heads/feat").ok().as_deref(),
        Some(head.as_str())
    );
    assert_eq!(plan.head, git::head(&dir));
    assert_eq!(plan.base.as_deref(), Some(head.as_str()));
    // The receiver moves only after inspection; the actual server lease must refuse it.
    let (other, theirs) = push_as_someone_else(&bare, "lease-other");
    tab.apply(Action::SubmitPreflight(Box::new(plan)), &ctx);
    settle(&mut tab, &ctx);
    assert!(
        tab.activity
            .last()
            .is_some_and(|entry| matches!(&entry.outcome,
        crate::repo::activity::Outcome::Failed(error) if error.contains("stale info"))),
        "{:?}",
        tab.activity.last()
    );
    wait(&mut tab, &ctx, |tab| {
        !tab.timeline.entries.iter().all(|e| e.undo.is_none())
    });
    let remote = git::resolve(&bare, "refs/heads/feat").unwrap_or_default();
    let rewritten = git::head(&dir) != head;
    let undo_label = tab
        .timeline
        .current_entry()
        .and_then(|e| e.undo.as_ref())
        .map(|e| e.label.clone());
    tab.undo(&ctx);
    settle(&mut tab, &ctx);
    let back = git::head(&dir);
    assert_eq!(git::resolve(&bare, "refs/heads/feat")?, theirs);
    tab.redo(&ctx);
    settle(&mut tab, &ctx);
    assert_eq!(git::head(&dir), signed);
    assert_eq!(git::resolve(&bare, "refs/heads/feat")?, theirs);
    assert!(
        tab.dialog.is_none(),
        "local redo must not request a remote push"
    );
    remove(&[&dir, &bare, &other]);
    assert_eq!(remote, theirs);
    assert!(rewritten);
    assert!(
        undo_label.is_some_and(|l| l.contains("here only")),
        "the rewrite is undoable"
    );
    assert_eq!(back, head);
    Ok(())
}

fn confirm_local_signoff(tab: &mut RepoTab, ctx: &egui::Context) {
    if let Some(Dialog::Confirm {
        title, ops, undo, ..
    }) = tab.dialog.take()
    {
        assert_eq!(title, "Sign off the commits of feat");
        assert!(!crate::push_preflight::has_push(&ops));
        tab.apply(
            Action::UndoableOps {
                label: title,
                ops,
                refresh: crate::repo::Refresh::All,
                undo,
            },
            ctx,
        );
    }
    assert_eq!(tab.busy_op.as_deref(), Some("Sign off the commits of feat"));
    assert!(
        tab.dialog.is_none(),
        "local rewrite must finish before push review"
    );
    wait(tab, ctx, |tab| {
        tab.busy_op.is_none() && tab.dialog.is_some()
    });
    wait(tab, ctx, |tab| {
        !matches!(tab.dialog, Some(Dialog::PushLoading { .. }))
    });
}
