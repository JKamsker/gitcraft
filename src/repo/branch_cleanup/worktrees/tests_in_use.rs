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
) -> std::sync::Arc<dyn Fn() -> Option<std::path::PathBuf> + Send + Sync> {
    std::sync::Arc::new(move || Some(path.clone()))
}

#[test]
fn changed_terminal_cwd_and_nested_directories_are_protected()
-> Result<(), Box<dyn std::error::Error>> {
    let root = std::env::temp_dir().join("224-cd-prefix");
    let cwd = std::sync::Arc::new(std::sync::Mutex::new(root.join("elsewhere")));
    let observed = cwd.clone();
    let mut view = Presence::new(&root.join("tab"));
    view.terminals(vec![std::sync::Arc::new(move || {
        observed.lock().ok().map(|p| p.clone())
    })]);
    assert!(Presence::using(&root.join("worktree"), None).is_empty());
    *cwd.lock().map_err(|e| e.to_string())? = root.join("worktree/nested/subdirectory");
    assert!(!Presence::using(&root.join("worktree"), None).is_empty());
    assert!(Presence::using(&root.join("work"), None).is_empty());
    Ok(())
}

#[test]
fn unavailable_terminal_cwd_blocks_instead_of_using_launch_path() {
    let root = std::env::temp_dir().join("224-unknown-cwd");
    let mut view = Presence::new(&root.join("tab"));
    view.terminals(vec![std::sync::Arc::new(|| None)]);
    assert!(!Presence::using(&root.join("worktree"), None).is_empty());
}
