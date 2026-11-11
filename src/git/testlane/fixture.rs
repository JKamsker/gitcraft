//! Repositories for the tests of tests over history: real ones, in a temporary folder whose
//! name has spaces and letters beyond ASCII, removed when dropped.

use std::path::{Path, PathBuf};

use crate::git;

/// A temporary repository on `main`, gone when dropped.
pub(crate) struct Repo {
    pub(crate) dir: PathBuf,
}

impl Drop for Repo {
    fn drop(&mut self) {
        crate::repo::hang_diagnostics::run(format_args!("fixture remove directory"), || {
            let _ = std::fs::remove_dir_all(&self.dir);
        });
    }
}

impl Repo {
    pub(crate) fn new(name: &str) -> Result<Repo, String> {
        let dir =
            std::env::temp_dir().join(format!("gitcraft tëst-{name} ü-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let repo = Repo { dir };
        for args in [
            &["init", "-q", "-b", "main"][..],
            &["config", "user.name", "Tëster"],
            &["config", "user.email", "t@t"],
            &["config", "commit.gpgSign", "false"],
            &["config", "core.autocrlf", "false"],
        ] {
            repo.git(args)?;
        }
        Ok(repo)
    }

    pub(crate) fn path(&self) -> &Path {
        &self.dir
    }

    pub(crate) fn git(&self, args: &[&str]) -> Result<String, String> {
        crate::repo::hang_diagnostics::run(format_args!("fixture git {args:?}"), || {
            git::run(Some(&self.dir), args)
        })
    }

    /// Writes `files` (path, text; `None` removes it) and commits them as `message`: the
    /// new commit.
    pub(crate) fn commit(
        &self,
        message: &str,
        files: &[(&str, Option<&str>)],
    ) -> Result<String, String> {
        for (path, text) in files {
            let file = self.dir.join(path);
            match text {
                Some(text) => {
                    if let Some(parent) = file.parent() {
                        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
                    }
                    std::fs::write(&file, text).map_err(|e| e.to_string())?;
                }
                None => std::fs::remove_file(&file).map_err(|e| e.to_string())?,
            }
        }
        self.git(&["add", "-A"])?;
        self.git(&["commit", "-q", "--allow-empty", "-m", message])?;
        self.rev("HEAD")
    }

    pub(crate) fn rev(&self, rev: &str) -> Result<String, String> {
        self.git(&["rev-parse", "--verify", rev])
            .map(|s| s.trim().to_owned())
    }

    /// The tree of `rev`.
    pub(crate) fn tree(&self, rev: &str) -> Result<String, String> {
        self.rev(&format!("{rev}^{{tree}}"))
    }

    /// The worktrees git lists, its scratch ones too.
    pub(crate) fn worktree_count(&self) -> Result<usize, String> {
        Ok(self
            .git(&["worktree", "list", "--porcelain"])?
            .lines()
            .filter(|l| l.starts_with("worktree "))
            .count())
    }

    /// What is left in the repository's scratch folder.
    pub(crate) fn scratch_left(&self) -> usize {
        git::scratch_root(&self.dir)
            .ok()
            .and_then(|root| std::fs::read_dir(root).ok())
            .map(|d| d.count())
            .unwrap_or(0)
    }
}
