use super::*;

#[test]
fn quick_drop_refuses_a_different_branch_at_the_same_commit() -> Result<(), String> {
    use crate::repo::{Action, Msg, RepoTab, quick_edit::Edit};
    let dir = repo("quick-drop-same-sha");
    git::run(Some(&dir), &["checkout", "-qb", "original"])?;
    let base = git::head(&dir);
    git::run(Some(&dir), &["commit", "-q", "--allow-empty", "-m", "tip"])?;
    let tip = git::head(&dir);
    git::run(Some(&dir), &["branch", "other"])?;
    let ctx = egui::Context::default();
    let mut tab = RepoTab::new_deferred(dir.clone(), &ctx);
    tab.refs.current = Some("original".into());
    tab.refs.head = tip.clone();
    let captured = Action::QuickEdit {
        edit: Edit::Drop,
        hash: tip.clone(),
        branch_guard: tab.same_branch(),
    };
    git::run(Some(&dir), &["checkout", "-q", "other"])?;
    tab.refs.current = Some("other".into());
    let execute = |tab: &mut RepoTab, action: Action| -> Result<bool, String> {
        tab.apply(action, &ctx);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
        loop {
            let remaining = deadline.saturating_duration_since(std::time::Instant::now());
            let msg = tab.rx.recv_timeout(remaining).map_err(|e| e.to_string())?;
            let completed = match &msg {
                Msg::OpDone { result, .. } => Some(result.is_ok()),
                _ => None,
            };
            tab.handle(msg, &ctx);
            if let Some(ok) = completed {
                return Ok(ok);
            }
        }
    };
    assert!(!execute(&mut tab, captured)?);
    assert_eq!(git::head(&dir), tip);
    assert_eq!(git::resolve(&dir, "refs/heads/original")?, tip);
    assert_eq!(git::resolve(&dir, "refs/heads/other")?, tip);
    assert!(
        tab.timeline
            .entries
            .iter()
            .all(|entry| entry.undo.is_none())
    );
    assert!(git::in_progress(&dir)?.is_none());
    let fresh = Action::QuickEdit {
        edit: Edit::Drop,
        hash: tip.clone(),
        branch_guard: tab.same_branch(),
    };
    assert!(execute(&mut tab, fresh)?);
    assert_eq!(git::head(&dir), base);
    assert_eq!(git::resolve(&dir, "refs/heads/original")?, tip);
    let _ = std::fs::remove_dir_all(dir);
    Ok(())
}

#[test]
fn remote_spawn_mutation_serializes_whole_recorded_operations() -> Result<(), String> {
    use std::sync::mpsc::channel;
    use std::time::Duration;
    let ctx = egui::Context::default();
    let a = crate::repo::RepoTab::new_deferred("ssh://build/main".into(), &ctx);
    let b = crate::repo::RepoTab::new_deferred("ssh://alias/linked".into(), &ctx);
    assert_ne!(a.tx.repo, b.tx.repo);
    let (release, gate) = channel::<()>();
    let (out, events) = channel();
    let record = out.clone();
    crate::repo::messages::spawn_mutation(&a.tx, &ctx, crate::repo::Msg::JobPanicked, move || {
        let _ = record.send("expect refs/stash");
        let _ = gate.recv();
        let _ = record.send("stash pop --index");
        crate::repo::Msg::Then(Box::new(|_, _| {}))
    });
    assert_eq!(
        events
            .recv_timeout(Duration::from_secs(5))
            .map_err(|e| e.to_string())?,
        "expect refs/stash"
    );
    let unrelated = crate::tasks::Cancellation::default();
    let unrelated_out = out.clone();
    crate::tasks::pool()?
        .submit(
            unrelated.clone(),
            crate::tasks::Priority::Background,
            Some("ssh://unrelated/fixture".into()),
            Box::new(move || {
                let _ = unrelated_out.send("unrelated remote job");
            }),
        )
        .map_err(str::to_owned)?;
    let record = out.clone();
    crate::repo::messages::spawn_mutation(&b.tx, &ctx, crate::repo::Msg::JobPanicked, move || {
        let _ = record.send("stash push");
        crate::repo::Msg::Then(Box::new(|_, _| {}))
    });
    assert!(
        crate::tasks::pool()?.queued_priority_for(&b.tx.cancel)
            == Some(crate::tasks::Priority::Background)
    );
    assert!(
        crate::tasks::pool()?.queued_priority_for(&unrelated)
            == Some(crate::tasks::Priority::Background)
    );
    // An ordinary tab read remains unkeyed and runs while A holds the bucket.
    crate::repo::messages::spawn(&b.tx, &ctx, move || {
        let _ = out.send("read");
        crate::repo::Msg::Then(Box::new(|_, _| {}))
    });
    assert_eq!(
        events
            .recv_timeout(Duration::from_secs(5))
            .map_err(|e| e.to_string())?,
        "read"
    );
    drop(release);
    assert_eq!(
        events
            .recv_timeout(Duration::from_secs(5))
            .map_err(|e| e.to_string())?,
        "stash pop --index"
    );
    assert_eq!(
        events
            .recv_timeout(Duration::from_secs(5))
            .map_err(|e| e.to_string())?,
        "unrelated remote job"
    );
    assert_eq!(
        events
            .recv_timeout(Duration::from_secs(5))
            .map_err(|e| e.to_string())?,
        "stash push"
    );
    Ok(())
}

