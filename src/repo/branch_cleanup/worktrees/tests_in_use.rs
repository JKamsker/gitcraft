//! Registry lifetime, terminal counts, and primary-worktree restore exclusion.
use super::presence::Presence;

#[test]
fn open_views_and_terminals_block_until_the_last_view_drops() {
    let path = std::env::temp_dir().join("presence-224-unique");
    let other = Presence::new(&path.with_extension("other"));
    assert!(other.others(&path).is_empty());
    let mut view = Presence::new(&path);
    view.terminals(vec![probe(path.clone()), probe(path.clone())]);
    let reasons = other.others(&path);
    assert!(reasons.iter().any(|s| s.contains("tab")));
    assert!(reasons.iter().any(|s| s.contains('2')));
    assert!(
        view.others(&path)
            .iter()
            .all(|reason| !reason.contains("tab"))
    );
    drop(view);
    assert!(other.others(&path).is_empty());
}

#[test]
fn auto_restore_excludes_primary_worktrees() -> Result<(), String> {
    let repo = crate::git::stack::test_repo::TestRepo::new("224 restore primary")?;
    let trees = crate::git::worktrees(repo.path(), None)?;
    assert!(trees.iter().any(|w| w.main));
    assert!(super::jobs::restore_targets(&trees).is_empty());
    Ok(())
}

#[test]
fn terminal_uses_its_own_worktree_even_when_its_tab_is_elsewhere() {
    let root = std::env::temp_dir().join("224-terminal-elsewhere");
    let mut view = Presence::new(&root.join("tab"));
    let cwd = root.join("terminal");
    view.terminals(vec![probe(cwd.clone())]);
    assert!(!Presence::using(&cwd, None).is_empty());
    assert!(!view.others(&cwd).is_empty());
    drop(view);
    assert!(Presence::using(&cwd, None).is_empty());
}

fn probe(
    path: std::path::PathBuf,
) -> std::sync::Arc<dyn Fn() -> Option<Vec<std::path::PathBuf>> + Send + Sync> {
    std::sync::Arc::new(move || Some(vec![path.clone()]))
}

#[test]
fn changed_terminal_cwd_and_nested_directories_are_protected()
-> Result<(), Box<dyn std::error::Error>> {
    let root = std::env::temp_dir().join("224-cd-prefix");
    let cwd = std::sync::Arc::new(std::sync::Mutex::new(root.join("elsewhere")));
    let observed = cwd.clone();
    let mut view = Presence::new(&root.join("tab"));
    view.terminals(vec![std::sync::Arc::new(move || {
        observed.lock().ok().map(|p| vec![p.clone()])
    })]);
    assert!(Presence::using(&root.join("worktree"), None).is_empty());
    *cwd.lock().map_err(|e| e.to_string())? = root.join("worktree/nested/subdirectory");
    assert!(!Presence::using(&root.join("worktree"), None).is_empty());
    assert!(Presence::using(&root.join("work"), None).is_empty());
    Ok(())
}

#[test]
fn one_terminal_protects_both_shell_and_foreground_worktrees() {
    let root = std::env::temp_dir().join("224-divergent-terminal");
    let mut view = Presence::new(&root.join("tab"));
    let shell = root.join("shell/nested");
    let active = root.join("foreground/nested");
    view.terminals(vec![std::sync::Arc::new(move || {
        Some(vec![shell.clone(), active.clone()])
    })]);
    for name in ["shell", "foreground"] {
        let reasons = view.others(&root.join(name));
        assert!(reasons.iter().any(|why| why.contains('1')));
    }
}

/// A real cleanup result so purge tests exercise validation and the final observer callback.
pub(super) fn retained(
    name: &str,
) -> Result<(crate::git::stack::test_repo::TestRepo, std::path::PathBuf), String> {
    use crate::git::worktree_cleanup::archive::Store;
    use crate::git::worktree_cleanup::{Evidence, Outcome, Proof, Target, inspect, remove};
    let repo = crate::git::stack::test_repo::TestRepo::new(name)?;
    repo.git(&["branch", "done", "main"])?;
    let path = repo.path().join("linked");
    repo.git(&["worktree", "add", "-q", &path.to_string_lossy(), "done"])?;
    let target = Target {
        path,
        branch: "done".into(),
        evidence: Evidence {
            base_ref: "refs/heads/main".into(),
            proof: Proof::Ancestor,
            tip: repo.tip("main")?,
            describe: "merged".into(),
        },
        in_use: None,
    };
    let preview = inspect(repo.path(), &target)?;
    let outcome = remove(repo.path(), &target, &preview, None, false);
    assert!(matches!(outcome, Outcome::Removed { .. }), "{outcome:?}");
    let entry = Store::recoveries(repo.path())?
        .into_iter()
        .next()
        .ok_or("Missing recovery")?
        .0;
    Ok((repo, entry))
}

#[test]
fn purge_rechecks_a_terminal_inside_the_retained_tree() -> Result<(), String> {
    use crate::git::worktree_cleanup::archive::Store;
    let (repo, entry) = retained("224 purge active terminal")?;
    let nested = entry.join("tree/nested");
    std::fs::create_dir(&nested).map_err(|e| e.to_string())?;
    let mut view = Presence::new(repo.path());
    view.terminals(vec![probe(nested)]);
    assert!(
        Store::purge_recovery(repo.path(), &entry, |path| super::jobs::current_use(
            repo.path(),
            path
        ))
        .is_err()
    );
    assert!(entry.join("tree/base.txt").is_file());
    drop(view);
    Store::purge_recovery(repo.path(), &entry, |path| {
        super::jobs::current_use(repo.path(), path)
    })?;
    assert!(!entry.exists());
    Ok(())
}

#[test]
fn purge_rechecks_nested_tabs_and_agent_tasks() -> Result<(), String> {
    use crate::git::worktree_cleanup::archive::Store;
    let (repo, entry) = retained("224 purge active tab and agent")?;
    let nested = entry.join("tree/nested");
    std::fs::create_dir(&nested).map_err(|e| e.to_string())?;
    let view = Presence::new(&nested);
    assert!(
        Store::purge_recovery(repo.path(), &entry, |path| super::jobs::current_use(
            repo.path(),
            path
        ))
        .is_err()
    );
    drop(view);
    crate::agent::tasks::add(
        repo.path(),
        crate::agent::tasks::AgentTask {
            path: nested.to_string_lossy().into_owned(),
            agent: "codex".into(),
            ..Default::default()
        },
    )?;
    assert!(
        Store::purge_recovery(repo.path(), &entry, |path| super::jobs::current_use(
            repo.path(),
            path
        ))
        .is_err()
    );
    assert!(entry.join("tree/base.txt").is_file());
    Ok(())
}
