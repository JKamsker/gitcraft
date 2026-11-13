//! Which worktrees the tab offers, with what evidence, and restoring archives by themselves,
//! on real repositories in temporary folders.

use std::path::PathBuf;

use super::jobs::auto_restore;
use super::{evidence, rows, window};
use crate::git;
use crate::git::stack::test_repo::TestRepo;
use crate::git::worktree_cleanup::archive::Store;
use crate::git::worktree_cleanup::{Blocker, Outcome, Proof, inspect, remove};
use crate::repo::RepoTab;
use crate::repo::branch_health::{self, Bases, Inputs, Merged};

/// A repository with `done` merged into `main` and `open` not, each in a worktree, and a
/// folder for those worktrees and the archives, removed when dropped.
struct Fixture {
    repo: TestRepo,
    scratch: PathBuf,
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.scratch);
    }
}

impl Fixture {
    fn new(name: &str) -> Result<Fixture, String> {
        let repo = TestRepo::new(&format!("tab wt cleanup {name}"))?;
        let scratch =
            std::env::temp_dir().join(format!("gitcraft tab wt {name} {}", std::process::id()));
        let _ = std::fs::remove_dir_all(&scratch);
        std::fs::create_dir_all(&scratch).map_err(|e| e.to_string())?;
        for (branch, merge) in [("done", true), ("open", false)] {
            repo.branch(branch, "main")?;
            repo.commit(&format!("{branch}.txt"), "work\n", branch)?;
            repo.checkout("main")?;
            if merge {
                repo.git(&["merge", "-q", "--no-ff", "-m", "merge", branch])?;
            }
            let folder = scratch.join(format!("wt {branch}"));
            repo.git(&["worktree", "add", "-q", &folder.to_string_lossy(), branch])?;
        }
        Ok(Fixture { repo, scratch })
    }

    fn wt(&self, branch: &str) -> PathBuf {
        self.scratch.join(format!("wt {branch}"))
    }

    fn store(&self) -> Store {
        Store {
            root: self.scratch.join("archives"),
            quota: 64 << 20,
        }
    }

    /// A tab on the repository with its refs, worktrees and the branch read done.
    fn tab(&self) -> RepoTab {
        let dir = self.repo.path();
        let ctx = egui::Context::default();
        let mut tab = RepoTab::new_deferred(dir.to_path_buf(), &ctx);
        tab.refs = git::refs(dir).unwrap_or_default();
        tab.worktrees = git::worktrees(dir, None).unwrap_or_default();
        let inputs = Inputs::new(&tab.refs.local, tab.health_bases(), &[]);
        let mut cache = branch_health::CherryCache::new();
        tab.branch_health.health = branch_health::read(dir, &inputs, &mut cache).ok();
        tab
    }
}

#[test]
fn only_worktrees_of_merged_branches_are_offered_with_their_basis() -> Result<(), String> {
    let f = Fixture::new("offered")?;
    let tab = f.tab();
    let offered = tab.merged_worktrees();
    assert_eq!(offered.len(), 1, "{offered:?}");
    let target = &offered[0];
    assert_eq!(target.branch, "done");
    assert_eq!(
        target.path.canonicalize().ok(),
        f.wt("done").canonicalize().ok()
    );
    assert_eq!(target.evidence.base_ref, "refs/heads/main");
    assert_eq!(target.evidence.proof, Proof::Ancestor);
    assert!(target.evidence.describe.contains("main"));
    let found = inspect(f.repo.path(), target)?;
    assert_eq!(found.blockers, []);
    // Nothing offered before the branch read is in.
    let mut unread = f.tab();
    unread.branch_health.health = None;
    assert!(unread.merged_worktrees().is_empty());
    Ok(())
}

