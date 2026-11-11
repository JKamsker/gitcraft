//! The fence as the tab uses it, on a repository with an agent worktree.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crate::git;
use crate::repo::actions::BranchRef;
use crate::repo::{Action, Dialog, RepoTab, merge_action};

fn run(dir: &Path, args: &[&str]) -> Result<String, String> {
    git::run(Some(dir), args)
}

fn repo(name: &str) -> Result<PathBuf, String> {
    let dir = std::env::temp_dir().join(format!("gitcraft-fview-{name}-{}", std::process::id()));
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
    std::fs::write(dir.join("Cargo.lock"), "v1\n").map_err(|e| e.to_string())?;
    run(&dir, &["add", "-A"])?;
    run(&dir, &["commit", "-qm", "base"])?;
    Ok(dir)
}

/// Polls the tab until `done` holds of it.
fn until(
    tab: &mut RepoTab,
    ctx: &egui::Context,
    done: impl Fn(&RepoTab) -> bool,
) -> Result<(), String> {
    let started = Instant::now();
    while !done(tab) {
        tab.poll(ctx);
        if started.elapsed() > Duration::from_secs(30) {
            return Err("timed out".into());
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    Ok(())
}

#[test]
fn a_worktree_touching_the_fence_turns_red_and_its_merge_asks_first() -> Result<(), String> {
    let dir = repo("gate")?;
    let wt = dir.with_extension("wt").join("agent");
    run(
        &dir,
        &[
            "worktree",
            "add",
            "-q",
            "-b",
            "agent/lock",
            &wt.to_string_lossy(),
            "main",
        ],
    )?;
    std::fs::write(wt.join("Cargo.lock"), "v2\n").map_err(|e| e.to_string())?;
    run(&wt, &["commit", "-qam", "bump"])?;
    crate::fence::write(&dir, &["Cargo.lock".to_owned()])?;

    let ctx = egui::Context::default();
    let mut tab = RepoTab::new_deferred(dir.clone(), &ctx);
    tab.refs = git::refs(&dir)?;
    tab.worktrees = git::worktrees(&dir, None)?;
    tab.check_fence(&ctx);
    until(&mut tab, &ctx, |t| !t.fence.checking)?;
    assert_eq!(tab.fence.list, vec!["Cargo.lock"]);
    let wt_path = tab
        .worktrees
        .iter()
        .find(|w| !w.main)
        .map(|w| w.path.clone())
        .ok_or("the linked worktree")?;
    let found = tab.fence.violations(&wt_path);
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].path, "Cargo.lock");

    // Merging the branch is checked in the background, then asks.
    tab.apply(merge_action(BranchRef::local("agent/lock"), "main"), &ctx);
    until(&mut tab, &ctx, |t| t.dialog.is_some())?;
    let Some(Dialog::Confirm {
        title, body, ops, ..
    }) = tab.dialog.take()
    else {
        return Err("a confirmation".into());
    };
    assert!(title.ends_with("(past the agent fence)"), "{title}");
    assert!(
        body.contains("Cargo.lock (committed; fence: Cargo.lock)"),
        "{body}"
    );
    // Confirmed, it goes through the gate.
    let confirmed = Action::UndoableOps {
        label: title,
        ops,
        refresh: crate::repo::Refresh::All,
        undo: crate::repo::Undo::None,
    };
    assert!(matches!(
        tab.fence_gate(confirmed, &ctx),
        Some(Action::UndoableOps { .. })
    ));
    // The same branch once its worktree is gone: its commits still ask.
    tab.worktrees.retain(|w| w.main);
    assert!(
        tab.fence_gate(merge_action(BranchRef::local("agent/lock"), "main"), &ctx)
            .is_none()
    );
    until(&mut tab, &ctx, |t| t.dialog.is_some())?;
    tab.dialog = None;
    tab.worktrees = git::worktrees(&dir, None)?;
    // A branch that touched nothing: checked, then run with no question.
    run(&dir, &["branch", "other"])?;
    assert!(
        tab.fence_gate(merge_action(BranchRef::local("other"), "main"), &ctx)
            .is_none()
    );
    until(&mut tab, &ctx, |t| {
        t.busy_op.is_some() || t.status.is_none()
    })?;
    assert!(tab.dialog.is_none());
    // A worktree that couldn't be checked is red too.
    tab.fence.found.insert(wt_path.clone(), Err("lost".into()));
    assert!(tab.fence.red(&wt_path));
    tab.check_fence(&ctx);
    until(&mut tab, &ctx, |t| !t.fence.checking)?;

    // The list emptied: the worktree is clean again.
    crate::fence::write(&dir, &[])?;
    tab.check_fence(&ctx);
    until(&mut tab, &ctx, |t| !t.fence.checking)?;
    assert!(tab.fence.violations(&wt_path).is_empty() && !tab.fence.red(&wt_path));
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_dir_all(dir.with_extension("wt"));
    Ok(())
}

#[test]
fn saving_writes_every_worktrees_agent_configs() -> Result<(), String> {
    let dir = repo("save")?;
    let wt = dir.with_extension("wt").join("agent two");
    run(
        &dir,
        &[
            "worktree",
            "add",
            "-q",
            "-b",
            "agent/two",
            &wt.to_string_lossy(),
            "main",
        ],
    )?;
    let entries = vec![".env".to_owned(), "secrets/".to_owned()];
    let (said, error) = super::editor::save_for_test(&dir, &entries, &[dir.clone(), wt.clone()]);
    assert!(!error, "{said}");
    assert_eq!(
        said,
        "Saved 2 entries; 6 agent config files written in 2 worktrees."
    );
    assert_eq!(crate::fence::read(&dir)?, entries);
    // The linked worktree reads the same list (the config is shared).
    assert_eq!(crate::fence::read(&wt)?, entries);
    for root in [&dir, &wt] {
        let cursor =
            std::fs::read_to_string(root.join(".cursorignore")).map_err(|e| e.to_string())?;
        assert!(cursor.contains("secrets/"));
    }
    // A worktree folder gone: said, the rest still written.
    let (said, error) = super::editor::save_for_test(&dir, &entries, &[dir.join("gone/away")]);
    assert!(
        error && said.starts_with("Saved, but not everywhere"),
        "{said}"
    );
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_dir_all(dir.with_extension("wt"));
    Ok(())
}
