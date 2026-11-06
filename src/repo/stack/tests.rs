//! Stacks through the tab, as the views drive them: the dialog's suggestion, recording a
//! parent, undoing and redoing it, and what is refused.

use std::time::{Duration, Instant};

use super::StackAct;
use crate::git::stack::{self, test_repo::TestRepo};
use crate::repo::{Action, RepoTab};

/// Takes in what the tab's background work sends until `done` holds and it is idle.
pub(crate) fn settle(
    tab: &mut RepoTab,
    ctx: &egui::Context,
    done: impl Fn(&RepoTab) -> bool,
) -> Result<(), String> {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        tab.poll(ctx);
        if done(tab) && !tab.is_busy() {
            return Ok(());
        }
        if Instant::now() > deadline {
            return Err(format!("the tab never settled: {:?}", tab.status));
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// A tab on `repo`, its first read in.
pub(crate) fn open(repo: &TestRepo, ctx: &egui::Context) -> Result<RepoTab, String> {
    let mut tab = RepoTab::new(repo.path().to_path_buf(), ctx);
    settle(&mut tab, ctx, |t| !t.opening())?;
    Ok(tab)
}

#[test]
fn based_on_is_suggested_recorded_undone_and_redone() -> Result<(), String> {
    let repo = TestRepo::new("tab based on")?;
    repo.branch("schema", "main")?;
    repo.commit("s.txt", "s\n", "schema")?;
    repo.branch("api", "schema")?;
    repo.commit("a.txt", "a\n", "api")?;
    let ctx = egui::Context::default();
    let mut tab = open(&repo, &ctx)?;
    assert!(tab.stacks.layers.is_empty());

    tab.apply(Action::Stack(StackAct::OpenBasedOn("api".into())), &ctx);
    settle(&mut tab, &ctx, |t| {
        t.stacks
            .dialog
            .as_ref()
            .is_some_and(|d| d.suggestion.is_some())
    })?;
    let choice = tab.stacks.dialog.as_ref().map(|d| d.choice.clone());
    assert_eq!(choice.as_deref(), Some("schema"));

    tab.apply(
        Action::Stack(StackAct::SetParent {
            branch: "api".into(),
            parent: "schema".into(),
        }),
        &ctx,
    );
    assert!(tab.stacks.dialog.is_none());
    settle(&mut tab, &ctx, |t| t.stacks.layer("api").is_some())?;
    let recorded = stack::recorded(repo.path())?;
    let api = recorded.get("api").ok_or("api has no parent")?;
    assert_eq!(api.parent, "schema");
    assert_eq!(api.base.as_deref(), Some(repo.tip("schema")?.as_str()));
    assert_eq!(
        tab.stack_line("api").as_deref(),
        Some("Stack: schema → api")
    );

    tab.apply(Action::Undo, &ctx);
    settle(&mut tab, &ctx, |t| t.stacks.layer("api").is_none())?;
    assert!(stack::recorded(repo.path())?.is_empty());
    tab.apply(Action::Redo, &ctx);
    settle(&mut tab, &ctx, |t| t.stacks.layer("api").is_some())?;

    // schema on api would be a loop: refused, nothing recorded.
    tab.apply(
        Action::Stack(StackAct::SetParent {
            branch: "schema".into(),
            parent: "api".into(),
        }),
        &ctx,
    );
    settle(&mut tab, &ctx, |_| true)?;
    assert!(
        tab.status
            .as_ref()
            .is_some_and(|(s, failed)| *failed && s.contains("based on it"))
    );
    assert!(!stack::recorded(repo.path())?.contains_key("schema"));

    tab.apply(Action::Stack(StackAct::RemoveParent("api".into())), &ctx);
    settle(&mut tab, &ctx, |t| t.stacks.layer("api").is_none())?;
    assert!(stack::recorded(repo.path())?.is_empty());
    Ok(())
}

#[test]
fn branches_without_history_in_common_are_refused() -> Result<(), String> {
    let repo = TestRepo::new("tab unrelated")?;
    repo.git(&["checkout", "-q", "--orphan", "lonely"])?;
    repo.git(&["rm", "-rfq", "."])?;
    repo.commit("l.txt", "l\n", "lonely")?;
    let ctx = egui::Context::default();
    let mut tab = open(&repo, &ctx)?;
    tab.apply(
        Action::Stack(StackAct::SetParent {
            branch: "lonely".into(),
            parent: "main".into(),
        }),
        &ctx,
    );
    settle(&mut tab, &ctx, |t| t.status.is_some())?;
    let status = tab.status.clone().unwrap_or_default();
    assert!(
        status.1 && status.0.contains("no commit in common"),
        "{status:?}"
    );
    assert!(stack::recorded(repo.path())?.is_empty());
    Ok(())
}
