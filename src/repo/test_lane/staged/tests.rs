use super::*;
use crate::git::fixture::Repo;

#[test]
fn closing_state_stops_its_run_only() {
    let owned = RunStop::default();
    let unrelated = RunStop::default();
    let state = Staged {
        stop: Some(owned.clone()),
        ..Default::default()
    };
    drop(state);
    assert!(owned.stopped());
    assert!(!unrelated.stopped());
}

#[test]
fn staged_panel_smoke_has_accessible_command_and_button_() -> Result<(), String> {
    let repo = Repo::new("staged-ui")?;
    let ctx = egui::Context::default();
    let mut tab = RepoTab::new(repo.path().to_path_buf(), &ctx);
    tab.test_lane.staged.initialized = true;
    tab.test_lane.staged.command = "exit 0".into();
    {
        let mut acts = Vec::new();
        let _ = ctx.run_ui(Default::default(), |ui| tab.staged_box(ui, &mut acts));
        assert!(acts.is_empty());
    }
    Ok(())
}
