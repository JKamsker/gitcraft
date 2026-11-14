//! Registry lifetime, terminal counts, and primary-worktree restore exclusion.
use super::presence::Presence;

#[test]
fn open_views_and_terminals_block_until_the_last_view_drops() {
    let path = std::env::temp_dir().join("presence-224-unique");
    let other = Presence::new(&path.with_extension("other"));
    assert!(other.others(&path).is_empty());
    let mut view = Presence::new(&path);
    view.terminals(vec![path.clone(), path.clone()]);
    let reasons = other.others(&path);
    assert!(reasons.iter().any(|s| s.contains("tab")));
    assert!(reasons.iter().any(|s| s.contains('2')));
    assert!(view.others(&path).is_empty());
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
    view.terminals(vec![cwd.clone()]);
    assert!(!Presence::using(&cwd, None).is_empty());
    drop(view);
    assert!(Presence::using(&cwd, None).is_empty());
}
