use super::*;
#[test]
fn real_commit_worker_pins_its_target_before_a_later_commit_and_delayed_read() -> Result<(), String>
{
    use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
    struct Fixture(std::path::PathBuf);
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    let fixture = Fixture(std::env::temp_dir().join(format!(
            "gitcraft-celebration-worker-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_err(|e| e.to_string())?
                .as_nanos()
        )));
    std::fs::create_dir(&fixture.0).map_err(|e| e.to_string())?;
    let git = |args: &[&str]| crate::git::run(Some(&fixture.0), args);
    for args in [
        &["init", "-q"][..],
        &["config", "user.name", "Test"],
        &["config", "user.email", "test@example.invalid"],
        &["config", "commit.gpgSign", "false"],
    ] {
        git(args)?;
    }
    let ctx = egui::Context::default();
    crate::settings::Settings {
        celebration_sound: true,
        ..Default::default()
    }
    .set(&ctx);
    crate::audio::take_recorded();
    let mut tab = RepoTab::new_deferred(fixture.0.clone(), &ctx);
    tab.life.phase = super::super::lifecycle::Phase::Live;
    tab.loading = true; // delay the confirming reload; do not start a watcher
    tab.run_ops_ex(
        "Commit",
        super::super::ops(&[&["commit", "--allow-empty", "-m", "first"]]),
        super::super::Refresh::All,
        super::super::OpKind::plain(),
        true,
        &ctx,
    );
    let start = Instant::now();
    while tab.busy_op.is_some() {
        while let Ok(msg) = tab.rx.try_recv() {
            tab.handle(msg, &ctx);
        }
        if start.elapsed() > Duration::from_secs(5) {
            return Err("Owned commit did not complete".into());
        }
        std::thread::yield_now();
    }
    let first = crate::git::head(&fixture.0);
    assert_eq!(tab.celebration.pending.len(), 1);
    assert_eq!(
        tab.celebration.pending[0].2.as_deref(),
        Some(first.as_str())
    );
    assert!(crate::audio::take_recorded().is_empty());
    git(&["commit", "--allow-empty", "-m", "second"])?;
    tab.refs.head = crate::git::head(&fixture.0);
    tab.history = crate::git::log(&fixture.0, None, crate::git::LogOrder::default(), 0, 10)?
        .into_iter()
        .map(std::sync::Arc::new)
        .collect();
    tab.celebration_loaded(tab.reads + 1, &ctx);
    assert_eq!(
        crate::audio::take_recorded(),
        [crate::audio::Cue::Celebration]
    );
    // Keep the tab's old HEAD while another actor creates a real merge.
    let second = tab.refs.head.clone();
    let merged = git(&[
        "commit-tree",
        "HEAD^{tree}",
        "-p",
        &second,
        "-p",
        &first,
        "-m",
        "merge",
    ])?;
    let merged = merged.trim().to_owned();
    git(&["update-ref", "HEAD", &merged])?;
    tab.run_ops_ex(
        "Merge",
        super::super::ops(&[&["merge", "--ff-only", "HEAD"]]),
        super::super::Refresh::All,
        super::super::OpKind::plain(),
        true,
        &ctx,
    );
    let start = Instant::now();
    while tab.busy_op.is_some() {
        while let Ok(msg) = tab.rx.try_recv() {
            tab.handle(msg, &ctx);
        }
        if start.elapsed() > Duration::from_secs(5) {
            return Err("Owned no-op merge did not complete".into());
        }
        std::thread::yield_now();
    }
    tab.refs.head = merged;
    tab.history = crate::git::log(&fixture.0, None, crate::git::LogOrder::default(), 0, 10)?
        .into_iter()
        .map(std::sync::Arc::new)
        .collect();
    tab.celebration_loaded(tab.reads + 1, &ctx);
    assert!(crate::audio::take_recorded().is_empty());
    Ok(())
}
#[test]
fn confirmation_uses_the_operation_target_after_later_head_moves() {
    let ctx = egui::Context::default();
    crate::settings::Settings {
        celebration_sound: true,
        ..Default::default()
    }
    .set(&ctx);
    crate::audio::take_recorded();
    let mut tab = RepoTab::new_deferred("/nonexistent/exact-celebration-target".into(), &ctx);
    tab.refs.head = "old".into();
    tab.celebration_begin("Merge", &[vec!["merge".into(), "topic".into()]], &ctx);
    tab.celebration_observed_heads("old".into(), "old".into()); // success was a no-op
    tab.celebration_done(true);
    tab.refs.head = "later".into();
    tab.history = vec![std::sync::Arc::new(crate::git::Commit {
        hash: "later".into(),
        parents: vec!["old".into(), "topic".into()],
        ..Default::default()
    })];
    tab.celebration_loaded(tab.reads + 1, &ctx);
    assert!(crate::audio::take_recorded().is_empty());
    tab.refs = Default::default();
    tab.history.clear();
    tab.celebration_begin("Commit", &[vec!["commit".into()]], &ctx);
    tab.celebration_observed_heads(String::new(), "root".into());
    tab.celebration_done(true);
    tab.refs.head = "second".into();
    tab.history = vec![
        std::sync::Arc::new(crate::git::Commit {
            hash: "second".into(),
            parents: vec!["root".into()],
            ..Default::default()
        }),
        std::sync::Arc::new(crate::git::Commit {
            hash: "root".into(),
            ..Default::default()
        }),
    ];
    tab.celebration_loaded(tab.reads + 1, &ctx);
    assert_eq!(
        crate::audio::take_recorded(),
        [crate::audio::Cue::Celebration]
    );
}
#[test]
fn successful_first_commit_survives_queued_push_then_failure_until_confirmed_read()
-> Result<(), String> {
    let f = crate::push_preflight::tests::Fixture::new()?;
    f.commit("outgoing.txt", "outgoing\n", "Outgoing")?;
    let ctx = egui::Context::default();
    crate::settings::Settings {
        celebration_sound: true,
        ..Default::default()
    }
    .set(&ctx);
    crate::audio::take_recorded();
    let mut tab = RepoTab::new_deferred(f.repo.clone(), &ctx);
    tab.remotes = crate::git::remotes(&f.repo)?;
    tab.life.phase = super::super::lifecycle::Phase::Live;
    tab.loading = true;
    tab.celebration_begin("Commit", &[vec!["commit".into()]], &ctx);
    tab.busy_op = Some("Commit".into());
    tab.refs.current = Some("main".into());
    tab.refs.local.push(crate::git::Branch {
        name: "main".into(),
        target: "root".into(),
        is_head: true,
        upstream: "origin/main".into(),
        track: String::new(),
    });
    tab.waiting.push_back(super::super::running::Waiter::Push);
    tab.celebration_observed_heads(String::new(), "root".into());
    let theirs = f.advance_receiver()?;
    tab.op_done(
        "Commit".into(),
        Ok(String::new()),
        super::super::Refresh::All,
        super::super::OpKind::plain(),
        true,
        &ctx,
    );
    assert!(tab.busy_op.is_some(), "queued push starts directly");
    assert!(tab.dialog.is_none());
    assert_eq!(tab.celebration.pending.len(), 1);
    assert!(crate::audio::take_recorded().is_empty());
    // Handle only operation settlement: a real refresh would confirm the celebration,
    // which this test deliberately checks with synthetic reads below.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    loop {
        let msg = tab
            .rx
            .recv_timeout(deadline.saturating_duration_since(std::time::Instant::now()))
            .map_err(|e| e.to_string())?;
        if matches!(&msg, super::super::Msg::OpDone { label, .. } if label == "Push") {
            tab.handle(msg, &ctx);
            break;
        }
        if let super::super::Msg::Then(next) = msg {
            next(&mut tab, &ctx);
        }
        if std::time::Instant::now() >= deadline {
            return Err("Push did not settle".into());
        }
    }
    assert!(
        tab.activity
            .last()
            .is_some_and(|entry| matches!(&entry.outcome,
        super::super::activity::Outcome::Failed(error) if error.contains("rejected")))
    );
    assert_eq!(f.receiver_head()?, theirs);

    assert_eq!(tab.celebration.pending.len(), 1);
    tab.refs.head = "root".into();
    tab.history.push(std::sync::Arc::new(crate::git::Commit {
        hash: "root".into(),
        ..Default::default()
    }));
    tab.celebration_loaded(tab.reads, &ctx);
    assert!(crate::audio::take_recorded().is_empty());
    tab.celebration_loaded(tab.reads + 1, &ctx);
    assert_eq!(
        crate::audio::take_recorded(),
        [crate::audio::Cue::Celebration]
    );
    tab.celebration_loaded(tab.reads + 2, &ctx);
    assert!(crate::audio::take_recorded().is_empty());
    Ok(())
}
#[test]
fn merging_pull_celebrates_confirmed_merge_but_not_fast_forward_or_failure() {
    let ctx = egui::Context::default();
    crate::settings::Settings {
        celebration_sound: true,
        ..Default::default()
    }
    .set(&ctx);
    let mut tab = RepoTab::new_deferred("/nonexistent/pull-celebration".into(), &ctx);
    let pull = super::super::ops(&[super::super::pull::PullMode::Merge.args()]);
    for (success, parents, expected) in [
        (true, vec!["old", "topic"], 1),
        (true, vec!["old"], 0),
        (false, vec!["old", "topic"], 0),
    ] {
        crate::audio::take_recorded();
        tab.refs.head = "old".into();
        tab.celebration_begin("Sync: pull", &pull, &ctx);
        tab.celebration_observed_heads("old".into(), "new".into());
        tab.celebration_done(success);
        tab.celebration_begin("Sync: push", &[vec!["push".into()]], &ctx);
        tab.celebration_done(true);
        tab.refs.head = "new".into();
        tab.history = vec![std::sync::Arc::new(crate::git::Commit {
            hash: "new".into(),
            parents: parents.into_iter().map(String::from).collect(),
            ..Default::default()
        })];
        tab.celebration_loaded(tab.reads + 1, &ctx);
        assert_eq!(crate::audio::take_recorded().len(), expected);
    }
}
#[test]
fn stale_ui_head_does_not_celebrate_an_already_completed_merge_on_pull() {
    let ctx = egui::Context::default();
    crate::settings::Settings {
        celebration_sound: true,
        ..Default::default()
    }
    .set(&ctx);
    crate::audio::take_recorded();
    let mut tab = RepoTab::new_deferred("/nonexistent/stale-pull-celebration".into(), &ctx);
    tab.refs.head = "old".into();
    tab.celebration_begin(
        "Pull",
        &super::super::ops(&[super::super::pull::PullMode::Merge.args()]),
        &ctx,
    );
    tab.celebration_observed_heads("merge".into(), "merge".into());
    tab.celebration_done(true);
    tab.history = vec![std::sync::Arc::new(crate::git::Commit {
        hash: "merge".into(),
        parents: vec!["old".into(), "topic".into()],
        ..Default::default()
    })];
    tab.celebration_loaded(tab.reads + 1, &ctx);
    assert!(crate::audio::take_recorded().is_empty());
}
#[test]
fn failed_noop_and_unconfirmed_operations_do_not_celebrate() {
    let ctx = egui::Context::default();
    let settings = crate::settings::Settings {
        celebration_sound: true,
        ..Default::default()
    };
    settings.set(&ctx);
    let mut tab = RepoTab::new_deferred("/nonexistent/celebrate".into(), &ctx);
    crate::audio::take_recorded();
    tab.refs.head = "old".into();
    let merge = vec![vec!["merge".into(), "topic".into()]];
    tab.celebration_begin("Merge topic", &merge, &ctx);
    tab.celebration_done(false);
    assert!(tab.celebration.candidate.is_none());
    tab.celebration_begin("Merge topic", &merge, &ctx);
    tab.celebration_observed_heads("old".into(), "old".into());
    tab.celebration_done(true);
    tab.celebration_loaded(tab.reads + 1, &ctx);
    assert!(tab.celebration.visible.is_none());
    assert!(tab.celebration.candidate.is_none());
    assert!(crate::audio::take_recorded().is_empty());
    tab.celebration_begin("Merge topic", &merge, &ctx);
    tab.celebration_observed_heads("old".into(), "merged".into());
    tab.celebration_done(true);
    tab.refs.head = "merged".into();
    tab.history.push(std::sync::Arc::new(crate::git::Commit {
        hash: "merged".into(),
        ..Default::default()
    }));
    tab.celebration_loaded(0, &ctx);
    assert_eq!(tab.celebration.pending.len(), 1);
    tab.celebration_loaded(tab.reads + 1, &ctx);
    assert!(tab.celebration.candidate.is_none());
    assert_eq!(
        crate::audio::take_recorded(),
        [crate::audio::Cue::Celebration]
    );
    tab.celebration_loaded(tab.reads + 2, &ctx);
    assert!(crate::audio::take_recorded().is_empty());
}
#[test]
fn first_commit_is_opt_in_confirmed_once_and_sound_is_independent_of_motion() {
    let ctx = egui::Context::default();
    ctx.all_styles_mut(|style| style.animation_time = 0.0);
    let mut tab = RepoTab::new_deferred("/nonexistent/first-celebrate".into(), &ctx);
    let commit = vec![vec!["commit".into(), "-m".into(), "first".into()]];
    tab.celebration_begin("Commit", &commit, &ctx);
    assert!(tab.celebration.candidate.is_none());
    crate::settings::Settings {
        celebration_sound: true,
        celebration_motion: true,
        ..Default::default()
    }
    .set(&ctx);
    crate::audio::take_recorded();
    tab.celebration_begin("Commit", &commit, &ctx);
    tab.celebration_observed_heads(String::new(), "root".into());
    tab.celebration_done(true);
    tab.refs.head = "root".into();
    tab.history.push(std::sync::Arc::new(crate::git::Commit {
        hash: "root".into(),
        ..Default::default()
    }));
    tab.celebration_loaded(tab.reads + 1, &ctx);
    assert!(tab.celebration.visible.is_none());
    assert_eq!(
        crate::audio::take_recorded(),
        [crate::audio::Cue::Celebration]
    );
    tab.celebration_loaded(tab.reads + 2, &ctx);
    assert!(crate::audio::take_recorded().is_empty());
}
#[test]
fn release_requires_explicit_designation_and_confirmed_target() {
    let ctx = egui::Context::default();
    crate::settings::Settings {
        celebration_sound: true,
        ..Default::default()
    }
    .set(&ctx);
    let mut tab = RepoTab::new_deferred("/nonexistent/release-celebrate".into(), &ctx);
    let tag = vec![vec!["tag".into(), "release-any-name".into(), "abc".into()]];
    tab.celebration_begin("Create tag release-any-name", &tag, &ctx);
    assert!(tab.celebration.candidate.is_none());
    tab.celebration_begin("Create release tag release-any-name", &tag, &ctx);
    assert!(matches!(
        tab.celebration.candidate,
        Some(Event::ReleaseTag { .. })
    ));
    tab.celebration_done(true);
    tab.celebration_loaded(tab.reads + 1, &ctx);
    assert!(tab.celebration.visible.is_none());
}
