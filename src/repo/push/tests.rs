//! Pushes refused for real: two clones of a bare repository, one pushing behind the other's
//! back, and what the check, the lease, Sync's pull and a first push make of it.

use std::path::{Path, PathBuf};

use super::check::{self, Checked, Question, Rejection, Shape, Target};
use super::porcelain::{self, RefLine, Refusal};
use super::{Attempt, Chain, force_push_ops, publish_args, pull_from, refused_target};
use crate::git;
use crate::repo::operations::run_steps;
use crate::repo::pull::PullMode;
use crate::repo::{OpKind, ops};

/// A bare repository with `main`, and two clones of it, `mine` and `theirs`; gone when dropped.
struct World {
    root: PathBuf,
    bare: PathBuf,
    mine: PathBuf,
    theirs: PathBuf,
}

impl Drop for World {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn run(dir: &Path, args: &[&str]) -> Result<String, String> {
    git::run(Some(dir), args).map_err(|e| format!("git {args:?} in {}: {e}", dir.display()))
}

fn path(p: &Path) -> String {
    p.to_string_lossy().into_owned()
}

fn world(name: &str) -> Result<World, String> {
    let root = std::env::temp_dir().join(format!("gitcraft-push-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).map_err(|e| e.to_string())?;
    let w = World {
        bare: root.join("remote.git"),
        mine: root.join("mine"),
        theirs: root.join("theirs"),
        root,
    };
    run(
        &w.root,
        &["init", "-q", "--bare", "-b", "main", &path(&w.bare)],
    )?;
    for dir in [&w.mine, &w.theirs] {
        run(&w.root, &["clone", "-q", &path(&w.bare), &path(dir)])?;
        run(dir, &["config", "user.name", "t"])?;
        run(dir, &["config", "user.email", "t@t"])?;
        run(dir, &["checkout", "-q", "-B", "main"])?;
    }
    commit(&w.mine, "base")?;
    run(&w.mine, &["push", "-q", "-u", "origin", "main"])?;
    run(&w.theirs, &["pull", "-q", "origin", "main"])?;
    run(&w.theirs, &["branch", "-q", "-u", "origin/main"])?;
    Ok(w)
}

fn commit(dir: &Path, message: &str) -> Result<String, String> {
    run(dir, &["commit", "-q", "--allow-empty", "-m", message])?;
    Ok(git::head(dir))
}

/// A file `f.txt` saying `text`, committed.
fn commit_file(dir: &Path, text: &str) -> Result<String, String> {
    std::fs::write(dir.join("f.txt"), text).map_err(|e| e.to_string())?;
    run(dir, &["add", "f.txt"])?;
    commit(dir, text.trim())
}

fn amend(dir: &Path, message: &str) -> Result<String, String> {
    run(
        dir,
        &["commit", "-q", "--amend", "--allow-empty", "-m", message],
    )
}

fn remote_main(w: &World) -> String {
    git::resolve(&w.bare, "refs/heads/main").unwrap_or_default()
}

/// Pushes as the toolbar does, from `dir`: the error, if it failed.
fn plain_push(dir: &Path) -> Option<String> {
    run_steps(
        dir,
        &ops(&[&["push", "--porcelain"]]),
        OpKind::plain(),
        None,
    )
    .0
    .err()
}

fn question(branch: &str, target: Option<Target>, to: &str, remotes: &[&str]) -> Question {
    Question {
        branch: branch.to_owned(),
        target,
        to: to.to_owned(),
        remotes: remotes.iter().map(|r| (*r).to_owned()).collect(),
        publish: false,
        moved: false,
    }
}

fn origin(branch: &str) -> Target {
    Target {
        remote: "origin".to_owned(),
        branch: branch.to_owned(),
    }
}

/// The refused line of a push of `main`, and why.
fn refused_line(error: &str) -> Result<(RefLine, Refusal), String> {
    porcelain::refused(error, "refs/heads/main", None)
        .ok_or_else(|| format!("no ref line: {error}"))
}

/// Pushes `mine` as the toolbar does, and checks the refusal as the tab does.
fn push_and_check(w: &World) -> Result<Box<Rejection>, String> {
    let error = plain_push(&w.mine).ok_or("the push went through")?;
    let (line, why) = refused_line(&error)?;
    if why != Refusal::Behind {
        return Err(format!("refused otherwise: {why:?}"));
    }
    let q = question("main", None, &line.to, &["origin"]);
    rejection(check::check(&w.mine, &q, None)?)
}

fn rejection(checked: Checked) -> Result<Box<Rejection>, String> {
    match checked {
        Checked::Rejected(r) => Ok(r),
        other => Err(format!("not a rejection: {other:?}")),
    }
}
include!("tests/rejection_and_pull.rs");
include!("tests/destination.rs");
include!("tests/sync.rs");

include!("tests/direct.rs");
