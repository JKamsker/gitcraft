use super::*;
use crate::git::fixture::Repo;

#[test]
fn closing_state_stops_its_run_only() {
    let owned = RunStop::default();
    let unrelated = RunStop::default();
    let mut state = Staged::default();
    state.stop.replace(owned.clone());
    drop(state);
    assert!(owned.stopped());
    assert!(!unrelated.stopped());
}

#[test]
fn staged_panel_exposes_named_command_and_test_button() -> Result<(), String> {
    use egui_kittest::{Harness, kittest::Queryable};
    let repo = Repo::new("staged-ui")?;
    let ctx = egui::Context::default();
    let mut tab = RepoTab::new(repo.path().to_path_buf(), &ctx);
    tab.test_lane.staged.initialized = true;
    tab.test_lane.staged.command = "exit 0".into();
    let mut harness = Harness::new_ui_state(
        |ui, tab: &mut RepoTab| {
            tab.staged_box(ui, &mut Vec::new());
        },
        tab,
    );
    harness.step();
    harness.get_by_role_and_label(
        egui::accesskit::Role::Button,
        crate::i18n::text("staged-run"),
    );
    harness.get_by_role_and_label(
        egui::accesskit::Role::TextInput,
        crate::i18n::text("staged-command"),
    );
    Ok(())
}

#[test]
fn restaging_invalidates_verdict_and_late_checks_cannot_clear_it() -> Result<(), String> {
    let repo = Repo::new("staged-ui-freshness")?;
    repo.commit("base", &[("file", Some("base"))])?;
    let result = git::test_staged(repo.path(), "exit 0", &RunStop::default())?;
    let original = result.key.fingerprint.clone();
    let mut state = Staged::default();
    state.result.replace(Ok(result));
    state.finish_check(0, 0, Ok(original.clone()));
    assert!(!state.stale);
    std::fs::write(repo.path().join("file"), "restaged").map_err(|e| e.to_string())?;
    repo.git(&["add", "file"])?;
    state.finish_check(0, 0, git::staged_fingerprint(repo.path(), "exit 0"));
    assert!(state.stale);
    state.freshness += 1;
    state.finish_check(0, 0, Ok(original.clone()));
    assert!(
        state.stale,
        "an invalidated read must not make a verdict fresh"
    );
    state.generation += 1;
    state.finish_check(0, 1, Ok(original));
    assert!(state.stale, "an older run must not overwrite a newer run");
    Ok(())
}

#[test]
fn watcher_events_invalidate_before_debounce_and_reject_older_freshness() -> Result<(), String> {
    use crate::repo::watch::FsChange;
    let repo = Repo::new("staged-review-watcher")?;
    repo.commit("base", &[])?;
    let ctx = egui::Context::default();
    for change in [FsChange::Index, FsChange::Git] {
        let mut tab = RepoTab::new_deferred(repo.path().into(), &ctx);
        let result = git::test_staged(repo.path(), "exit 0", &RunStop::default())?;
        let fingerprint = result.key.fingerprint.clone();
        tab.test_lane.staged.result = Some(Ok(result));
        tab.test_lane
            .staged
            .finish_check(0, 0, Ok(fingerprint.clone()));
        assert!(!tab.test_lane.staged.stale);
        let (tx, rx) = std::sync::mpsc::channel();
        tab.fs_rx = Some(rx);
        tx.send(change).map_err(|e| e.to_string())?;
        tab.poll(&ctx);
        assert!(tab.test_lane.staged.stale);
        assert!(tab.test_lane.staged.freshness > 0);
        tab.test_lane.staged.finish_check(0, 0, Ok(fingerprint));
        assert!(tab.test_lane.staged.stale);
    }
    Ok(())
}

