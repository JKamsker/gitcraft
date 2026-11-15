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
    harness.get_by_label(crate::i18n::text("staged-test"));
    harness.get_by_label(crate::i18n::text("staged-command"));
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
