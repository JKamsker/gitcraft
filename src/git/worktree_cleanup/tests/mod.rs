//! Worktree cleanup on real repositories in temporary folders (never the user's): merged and
//! unmerged branches, changed and untracked files, archives, restores and failures.

mod archive_tests;
mod inspect_tests;
mod remove_tests;
mod restore_tests;

use std::path::{Path, PathBuf};

use super::archive::Store;
use super::{Evidence, Proof, Target};
use crate::git::stack::test_repo::TestRepo;

/// A repository on `main` with a branch merged into it, checked out in a worktree beside it;
/// archives go to a folder of the fixture's own.
pub(super) struct Fixture {
    pub repo: TestRepo,
    /// The folder the worktrees and the archives are in, removed when dropped.
    pub scratch: PathBuf,
    pub store: Store,
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.scratch);
    }
}

impl Fixture {
    pub fn new(name: &str) -> Result<Fixture, String> {
        let repo = TestRepo::new(&format!("wt cleanup {name}"))?;
        let scratch = std::env::temp_dir().join(format!(
            "gitcraft wt-cleanup ü {name} {}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&scratch);
        std::fs::create_dir_all(&scratch).map_err(|e| e.to_string())?;
        let store = Store {
            root: scratch.join("archives"),
            quota: 64 << 20,
        };
        Ok(Fixture {
            repo,
            scratch,
            store,
        })
    }

    pub fn git(&self, dir: &Path, args: &[&str]) -> Result<String, String> {
        crate::git::run(Some(dir), args).map_err(|e| format!("git {args:?} in {dir:?}: {e}"))
    }

    /// `branch` made from `main` with a commit, merged into `main` (a merge commit), and
    /// checked out in a worktree of its own: its folder.
    pub fn merged_worktree(&self, branch: &str) -> Result<PathBuf, String> {
        let repo = &self.repo;
        repo.branch(branch, "main")?;
        repo.commit(&format!("{branch}.txt"), "work\n", branch)?;
        repo.checkout("main")?;
        repo.git(&["merge", "-q", "--no-ff", "-m", "merge", branch])?;
        self.add_worktree(branch)
    }

    /// `branch` with a commit `main` doesn't have, in a worktree.
    pub fn open_worktree(&self, branch: &str) -> Result<PathBuf, String> {
        let repo = &self.repo;
        repo.branch(branch, "main")?;
        repo.commit(&format!("{branch}.txt"), "open\n", branch)?;
        repo.checkout("main")?;
        self.add_worktree(branch)
    }

    pub fn add_worktree(&self, branch: &str) -> Result<PathBuf, String> {
        let path = self.scratch.join(format!("tree {branch}"));
        let folder = path.to_string_lossy().into_owned();
        self.repo.git(&["worktree", "add", "-q", &folder, branch])?;
        Ok(path)
    }

    /// The worktree of `branch` as offered, found merged into `main` at its tip now.
    pub fn target(&self, path: &Path, branch: &str) -> Result<Target, String> {
        Ok(Target {
            path: path.to_path_buf(),
            branch: branch.to_owned(),
            evidence: Evidence {
                base_ref: "refs/heads/main".to_owned(),
                proof: Proof::Ancestor,
                tip: self.repo.tip(&format!("refs/heads/{branch}"))?,
                describe: "merged into main".to_owned(),
            },
            in_use: None,
        })
    }
}

/// Writes `bytes` to `rel` under `root`, making its folders.
pub(super) fn write(root: &Path, rel: &str, bytes: &[u8]) -> Result<(), String> {
    let path = root.join(rel);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    std::fs::write(path, bytes).map_err(|e| e.to_string())
}

/// A name with spaces and letters outside ASCII, in a folder likewise.
pub(super) const UNICODE: &str = "notes ü/my file ✓ 日本.txt";
