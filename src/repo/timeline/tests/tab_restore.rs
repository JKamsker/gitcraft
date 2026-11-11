/// Staging right after a commit, before its read is in, doesn't leave undo waiting for good.
#[test]
fn staging_after_an_operation_does_not_hold_undo_back() {
    use crate::repo::Refresh;
    let dir = repo("stage-after");
    let ctx = egui::Context::default();
    let mut tab = open(&dir, &ctx);
    let _ = std::fs::write(dir.join("a.txt"), "a");
    tab.commit_summary = "first".to_owned();
    tab.apply(Action::Commit, &ctx);
    assert!(wait(&mut tab, &ctx, |t| t.busy_op.is_none()));
    let _ = std::fs::write(dir.join("b.txt"), "b");
    tab.apply(
        Action::Ops {
            label: "Stage".to_owned(),
            ops: crate::repo::ops(&[&["add", "b.txt"]]),
            refresh: Refresh::Status,
        },
        &ctx,
    );
    let settles = wait(&mut tab, &ctx, settled);
    let _ = std::fs::remove_dir_all(&dir);
    assert!(settles, "{:?}", tab.timeline.awaiting);
    assert_eq!(tab.timeline.entries.len(), 1);
}

#[test]
fn review22_restore_plan_can_checkout_feat_then_reset_to_f1()
-> Result<(), Box<dyn std::error::Error>> {
    let dir = repo("review22-checkout-reset");
    let main = git::head(&dir);
    git_ok(&dir, &["checkout", "-q", "-b", "feat"]);
    git_ok(&dir, &["commit", "-q", "--allow-empty", "-m", "F1"]);
    let target = super::RefState::of(&git::refs(&dir)?);
    let f1 = git::head(&dir);
    git_ok(&dir, &["commit", "-q", "--allow-empty", "-m", "F2"]);
    let f2 = git::head(&dir);
    git_ok(&dir, &["checkout", "-q", "main"]);
    let now = super::RefState::of(&git::refs(&dir)?);
    let steps = snapshot::restore_steps(&now, &target, &[])?;
    assert!(steps.iter().any(|s| s == &["checkout", "feat", "--"]));
    assert!(steps.iter().any(|s| s == &["reset", "--keep", f1.as_str()]));
    assert_eq!(git::resolve(&dir, "feat")?, f2);
    let result =
        crate::repo::operations::run_steps(&dir, &steps, crate::repo::OpKind::plain(), None).0;
    assert!(result.is_ok(), "actual timeline restore failed: {result:?}");
    assert_eq!(git_ok(&dir, &["branch", "--show-current"]), "feat");
    assert_eq!(git::head(&dir), f1);
    assert_eq!(git::resolve(&dir, "main")?, main);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}
