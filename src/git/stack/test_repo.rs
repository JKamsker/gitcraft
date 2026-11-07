//! A real git repository in a temporary folder, for the tests of stacks (and the views on
//! them): branches made of commits, through the same `git::run` the app uses.

use std::path::{Path, PathBuf};

use crate::git;

pub(crate) struct TestRepo {
    pub dir: PathBuf,
}

impl TestRepo {
    /// A new repository on `main` with one commit, in a folder of its own (`name` may hold
    /// spaces and letters outside ASCII, as users' folders do).
    pub fn new(name: &str) -> Result<TestRepo, String> {
        let dir =
            std::env::temp_dir().join(format!("gitcraft stack {name} {}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let repo = TestRepo { dir };
        for args in [
            &["init", "-q", "-b", "main"][..],
            &["config", "user.name", "t"],
            &["config", "user.email", "t@t"],
            &["config", "commit.gpgSign", "false"],
            &["config", "core.autocrlf", "false"],
        ] {
            repo.git(args)?;
        }
        repo.commit("base.txt", "base\n", "base")?;
        Ok(repo)
    }

    pub fn path(&self) -> &Path {
        &self.dir
    }

    /// Runs git, saying which command failed should it fail.
    pub fn git(&self, args: &[&str]) -> Result<String, String> {
        git::run(Some(&self.dir), args).map_err(|e| format!("git {args:?}: {e}"))
    }

    /// Writes `file` and commits it on the branch checked out.
    pub fn commit(&self, file: &str, text: &str, message: &str) -> Result<String, String> {
        let path = self.dir.join(file);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        std::fs::write(&path, text).map_err(|e| e.to_string())?;
        self.git(&["add", "--", file])?;
        self.git(&["commit", "-q", "-m", message])?;
        self.tip("HEAD")
    }

    /// Makes `branch` at `from` and checks it out.
    pub fn branch(&self, branch: &str, from: &str) -> Result<(), String> {
        self.git(&["checkout", "-q", "-b", branch, from]).map(drop)
    }

    pub fn checkout(&self, branch: &str) -> Result<(), String> {
        self.git(&["checkout", "-q", branch]).map(drop)
    }

    /// The commit `rev` is at.
    pub fn tip(&self, rev: &str) -> Result<String, String> {
        Ok(self.git(&["rev-parse", rev])?.trim().to_owned())
    }

    /// Runs the steps an operation is made of, as git commands, one after another.
    pub fn steps(&self, steps: &[Vec<String>]) -> Result<(), String> {
        for step in steps {
            let args: Vec<&str> = step.iter().map(String::as_str).collect();
            self.git(&args)?;
        }
        Ok(())
    }

    /// `main → a → b → c`, each layer one commit of its own on the one below (a file named
    /// after it), recorded as a stack. Leaves `main` checked out.
    pub fn stack(&self, layers: &[&str]) -> Result<(), String> {
        let mut parent = "main".to_owned();
        for layer in layers {
            self.branch(layer, &parent)?;
            self.commit(&format!("{layer}.txt"), &format!("{layer}\n"), layer)?;
            let base = self.tip(&parent)?;
            self.steps(&git::stack::set_parent_ops(layer, &parent, &base))?;
            parent = (*layer).to_owned();
        }
        self.checkout("main")
    }
}

impl Drop for TestRepo {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}