#[test]
fn compact_entry_opens_a_keyboard_accessible_command_popup() -> Result<(), String> {
    use egui_kittest::{Harness, kittest::Queryable};
    let ctx = egui::Context::default();
    let mut tab = RepoTab::new_deferred("/nonexistent/staged-popup".into(), &ctx);
    tab.test_lane.staged.initialized = true;
    tab.test_lane.staged.command = "exit 0".into();
    let mut h = Harness::new_ui_state(
        |ui, (tab, acts): &mut (RepoTab, Vec<Action>)| tab.staged_entry(ui, acts),
        (tab, Vec::new()),
    );
    h.step();
    h.key_press(egui::Key::Tab);
    h.step();
    assert!(
        h.get_by_label(crate::i18n::text("staged-test"))
            .is_focused()
    );
    h.key_press(egui::Key::Enter);
    for _ in 0..3 {
        h.step();
    }
    h.get_by_role_and_label(
        egui::accesskit::Role::TextInput,
        crate::i18n::text("staged-command"),
    );
    h.get_by_role_and_label(
        egui::accesskit::Role::Button,
        crate::i18n::text("staged-run"),
    )
    .click();
    h.step();
    assert!(
        h.state()
            .1
            .iter()
            .any(|a| matches!(a, Action::TestLane(LaneAct::StagedStart)))
    );
    Ok(())
}

#[test]
fn watch_dependency_rebuild_keeps_verdict_unverified_until_registration() -> Result<(), String> {
    let repo = Repo::new("staged-watch-rebuild")?;
    repo.commit("base", &[])?;
    let ctx = egui::Context::default();
    let mut tab = RepoTab::new_deferred(repo.path().into(), &ctx);
    let result = git::test_staged(repo.path(), "exit 0", &RunStop::default())?;
    let fingerprint = result.key.fingerprint.clone();
    tab.test_lane.staged.result = Some(Ok(result));
    tab.test_lane
        .staged
        .finish_check(0, 0, Ok(fingerprint.clone()));
    tab.staged_watching(true, &ctx);
    assert!(tab.test_lane.staged.watching);
    assert!(tab.test_lane.staged.stale);
    assert!(!tab.test_lane.staged.checking);
    let epoch = tab.test_lane.staged.freshness;
    tab.staged_watching(false, &ctx);
    assert!(!tab.test_lane.staged.watching);
    assert!(tab.test_lane.staged.freshness > epoch);
    tab.test_lane.staged.finish_check(0, epoch, Ok(fingerprint));
    assert!(tab.test_lane.staged.stale);
    Ok(())
}

#[test]
fn checkout_variants_invalidate_before_any_work_is_dispatched() {
    let ctx = egui::Context::default();
    let mut tab = RepoTab::new_deferred("/nonexistent/staged-checkout".into(), &ctx);
    let actions = [
        Action::Checkout("main".into()),
        Action::CheckoutPull("main".into()),
        Action::CheckoutRemote("origin/main".into()),
        Action::CarryCheckout(Box::new(crate::repo::checkout_blocked::BlockedCheckout {
            label: "carry".into(),
            blockers: Default::default(),
            ops: Vec::new(),
            undo: crate::repo::Undo::None,
        })),
    ];
    for action in actions {
        tab.test_lane.staged.stale = false;
        let before = tab.test_lane.staged.freshness;
        tab.staged_before_action(&action, &ctx);
        assert!(tab.test_lane.staged.stale);
        assert!(tab.test_lane.staged.freshness > before);
        assert!(tab.busy_op.is_none());
    }
}

#[test]
fn timeout_panel_shows_failure_and_retained_output_without_cancel_label() -> Result<(), String> {
    use egui_kittest::{Harness, kittest::Queryable};
    let repo = Repo::new("staged-timeout-panel")?;
    repo.commit("base", &[])?;
    let ctx = egui::Context::default();
    let mut tab = RepoTab::new_deferred(repo.path().into(), &ctx);
    let mut result = git::test_staged(repo.path(), "exit 0", &RunStop::default())?;
    result.ended = git::Ended {
        code: None,
        stopped: true,
    };
    result.error = Some("diagnostic timeout".into());
    result.lines = vec!["compiler diagnostic retained".into()];
    tab.test_lane.staged.result = Some(Ok(result));
    tab.test_lane.staged.initialized = true;
    let mut h = Harness::new_ui_state(
        |ui, tab: &mut RepoTab| tab.staged_box(ui, &mut Vec::new()),
        tab,
    );
    h.step();
    assert!(h.query_by_label("diagnostic timeout").is_some());
    assert!(
        h.query_by_label(crate::i18n::text("staged-cancelled"))
            .is_none()
    );
    h.get_by_label(crate::i18n::text("staged-output")).click();
    for _ in 0..3 {
        h.step();
    }
    assert!(h.query_by_label("compiler diagnostic retained").is_some());
    Ok(())
}
