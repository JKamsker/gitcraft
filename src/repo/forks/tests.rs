//! Pull requests from forks against real repositories: an upstream and a contributor's fork as
//! bare repositories, and the maintainer's clone of the upstream.

use std::path::{Path, PathBuf};

use super::plan::{self, Setup};
use crate::forge::ForkInfo;
use crate::git;
use crate::repo::operations::run_steps;
use crate::repo::{OpKind, Undo, UndoEntry};

fn folder(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("gitcraft forks-ü {name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::create_dir_all(&dir);
    dir
}

fn run(dir: &Path, args: &[&str]) -> String {
    let out = git::run(Some(dir), args);
    assert!(out.is_ok(), "git {args:?} failed: {out:?}");
    out.unwrap_or_default()
}

fn arg(p: &Path) -> String {
    p.to_string_lossy().into_owned()
}

fn identity(dir: &Path, name: &str) {
    run(dir, &["config", "user.name", name]);
    run(
        dir,
        &["config", "user.email", &format!("{name}@example.org")],
    );
}

fn commit(dir: &Path, file: &str) {
    let _ = std::fs::write(dir.join(file), format!("{file}\n"));
    run(dir, &["add", "--", file]);
    run(dir, &["commit", "-q", "-m", &format!("add {file}")]);
}

/// The upstream, Ann's fork of it with her branch `fix`, and the maintainer's clone.
struct World {
    root: PathBuf,
    upstream: PathBuf,
    fork: PathBuf,
    ann: PathBuf,
    me: PathBuf,
}

impl World {
    fn new(name: &str) -> World {
        let root = folder(name);
        let upstream = root.join("upstream.git");
        let fork = root.join("ann fork.git");
        let ann = root.join("ann");
        let me = root.join("me");
        let _ = std::fs::create_dir_all(&upstream);
        run(&upstream, &["init", "-q", "--bare", "-b", "main"]);
        // The first commit, through Ann's clone.
        run(&root, &["clone", "-q", &arg(&upstream), &arg(&ann)]);
        identity(&ann, "ann");
        commit(&ann, "base.txt");
        run(&ann, &["push", "-q", "origin", "HEAD:main"]);
        run(
            &root,
            &["clone", "-q", "--bare", &arg(&upstream), &arg(&fork)],
        );
        run(&ann, &["remote", "add", "fork", &arg(&fork)]);
        run(&ann, &["checkout", "-q", "-b", "fix"]);
        commit(&ann, "fix ü.txt");
        run(&ann, &["push", "-q", "fork", "fix"]);
        run(&root, &["clone", "-q", &arg(&upstream), &arg(&me)]);
        identity(&me, "me");
        World {
            root,
            upstream,
            fork,
            ann,
            me,
        }
    }

    fn info(&self) -> ForkInfo {
        ForkInfo {
            number: 7,
            cross: true,
            owner: "ann".into(),
            repo_path: "ann/app".into(),
            clone_https: arg(&self.fork),
            clone_ssh: String::new(),
            branch: "fix".into(),
            base: "main".into(),
            sha: git::resolve(&self.fork, "refs/heads/fix").unwrap_or_default(),
            maintainers_can_edit: true,
            ..ForkInfo::default()
        }
    }

    fn setup(&self) -> Setup {
        plan::read_setup(&self.me).unwrap_or_default()
    }

    fn checkout(&self, locals: &[String]) -> Result<plan::Checkout, String> {
        let remotes = git::remotes(&self.me).unwrap_or_default();
        plan::checkout(&self.info(), &remotes, &self.setup(), locals, "main", None)
    }
}

impl Drop for World {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn custom(kind: OpKind) -> Option<UndoEntry> {
    match kind {
        OpKind::Normal(Undo::Custom { undo, redo }, _) => Some(UndoEntry {
            id: 0,
            agent: None,
            label: "x".into(),
            undo,
            redo,
            heads: None,
        }),
        _ => None,
    }
}

fn normal(plan_undo: Vec<Vec<String>>, redo: Vec<Vec<String>>) -> OpKind {
    OpKind::Normal(
        Undo::Custom {
            undo: plan_undo,
            redo,
        },
        String::new(),
    )
}
include!("tests/checkout_and_rebase.rs");
include!("tests/planning_scope.rs");
include!("tests/execution_scope.rs");
include!("tests/approval_and_staleness.rs");
include!("tests/destination_and_undo.rs");
include!("tests/recovery_leases.rs");
include!("tests/push_windows.rs");
include!("tests/checkout_guards.rs");
include!("tests/saved_identity.rs");
