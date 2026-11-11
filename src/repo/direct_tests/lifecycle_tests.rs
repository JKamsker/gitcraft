use super::*;

#[test]
fn suspension_releases_prefetch_but_keeps_completed_entries() {
    let ctx = egui::Context::default();
    let mut tab = crate::repo::RepoTab::new_deferred("/inert/prefetch-suspend".into(), &ctx);
    tab.files_cache.insert("kept".into(), Vec::new());
    tab.files_cache.reading = true;
    tab.suspend();
    assert!(!tab.files_cache.reading);
    assert!(tab.files_cache.settled("kept"));
}

#[test]
fn linked_local_tabs_keep_the_local_schedule_after_suspension() -> Result<(), String> {
    let scratch = tempfile::tempdir().map_err(|error| error.to_string())?;
    let main = scratch.path().join("main");
    let linked = scratch.path().join("linked");
    std::fs::create_dir(&main).map_err(|error| error.to_string())?;
    crate::git::run(Some(&main), &["init", "-q"])?;
    crate::git::run(
        Some(&main),
        &[
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@example.invalid",
            "commit",
            "-qm",
            "base",
            "--allow-empty",
        ],
    )?;
    crate::git::run(
        Some(&main),
        &[
            "worktree",
            "add",
            "--detach",
            linked.to_str().ok_or("fixture path")?,
        ],
    )?;
    let ctx = egui::Context::default();
    let mut a = crate::repo::RepoTab::new_deferred(main, &ctx);
    let mut b = crate::repo::RepoTab::new_deferred(linked, &ctx);
    let stable = |tab: &crate::repo::RepoTab| {
        assert_eq!(
            crate::tasks::mutation_schedule_key(tab.tx.repo.clone()),
            crate::tasks::MutationScheduleKey::Local
        );
    };
    stable(&a);
    stable(&b);
    a.suspend();
    b.suspend();
    stable(&a);
    stable(&b);
    Ok(())
}

#[test]
fn suspension_retires_retained_file_view_load_and_probe_slots() -> Result<(), String> {
    use crate::repo::file_view::{FileTab, FileView};
    let ctx = egui::Context::default();
    for rev in [None, Some("a".repeat(40))] {
        let mut tab = crate::repo::RepoTab::new_deferred("/inert/suspended-file-view".into(), &ctx);
        tab.file_view = Some(FileView::new("same.txt", rev, FileTab::File));
        let generation = tab.file_view.as_ref().ok_or("missing view")?.generation;
        let (_, old_jobs) = crate::repo::messages::record_jobs(|| {
            tab.load_file_view(&ctx);
            tab.status_changed_file_view(&ctx);
        });
        assert_eq!(old_jobs.len(), 2);
        tab.suspend();
        assert_ne!(
            tab.file_view.as_ref().ok_or("missing view")?.generation,
            generation
        );
        let (_, jobs) = crate::repo::messages::record_jobs(|| {
            tab.load_file_view(&ctx);
            tab.status_changed_file_view(&ctx);
        });
        assert_eq!(
            jobs.len(),
            2,
            "retained requests can be resubmitted on the new lifetime"
        );
    }
    Ok(())
}

