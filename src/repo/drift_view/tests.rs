//! The window driven as the tab drives it.

use std::time::{Duration, Instant};

use crate::drift::tests::{agent_worktree, repo};
use crate::git;
use crate::repo::{Action, RepoTab};

#[test]
fn opened_from_the_worktree_it_reads_the_task_and_measures() -> Result<(), String> {
    let dir = repo("view")?;
    let wt = agent_worktree(&dir, "agent/login")?;
    crate::drift::task::write(&wt, "Fix the login")?;
    let ctx = egui::Context::default();
    let mut tab = RepoTab::new_deferred(dir.clone(), &ctx);
    tab.refs = git::refs(&dir)?;
    tab.worktrees = git::worktrees(&dir, None)?;
    let path = tab
        .worktrees
        .iter()
        .find(|w| !w.main)
        .map(|w| w.path.clone())
        .ok_or("worktree")?;
    tab.apply(Action::OpenDrift(path.clone()), &ctx);
    let started = Instant::now();
    while tab.drift.as_ref().is_some_and(|v| v.busy) {
        tab.poll(&ctx);
        if started.elapsed() > Duration::from_secs(30) {
            return Err("never measured".into());
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    let view = tab.drift.as_ref().ok_or("open")?;
    assert_eq!(view.task, "Fix the login");
    let Some(Ok(report)) = &view.report else {
        return Err(format!("{:?}", view.report));
    };
    assert!(report.judged);
    assert!(report.unrelated().any(|f| f.path == "docs"));
    // A file of another worktree's change opens at its HEAD, not this tab's working tree.
    let key = view.diff_key(report, "src/auth/login.rs");
    assert!(matches!(
        key,
        Some(crate::repo::DiffKey::Range { to: Some(_), .. })
    ));
    // A change only that worktree's working tree has opens the worktree instead.
    assert!(view.diff_key(report, "notes.txt").is_none());
    assert!(view.diff_key(report, "docs/guide ü.md").is_none());
    // Put away and brought back as it was.
    if let Some(v) = &mut tab.drift {
        v.hidden = true;
    }
    tab.apply(Action::OpenDrift(path), &ctx);
    assert!(
        tab.drift
            .as_ref()
            .is_some_and(|v| !v.hidden && v.report.is_some())
    );
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_dir_all(dir.with_extension("wt"));
    Ok(())
}