#[test]
fn the_evidence_names_the_ref_it_was_found_in() {
    let bases = Bases {
        name: "dev".to_owned(),
        refs: vec![
            ("refs/remotes/origin/dev".to_owned(), "a".repeat(40)),
            ("refs/heads/dev".to_owned(), "b".repeat(40)),
        ],
    };
    let tip = "c".repeat(40);
    let of = |how: Merged| evidence(&how, &tip, &bases).map(|e| (e.base_ref, e.proof));
    assert_eq!(
        of(Merged::Into("origin/dev".to_owned())),
        Some(("refs/remotes/origin/dev".to_owned(), Proof::Ancestor))
    );
    assert_eq!(
        of(Merged::Into("dev".to_owned())),
        Some(("refs/heads/dev".to_owned(), Proof::Ancestor))
    );
    assert_eq!(
        of(Merged::Rebased("origin/dev".to_owned())),
        Some(("refs/remotes/origin/dev".to_owned(), Proof::PatchEquivalent))
    );
    assert_eq!(
        of(Merged::Pr(7, "dev".to_owned())),
        Some(("dev".to_owned(), Proof::PullRequest(7)))
    );
    assert_eq!(of(Merged::Into("main".to_owned())), None);
}

/// Archived at cleanup; restored once when the branch is checked out again at that commit;
/// never into a branch of the same name made again elsewhere.
#[test]
fn archives_come_back_by_themselves_only_for_the_same_branch_and_commit() -> Result<(), String> {
    let f = Fixture::new("auto")?;
    let store = f.store();
    let wt = f.wt("done");
    std::fs::write(wt.join("notes ✓.txt"), "kept\n").map_err(|e| e.to_string())?;
    let tab = f.tab();
    let target = tab.merged_worktrees().pop().ok_or("not offered")?;
    let preview = inspect(f.repo.path(), &target)?;
    assert_eq!(preview.blockers_with(false), [Blocker::Untracked(1)]);
    let outcome = remove(f.repo.path(), &target, &preview, Some(&store));
    assert!(
        matches!(
            outcome,
            Outcome::Removed {
                archive: Some(_),
                ..
            }
        ),
        "{outcome:?}"
    );
    let listed = |tab: &RepoTab| -> Vec<(PathBuf, String, String)> {
        tab.worktrees
            .iter()
            .filter_map(|w| Some((w.path.clone(), w.branch.clone()?, w.head.clone())))
            .collect()
    };
    // The same name, made again at another commit: nothing comes back.
    let tip = f.repo.tip("done")?;
    f.repo.git(&["branch", "-f", "done", "main"])?;
    let folder = wt.to_string_lossy().into_owned();
    f.repo.git(&["worktree", "add", "-q", &folder, "done"])?;
    assert!(auto_restore(f.repo.path(), &store, &listed(&f.tab())).is_empty());
    assert!(!wt.join("notes ✓.txt").exists());
    // At the commit it was archived from: restored, once.
    f.repo.git(&["worktree", "remove", &folder])?;
    f.repo.git(&["branch", "-f", "done", &tip])?;
    f.repo.git(&["worktree", "add", "-q", &folder, "done"])?;
    let done = auto_restore(f.repo.path(), &store, &listed(&f.tab()));
    assert_eq!(done.len(), 1);
    assert_eq!(
        std::fs::read_to_string(wt.join("notes ✓.txt"))
            .ok()
            .as_deref(),
        Some("kept\n")
    );
    assert!(auto_restore(f.repo.path(), &store, &listed(&f.tab())).is_empty());
    Ok(())
}

#[test]
fn outcomes_and_files_read_as_the_preview_says() {
    let kept = Outcome::Kept(vec![Blocker::Changed(2), Blocker::Primary]);
    let (text, bad) = window::describe(&kept);
    assert!(bad && text.contains("2 changed tracked files") && text.contains("main worktree"));
    let (text, bad) = window::describe(&Outcome::Changed);
    assert!(bad && text.contains("Look again"));
    let file = crate::git::worktree_cleanup::Untracked {
        path: "a b/ü.bin".to_owned(),
        size: 2048,
        kind: crate::git::worktree_cleanup::Kind::Binary,
    };
    assert_eq!(
        rows::untracked_line(&file),
        "?? a b/ü.bin (2.0 KB) · not archived: binary"
    );
}
