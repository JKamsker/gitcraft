//! The window driven the way the tab drives it, on a repository with agent worktrees.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crate::git;
use crate::repo::RepoTab;

fn run(dir: &Path, args: &[&str]) -> Result<String, String> {
    git::run(Some(dir), args)
}

fn repo(name: &str) -> Result<PathBuf, String> {
    let dir = std::env::temp_dir().join(format!("gitcraft-cview-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_dir_all(dir.with_extension("wt"));
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    for args in [
        &["init", "-q", "-b", "main"][..],
        &["config", "user.name", "t"],
        &["config", "user.email", "t@t"],
        &["config", "commit.gpgSign", "false"],
    ] {
        run(&dir, args)?;
    }
    std::fs::write(dir.join("a.txt"), "1\n2\n3\n").map_err(|e| e.to_string())?;
    run(&dir, &["add", "-A"])?;
    run(&dir, &["commit", "-qm", "base"])?;
    Ok(dir)
}

/// A worktree on a new branch `name` that sets `a.txt` to `text`.
fn agent(dir: &Path, name: &str, text: &str) -> Result<(), String> {
    let wt = dir.with_extension("wt").join(name.replace('/', "-"));
    let wt_s = wt.to_string_lossy().into_owned();
    run(dir, &["worktree", "add", "-q", "-b", name, &wt_s, "main"])?;
    std::fs::write(wt.join("a.txt"), text).map_err(|e| e.to_string())?;
    run(&wt, &["commit", "-qam", name])?;
    Ok(())
}

#[test]
fn worktree_branches_are_ticked_and_checked_in_the_background() -> Result<(), String> {
    let dir = repo("tab")?;
    agent(&dir, "agent/one", "1\nONE\n3\n")?;
    agent(&dir, "agent/two", "1\nTWO\n3\n")?;
    run(&dir, &["branch", "unrelated-work"])?;
    let ctx = egui::Context::default();
    let mut tab = RepoTab::new_deferred(dir.clone(), &ctx);
    tab.refs = git::refs(&dir)?;
    tab.worktrees = git::worktrees(&dir, None)?;
    tab.open_collisions();
    let view = tab.collisions.as_ref().ok_or("the window opened")?;
    assert_eq!(view.target, "main");
    assert_eq!(view.chosen(), vec!["agent/one", "agent/two"]);
    assert!(
        view.picks
            .iter()
            .any(|(b, on)| b == "unrelated-work" && !on)
    );

    tab.check_collisions(&ctx);
    let started = Instant::now();
    loop {
        tab.poll(&ctx);
        if tab.collisions.as_ref().is_some_and(|v| v.result.is_some()) {
            break;
        }
        if started.elapsed() > Duration::from_secs(30) {
            return Err("the matrix never came back".into());
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    let view = tab.collisions.as_ref().ok_or("still open")?;
    let Some(Ok((m, order))) = &view.result else {
        return Err(format!(
            "{:?}",
            view.result.as_ref().map(|r| r.as_ref().err())
        ));
    };
    assert_eq!(m.branches, vec!["agent/one", "agent/two"]);
    assert_eq!(m.target, "main");
    assert!(m.conflict(0, 1));
    assert_eq!(order.len(), 2);
    assert!(order[0].clean());
    assert_eq!(
        super::grid::step_note(m, &order[1]),
        format!("conflicts with {} (1 file)", m.branches[order[0].branch])
    );
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_dir_all(dir.with_extension("wt"));
    Ok(())
}

#[test]
fn the_target_can_be_changed_and_drops_out_of_the_picks() {
    let names: Vec<String> = ["a", "b", "main"].iter().map(|s| (*s).to_owned()).collect();
    let view = super::CollisionView::new(&names, &["a".to_owned()], "main");
    assert_eq!(view.picks.len(), 2);
    assert_eq!(view.chosen(), vec!["a"]);
}