#[test]
fn remote_mutation_schedule_survives_discovery_failure_and_suspend() {
    use crate::repo::{RepoTab, messages::Msg};
    use crate::tasks::{MutationScheduleKey, mutation_schedule_key};
    let ctx = egui::Context::default();
    let mut a = RepoTab::new_deferred("ssh://build/main".into(), &ctx);
    let mut b = RepoTab::new_deferred("ssh://build/linked".into(), &ctx);
    let stable = |tab: &RepoTab| {
        assert_eq!(
            mutation_schedule_key(tab.tx.repo.clone()),
            MutationScheduleKey::Remote
        );
    };
    assert_ne!(a.tx.repo, b.tx.repo);
    stable(&a);
    stable(&b);
    // Synthetic successful discovery takes the same identity path as Loaded.
    let discovered = vec![crate::git::Worktree {
        path: a.path.clone(),
        main: true,
        ..Default::default()
    }];
    a.take_worktree_identity(discovered.clone(), String::new());
    b.take_worktree_identity(discovered, String::new());
    assert_eq!(a.tx.repo, b.tx.repo);
    stable(&a);
    stable(&b);
    // Empty discovery, failed load and suspension can restore distinct identities.
    b.take_worktree_identity(Vec::new(), String::new());
    assert_ne!(a.tx.repo, b.tx.repo);
    b.handle(Msg::Loaded(Err("synthetic discovery failure".into())), &ctx);
    stable(&b);
    a.life.phase = Phase::Suspended;
    b.life.phase = Phase::Suspended;
    a.suspend();
    b.suspend();
    assert_ne!(a.tx.repo, b.tx.repo);
    stable(&a);
    stable(&b);
    // An alias or a different host is conservative too; no local path collides.
    assert_eq!(
        mutation_schedule_key("ssh://alias/elsewhere".into()),
        MutationScheduleKey::Remote
    );
    assert_eq!(
        mutation_schedule_key("/local/common.git".into()),
        MutationScheduleKey::Local
    );
}

#[test]
fn suspension_retires_private_analysis_readers_and_their_pending_captures() -> Result<(), String> {
    let ctx = egui::Context::default();
    let mut tab = crate::repo::RepoTab::new_deferred("/nonexistent/suspend-analysis".into(), &ctx);
    tab.life.phase = Phase::Live;
    let (entered_tx, entered_rx) = std::sync::mpsc::channel();
    let (release_tx, release_rx) = std::sync::mpsc::channel();
    let (finished_tx, finished_rx) = std::sync::mpsc::channel();
    tab.band_reader.queue(move |active| {
        entered_tx.send(()).map_err(|e| e.to_string())?;
        release_rx
            .recv_timeout(Duration::from_secs(5))
            .map_err(|e| e.to_string())?;
        finished_tx.send(active()).map_err(|e| e.to_string())?;
        // A late success must be discarded even if its worker ignores cancellation.
        Ok(crate::graph::Graph {
            rows: Vec::new().into(),
            width: 42,
        })
    });
    tab.band_reader.poll(&tab.tx, &ctx);
    entered_rx
        .recv_timeout(Duration::from_secs(5))
        .map_err(|e| e.to_string())?;
    tab.topology_reader.queue(|_| Ok(Default::default()));
    tab.atlas = Some(Default::default());
    tab.growth = Some(Default::default());
    tab.life.phase = Phase::Suspended;
    tab.suspend();
    release_tx.send(()).map_err(|e| e.to_string())?;
    assert!(
        !finished_rx
            .recv_timeout(Duration::from_secs(5))
            .map_err(|e| e.to_string())?
    );
    assert!(!tab.band_reader.busy());
    assert!(!tab.topology_reader.busy());
    assert!(tab.atlas.is_none());
    assert!(tab.growth.is_none());
    tab.poll_parked_view(&ctx);
    assert_eq!(tab.graph.width, 0);
    assert!(tab.topology.parents.is_empty());
    assert!(!tab.band_reader.busy());
    assert!(!tab.topology_reader.busy());
    Ok(())
}

#[test]
fn a_restored_tab_starts_only_when_first_shown() {
    let t0 = Instant::now();
    let mut life = Lifecycle::new(t0);
    assert!(life.sleeping());
    // In the background however long: still nothing.
    assert_eq!(life.background(t0 + IDLE_AFTER * 3, || true), Step::Nothing);
    assert_eq!(life.phase, Phase::Deferred);
    assert_eq!(life.shown(t0 + IDLE_AFTER * 3), Step::Start);
    assert_eq!(life.phase, Phase::Live);
    assert_eq!(life.shown(t0 + IDLE_AFTER * 3), Step::Nothing);
}

#[test]
fn an_idle_tab_is_suspended_after_a_while_and_resumes_when_shown() {
    let t0 = Instant::now();
    let mut life = Lifecycle::new(t0);
    life.shown(t0);
    let just_before = t0 + IDLE_AFTER - Duration::from_secs(1);
    assert_eq!(life.background(just_before, || true), Step::Nothing);
    assert_eq!(life.background(t0 + IDLE_AFTER, || true), Step::Suspend);
    assert!(life.sleeping());
    // Once is enough.
    assert_eq!(life.background(t0 + IDLE_AFTER * 2, || true), Step::Nothing);
    assert_eq!(life.shown(t0 + IDLE_AFTER * 2), Step::Resume);
    assert!(!life.sleeping());
}

