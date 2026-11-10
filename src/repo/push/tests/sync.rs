/// Sync's push waits for what started after its pull (updating the submodules), then runs.
#[test]
fn syncs_push_waits_for_the_submodule_update_after_the_pull() {
    let ctx = egui::Context::default();
    let mut tab = crate::repo::RepoTab::new_deferred("/nonexistent/sync-chain".into(), &ctx);
    let push = Attempt {
        label: "Sync: push".to_owned(),
        branch: "main".to_owned(),
        target: None,
        commit: None,
        publish: false,
        done: None,
    };
    tab.push_flow.then_push = Some(Chain {
        after: "Sync: pull".to_owned(),
        push,
        pulled: false,
    });
    // The pull ended, and the submodule update started right after it.
    tab.busy_op = Some("Update submodules".to_owned());
    tab.push_op_ended("Sync: pull", &Ok(String::new()), &ctx);
    let waiting = tab
        .push_flow
        .then_push
        .as_ref()
        .map(|c| (c.after.clone(), c.pulled));
    assert_eq!(waiting, Some(("Update submodules".to_owned(), true)));
    // It ended (even failing): the push is started. With no branch read, it says HEAD is
    // detached, which shows it ran.
    tab.busy_op = None;
    tab.push_op_ended("Update submodules", &Err("failed".to_owned()), &ctx);
    assert!(tab.push_flow.then_push.is_none());
    assert!(matches!(
        tab.push_flow.banner,
        Some(super::Banner::Detached)
    ));
}

/// A failed pull stops Sync: nothing is pushed, and the status says so.
#[test]
fn a_failed_sync_pull_pushes_nothing() {
    let ctx = egui::Context::default();
    let mut tab = crate::repo::RepoTab::new_deferred("/nonexistent/sync-stop".into(), &ctx);
    let push = Attempt {
        label: "Sync: push".to_owned(),
        branch: "main".to_owned(),
        target: None,
        commit: None,
        publish: false,
        done: None,
    };
    tab.push_flow.then_push = Some(Chain {
        after: "Sync: pull".to_owned(),
        push,
        pulled: false,
    });
    tab.push_op_ended(
        "Sync: pull",
        &Err("CONFLICT (content): f.txt".to_owned()),
        &ctx,
    );
    assert!(tab.push_flow.then_push.is_none());
    assert!(tab.push_flow.banner.is_none());
    let status = tab
        .status
        .as_deref()
        .cloned()
        .map(|(s, err)| (s.contains("nothing was pushed"), err));
    assert_eq!(status, Some((true, true)));
}