#[test]
fn successful_prefetch_settles_omitted_and_oversized_entries() {
    let ctx = egui::Context::default();
    let mut tab = crate::repo::RepoTab::new_deferred("/inert/prefetch".into(), &ctx);
    let file = git::ChangedFile {
        status: 'M',
        path: "f".into(),
        old_path: None,
        added: None,
        removed: None,
    };
    tab.files_cache.reading = true;
    tab.handle(
        crate::repo::Msg::FilesPrefetched(
            vec!["kept".into(), "omitted".into(), "oversized".into()],
            Ok(vec![
                ("kept".into(), vec![file.clone()]),
                ("oversized".into(), vec![file; 5001]),
            ]),
        ),
        &ctx,
    );
    assert!(!tab.files_cache.reading);
    assert_eq!(
        tab.files_cache.get("kept").map(|files| files.len()),
        Some(1)
    );
    for hash in ["kept", "omitted", "oversized"] {
        assert!(tab.files_cache.settled(hash));
    }
    assert!(tab.files_cache.get("omitted").is_none());
    assert!(tab.files_cache.get("oversized").is_none());
}

#[test]
fn completed_read_remains_recent_for_focus_without_retaining_its_timer() {
    let ctx = egui::Context::default();
    let mut tab = crate::repo::RepoTab::new_deferred("/inert/focus-read".into(), &ctx);
    assert!(!tab.just_read());
    let started = std::time::Instant::now();
    tab.loading = true;
    tab.load_started = Some(started);
    tab.last_load_started = Some(started);
    crate::repo::messages::record_jobs(|| {
        tab.handle(crate::repo::Msg::Loaded(Err("fixture".into())), &ctx)
    });
    assert!(tab.load_started.is_none());
    assert!(!tab.loading);
    assert!(tab.just_read());
    tab.last_load_started = Some(started - std::time::Duration::from_secs(4));
    assert!(!tab.just_read());
    tab.loading = true;
    assert!(tab.just_read());
}

#[test]
fn startup_snapshot_cannot_authorize_merge_even_after_a_failed_load() {
    let ctx = egui::Context::default();
    for failed in [false, true] {
        let mut tab = crate::repo::RepoTab::new_deferred("/inert/startup-snapshot".into(), &ctx);
        tab.snapshot_shown = true;
        tab.refs.current = Some("cached-old-branch".into());
        tab.load_error = failed.then(|| "read failed".into());
        assert!(tab.is_busy());
        let (_, jobs) = crate::repo::messages::record_jobs(|| {
            tab.apply(
                crate::repo::merge_action(
                    crate::repo::actions::BranchRef::local("incoming"),
                    "cached-old-branch",
                ),
                &ctx,
            );
            tab.run_ops(
                "direct merge",
                crate::repo::ops(&[&["merge", "incoming"]]),
                crate::repo::Refresh::All,
                crate::repo::OpKind::plain(),
                &ctx,
            );
        });
        assert!(jobs.is_empty());
        assert!(tab.busy_op.is_none());
        // A successful fresh read advances history_gen and retires the snapshot.
        tab.history_gen = 1;
        tab.snapshot_shown = false;
        tab.load_error = None;
        assert!(!tab.is_busy());
        let (_, jobs) = crate::repo::messages::record_jobs(|| {
            tab.apply(
                crate::repo::merge_action(
                    crate::repo::actions::BranchRef::local("incoming"),
                    "fresh-branch",
                ),
                &ctx,
            );
        });
        assert!(!jobs.is_empty());
        assert!(tab.busy_op.is_some());
    }
}

#[test]
fn startup_snapshot_remotes_stay_unread_until_a_fresh_load() {
    let ctx = egui::Context::default();
    let mut tab = crate::repo::RepoTab::new_deferred("/inert/snapshot-remotes".into(), &ctx);
    assert!(tab.remotes_read().is_none());
    tab.snapshot_shown = true;
    for failed in [false, true] {
        tab.load_error = failed.then(|| "read failed".into());
        assert!(tab.remotes_read().is_none());
    }
    tab.history_gen = 1;
    tab.snapshot_shown = false;
    tab.load_error = None;
    assert!(tab.remotes_read().is_some_and(|remotes| remotes.is_empty()));
}

#[test]
fn startup_snapshot_retains_queued_links_through_failed_loading() {
    let ctx = egui::Context::default();
    let mut tab = crate::repo::RepoTab::new_deferred("/inert/snapshot-links".into(), &ctx);
    tab.snapshot_shown = true;
    tab.queue(crate::repo::Action::OpenLinkedPr {
        number: 42,
        remote: "https://github.com/example/repo".into(),
    });
    tab.queue(crate::repo::Action::BlameAt {
        path: "src/lib.rs".into(),
        rev: Some("main".into()),
        line: 17,
    });
    assert!(tab.take_queued_actions().is_empty());
    crate::repo::messages::record_jobs(|| {
        tab.handle(crate::repo::Msg::Loaded(Err("read failed".into())), &ctx);
    });
    assert!(tab.take_queued_actions().is_empty());
    assert!(tab.is_busy());
    // Successful loading advances the generation and retires the startup snapshot.
    tab.history_gen = 1;
    tab.snapshot_shown = false;
    tab.load_error = None;
    let actions = tab.take_queued_actions();
    assert!(matches!(
        actions.as_slice(),
        [crate::repo::Action::OpenLinkedPr { number: 42, remote },
         crate::repo::Action::BlameAt { path, rev: Some(rev), line: 17 }]
            if remote == "https://github.com/example/repo"
                && path == "src/lib.rs" && rev == "main"
    ));
    assert!(tab.take_queued_actions().is_empty());
}