#[test]
fn a_tab_with_work_going_on_is_never_suspended() {
    let t0 = Instant::now();
    let mut life = Lifecycle::new(t0);
    life.shown(t0);
    assert_eq!(
        life.background(t0 + IDLE_AFTER * 5, || false),
        Step::Nothing
    );
    assert_eq!(life.phase, Phase::Live);
    // Its work done, the time counts from when it was last shown.
    assert_eq!(life.background(t0 + IDLE_AFTER * 5, || true), Step::Suspend);
}

#[test]
fn being_shown_again_restarts_the_clock() {
    let t0 = Instant::now();
    let mut life = Lifecycle::new(t0);
    life.shown(t0);
    let later = t0 + IDLE_AFTER - Duration::from_secs(5);
    life.shown(later);
    assert_eq!(life.background(t0 + IDLE_AFTER, || true), Step::Nothing);
    assert_eq!(life.background(later + IDLE_AFTER, || true), Step::Suspend);
}

/// A real repository (a name with spaces and letters outside ASCII), through the tab the
/// UI uses: nothing read or watched until shown; suspended after being idle, it drops its
/// graph and watcher, and shown again it reads afresh with the commit selected again.
#[test]
fn a_real_tab_reads_when_shown_and_reads_again_after_suspending() -> Result<(), String> {
    use crate::git;
    use crate::repo::RepoTab;
    let dir = std::env::temp_dir().join(format!("gitcraft-lazy tab ü-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let result = (|| {
        for args in [
            &["init", "-q"][..],
            &["config", "user.name", "t"],
            &["config", "user.email", "t@t"],
            &["commit", "-q", "--allow-empty", "-m", "one"],
            &["commit", "-q", "--allow-empty", "-m", "two"],
        ] {
            git::run(Some(&dir), args)?;
        }
        let ctx = egui::Context::default();
        let mut tab = RepoTab::new_deferred(dir.clone(), &ctx);
        let settle = |tab: &mut RepoTab, what: &str, done: &dyn Fn(&RepoTab) -> bool| {
            let until = Instant::now() + Duration::from_secs(20);
            while !done(tab) {
                if Instant::now() > until {
                    return Err(format!("timed out waiting for {what}"));
                }
                tab.poll(&ctx);
                std::thread::sleep(Duration::from_millis(10));
            }
            Ok(())
        };
        // Polled in the background for a while: nothing read, no watcher asked for.
        for _ in 0..20 {
            tab.poll(&ctx);
            tab.background_tick();
            std::thread::sleep(Duration::from_millis(5));
        }
        let untouched = (
            tab.history.is_empty(),
            tab.watcher.is_none(),
            tab.sleeping(),
        );
        tab.start_deferred(&ctx);
        settle(&mut tab, "the first read", &|t| {
            t.history.len() == 2 && !t.loading
        })?;
        settle(&mut tab, "the watcher", &|t| t.watcher.is_some())?;
        let first = git::run(Some(&dir), &["rev-parse", "HEAD~1"])?
            .trim()
            .to_owned();
        tab.goto(&first, &ctx);
        let picked = tab.selected_hash();
        // Left alone long enough, with nothing going on.
        tab.life.shown_at = Instant::now() - IDLE_AFTER * 2;
        tab.background_tick();
        let suspended = (
            tab.sleeping(),
            tab.history.is_empty(),
            tab.graph.rows.is_empty(),
            tab.watcher.is_none(),
            tab.fs_rx.is_none(),
        );
        // A read that lands while asleep lays out nothing and keeps the selection to
        // restore.
        tab.rebuild(&ctx);
        let kept = tab.resume_select.clone();
        tab.start_deferred(&ctx);
        let opening_while_resuming = tab.opening();
        settle(&mut tab, "the read after resuming", &|t| {
            t.history.len() == 2 && !t.loading
        })?;
        settle(&mut tab, "the watcher again", &|t| t.watcher.is_some())?;
        let detail_asked = tab.detail_hash.clone();
        Ok::<_, String>((
            untouched,
            picked,
            first,
            suspended,
            tab.selected_hash(),
            (kept, opening_while_resuming, tab.opening(), detail_asked),
        ))
    })();
    let _ = std::fs::remove_dir_all(&dir);
    let (untouched, picked, first, suspended, reselected, resume) = result?;
    let (kept, opening_while_resuming, opening_after, detail_asked) = resume;
    assert_eq!(kept.as_deref(), Some(first.as_str()));
    assert!(opening_while_resuming);
    assert!(!opening_after);
    assert_eq!(detail_asked.as_deref(), Some(first.as_str()));
    assert_eq!(untouched, (true, true, true));
    assert_eq!(picked.as_deref(), Some(first.as_str()));
    assert_eq!(suspended, (true, true, true, true, true));
    assert_eq!(reselected, Some(first));
    Ok(())
}

