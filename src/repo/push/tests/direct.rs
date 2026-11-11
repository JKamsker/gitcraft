fn direct_tab(w: &World, ctx: &egui::Context) -> Result<crate::repo::RepoTab, String> {
    let mut tab = crate::repo::RepoTab::new_deferred(w.mine.clone(), ctx);
    tab.start_deferred(ctx);
    settle(&mut tab, ctx)?;
    Ok(tab)
}

fn settle(tab: &mut crate::repo::RepoTab, ctx: &egui::Context) -> Result<(), String> {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    loop {
        tab.poll(ctx);
        if !tab.opening() && !tab.loading && tab.busy_op.is_none() { return Ok(()); }
        if std::time::Instant::now() >= deadline { return Err(format!("Push did not settle: {:?}", tab.status)); }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}

#[test]
fn toolbar_push_sends_even_heuristic_findings_without_a_checklist() -> Result<(), String> {
    let w = world("direct-findings")?;
    let head = commit(&w.mine, "WIP: ordinary push")?;
    let ctx = egui::Context::default();
    let mut tab = direct_tab(&w, &ctx)?;
    tab.apply(crate::repo::Action::Push, &ctx);
    assert!(tab.dialog.is_none());
    assert!(tab.busy_op.is_some());
    settle(&mut tab, &ctx)?;
    assert!(tab.dialog.is_none());
    assert_eq!(remote_main(&w), head);
    Ok(())
}

#[test]
fn toolbar_refusal_preserves_remote_and_offers_explicit_force_review() -> Result<(), String> {
    let w = world("direct-refusal")?;
    commit(&w.mine, "mine")?;
    let theirs = commit(&w.theirs, "theirs")?;
    run(&w.theirs, &["push", "origin", "main"])?;
    let ctx = egui::Context::default();
    let mut tab = direct_tab(&w, &ctx)?;
    tab.apply(crate::repo::Action::Push, &ctx);
    assert!(tab.dialog.is_none());
    settle(&mut tab, &ctx)?;
    assert_eq!(remote_main(&w), theirs);
    assert!(matches!(tab.push_flow.banner, Some(super::Banner::Rejected(_))));
    tab.apply(crate::repo::Action::PushFlow(super::PushAct::ConfirmForce), &ctx);
    assert!(matches!(tab.dialog, Some(crate::repo::Dialog::Confirm { .. })));
    assert_eq!(remote_main(&w), theirs);
    Ok(())
}

#[test]
fn toolbar_publish_starts_directly_with_one_remote_and_asks_with_two() -> Result<(), String> {
    let w = world("direct-publish")?;
    run(&w.mine, &["checkout", "-b", "topic"])?;
    let head = commit(&w.mine, "topic")?;
    let ctx = egui::Context::default();
    let mut tab = direct_tab(&w, &ctx)?;
    tab.apply(crate::repo::Action::Push, &ctx);
    assert!(tab.dialog.is_none());
    settle(&mut tab, &ctx)?;
    assert_eq!(git::resolve(&w.bare, "refs/heads/topic")?, head);
    assert_eq!(run(&w.mine, &["rev-parse", "--abbrev-ref", "@{upstream}"])?.trim(), "origin/topic");
    run(&w.mine, &["checkout", "-b", "ambiguous"])?;
    run(&w.mine, &["remote", "add", "second", &path(&w.bare)])?;
    let mut tab = direct_tab(&w, &ctx)?;
    tab.apply(crate::repo::Action::Push, &ctx);
    assert!(matches!(tab.push_flow.banner, Some(super::Banner::NoUpstream { .. })));
    assert!(git::resolve(&w.bare, "refs/heads/ambiguous").is_err());
    Ok(())
}

#[test]
fn toolbar_never_silently_follows_a_configured_force_refspec() -> Result<(), String> {
    let w = world("direct-force-config")?;
    let before = remote_main(&w);
    commit(&w.mine, "mine")?;
    run(&w.mine, &["config", "remote.origin.push", "+refs/heads/main:refs/heads/main"])?;
    let ctx = egui::Context::default();
    let mut tab = direct_tab(&w, &ctx)?;
    tab.apply(crate::repo::Action::Push, &ctx);
    settle(&mut tab, &ctx)?;
    assert_eq!(remote_main(&w), before);
    assert!(tab.status.as_deref().is_some_and(|(message, error)| *error && message.contains("force-with-lease")));
    Ok(())
}

#[test]
fn toolbar_honors_push_remote_and_keeps_gone_upstream_target() -> Result<(), String> {
    let w = world("direct-routing")?;
    let other = w.root.join("other.git");
    run(&w.root, &["clone", "--bare", &path(&w.bare), &path(&other)])?;
    run(&w.mine, &["remote", "add", "publish", &path(&other)])?;
    run(&w.mine, &["config", "branch.main.pushRemote", "publish"])?;
    let before = remote_main(&w);
    let head = commit(&w.mine, "routed")?;
    let ctx = egui::Context::default();
    let mut tab = direct_tab(&w, &ctx)?;
    tab.apply(crate::repo::Action::Push, &ctx);
    settle(&mut tab, &ctx)?;
    assert_eq!(git::resolve(&other, "refs/heads/main")?, head);
    assert_eq!(remote_main(&w), before);
    // A gone upstream must still use Git's configured route, rather than offer a
    // new remote or silently publish somewhere else.
    run(&w.mine, &["config", "--unset", "branch.main.pushRemote"])?;
    run(&w.mine, &["update-ref", "-d", "refs/remotes/origin/main"])?;
    run(&w.bare, &["update-ref", "-d", "refs/heads/main"])?;
    let mut tab = direct_tab(&w, &ctx)?;
    tab.apply(crate::repo::Action::Push, &ctx);
    settle(&mut tab, &ctx)?;
    assert!(tab.push_flow.banner.is_none());
    assert_eq!(remote_main(&w), head);
    Ok(())
}

#[test]
fn toolbar_honors_the_saved_push_branch() -> Result<(), String> {
    let w = world("direct-saved-branch")?;
    let before = remote_main(&w);
    run(&w.mine, &["config", "branch.main.gitcraftPushBranch", "dev"])?;
    let head = commit(&w.mine, "saved destination")?;
    let ctx = egui::Context::default();
    let mut tab = direct_tab(&w, &ctx)?;
    tab.apply(crate::repo::Action::Push, &ctx);
    settle(&mut tab, &ctx)?;
    assert!(tab.dialog.is_none());
    assert_eq!(remote_main(&w), before);
    assert_eq!(git::resolve(&w.bare, "refs/heads/dev")?, head);
    Ok(())
}

#[test]
fn cancelling_ordinary_push_preparation_never_sends() -> Result<(), String> {
    let w = world("direct-cancel")?;
    let before = remote_main(&w);
    commit(&w.mine, "cancel me")?;
    let ctx = egui::Context::default();
    let mut tab = direct_tab(&w, &ctx)?;
    tab.apply(crate::repo::Action::Push, &ctx);
    assert!(tab.can_cancel());
    tab.cancel_op();
    settle(&mut tab, &ctx)?;
    assert_eq!(remote_main(&w), before);
    Ok(())
}

#[test]
fn offline_push_settles_preparation_and_drops_queued_actions() -> Result<(), String> {
    let w = world("direct-offline")?;
    let before = remote_main(&w);
    commit(&w.mine, "offline")?;
    let ctx = egui::Context::default();
    let mut tab = direct_tab(&w, &ctx)?;
    let _offline = crate::network::test_pause::set(Some(crate::network::Pause::Switch));
    tab.apply(crate::repo::Action::Push, &ctx);
    tab.waiting.push_back(crate::repo::running::Waiter::Push);
    settle(&mut tab, &ctx)?;
    assert_eq!(remote_main(&w), before);
    assert!(tab.running.is_none());
    assert!(tab.waiting.is_empty());
    assert!(!tab.can_cancel());
    assert!(tab.taskbar_progress().is_none());
    Ok(())
}