/// Something typed in the commit message keeps the tab from being suspended.
#[test]
fn a_tab_with_a_message_typed_stays_awake() {
    use crate::repo::RepoTab;
    let ctx = egui::Context::default();
    let mut tab = RepoTab::new_deferred("/nonexistent/lazy-typed".into(), &ctx);
    tab.life.phase = Phase::Live;
    tab.life.shown_at = Instant::now() - IDLE_AFTER * 2;
    tab.commit_summary = "half-written ✍".to_owned();
    tab.background_tick();
    assert!(!tab.sleeping());
    tab.commit_summary.clear();
    tab.background_tick();
    assert!(tab.sleeping());
}

#[test]
fn whether_it_is_idle_is_asked_only_when_it_could_be_suspended() {
    let t0 = Instant::now();
    let mut life = Lifecycle::new(t0);
    let asked = std::cell::Cell::new(0);
    let idle = || {
        asked.set(asked.get() + 1);
        true
    };
    // Not live yet, then live but not long enough.
    life.background(t0 + IDLE_AFTER * 2, idle);
    life.shown(t0);
    life.background(t0 + Duration::from_secs(1), idle);
    assert_eq!(asked.get(), 0);
    life.background(t0 + IDLE_AFTER, idle);
    assert_eq!(asked.get(), 1);
    // Asleep: not asked again.
    life.background(t0 + IDLE_AFTER * 2, idle);
    assert_eq!(asked.get(), 1);
}

/// A tab not read yet that is asked to reload (coming back to the window does that for the
/// tab shown) reads once, and starts watching.
#[test]
fn a_reload_wakes_a_restored_tab_once() {
    use crate::repo::RepoTab;
    let ctx = egui::Context::default();
    let mut tab = RepoTab::new_deferred("/nonexistent/lazy-reload".into(), &ctx);
    tab.reload(&ctx);
    let reads = tab.reads;
    tab.start_deferred(&ctx);
    assert!(!tab.sleeping());
    assert_eq!(tab.reads, reads);
    assert!(!tab.reload_again);
    assert_eq!(tab.watches, 1);
}

/// A watcher set up before the tab was suspended, arriving after it was watched again, is
/// not the tab's (it sends to a channel no longer read).
#[test]
fn a_watcher_from_before_is_dropped() {
    use crate::repo::RepoTab;
    use crate::repo::messages::Msg;
    let ctx = egui::Context::default();
    let mut tab = RepoTab::new_deferred("/nonexistent/lazy-watcher".into(), &ctx);
    tab.start_deferred(&ctx);
    let first = tab.watches;
    tab.life.shown_at = Instant::now() - IDLE_AFTER * 2;
    tab.loading = false;
    tab.background_tick();
    // Asleep: nothing is kept.
    tab.handle(Msg::Watcher(first, None), &ctx);
    assert!(tab.watcher.is_none());
    tab.start_deferred(&ctx);
    assert_eq!(tab.watches, first + 1);
    // The old one's arrival does not count as the new one's (no recheck asked for).
    tab.recheck_after_load = false;
    tab.handle(Msg::Watcher(first, None), &ctx);
    assert!(!tab.recheck_after_load);
}
