//! Starting work on an issue in real repositories, through the steps the tab runs: the branch or
//! worktree made, named after the issue, and the link to the issue kept and read back; and, in a
//! tab, where the branch starts from and what the picker would say.

use std::path::{Path, PathBuf};

use super::link::{self, IssueLink, Place, Source, Start};
use super::naming::branch_name;
use super::{IssueAct, Plan};
use crate::forge::Issue;
use crate::git;
use crate::repo::operations::run_steps;
use crate::repo::{OpKind, RepoTab};

/// A repository with a commit on `main`, and `origin/main` a commit ahead of it (the remote
/// has moved on since the last pull), in a fresh folder named after `name`.
fn repo(name: &str) -> PathBuf {
    let dir = scratch(name);
    for args in [
        &["init", "-q"][..],
        &["symbolic-ref", "HEAD", "refs/heads/main"],
        &["config", "user.name", "t"],
        &["config", "user.email", "t@t"],
        &["config", "commit.gpgSign", "false"],
        &["commit", "-q", "--allow-empty", "-m", "base"],
        &["commit", "-q", "--allow-empty", "-m", "on the remote"],
        &["update-ref", "refs/remotes/origin/main", "HEAD"],
        &["reset", "-q", "--hard", "HEAD~1"],
    ] {
        assert!(git::run(Some(&dir), args).is_ok(), "{args:?}");
    }
    dir
}

/// A fresh, empty folder named after `name` (and nothing beside it left from a run before).
fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("gitcraft-issue-{name}-{}", std::process::id()));
    clean(&dir);
    let _ = std::fs::create_dir_all(&dir);
    dir
}

fn worktree_dir(repo: &Path) -> PathBuf {
    PathBuf::from(format!("{}-wt", repo.display()))
}

fn clean(dir: &Path) {
    let _ = std::fs::remove_dir_all(dir);
    for beside in ["wt", "remote", "seed", "clone"] {
        let _ = std::fs::remove_dir_all(format!("{}-{beside}", dir.display()));
    }
}

fn issue(number: u64, title: &str) -> Issue {
    Issue {
        tracker: None,
        acceptance: None,
        number,
        title: title.into(),
        body:
            "Login crashes.\n\n## Acceptance criteria\n- [ ] no crash\n- [x] a test\n\n## Notes\n\
               later"
                .into(),
        url: format!("https://github.com/o/r/issues/{number}"),
        labels: vec!["bug".into()],
    }
}

fn git_out(dir: &Path, args: &[&str]) -> String {
    git::run(Some(dir), args)
        .map(|out| out.trim().to_owned())
        .unwrap_or_default()
}

fn new_from(from: &str) -> Source {
    Source::New { from: from.into() }
}

/// A remote whose default branch (its `HEAD`) is `dev`, with a `master` beside it, and a clone
/// of it; then `dev` moves on, on the remote, without the clone fetching. The clone's folder.
fn cloned(name: &str) -> PathBuf {
    let dir = scratch(name);
    let _ = std::fs::remove_dir_all(&dir);
    let remote = format!("{}-remote", dir.display());
    let seed = PathBuf::from(format!("{}-seed", dir.display()));
    let _ = std::fs::create_dir_all(&seed);
    let ok = |dir: Option<&Path>, args: &[&str]| {
        assert!(git::run(dir, args).is_ok(), "{args:?}");
    };
    ok(None, &["init", "-q", "--bare", &remote]);
    for args in [
        &["init", "-q"][..],
        &["symbolic-ref", "HEAD", "refs/heads/master"],
        &["config", "user.name", "t"],
        &["config", "user.email", "t@t"],
        &["config", "commit.gpgSign", "false"],
        &["commit", "-q", "--allow-empty", "-m", "master"],
        &["remote", "add", "origin", &remote],
        &["push", "-q", "origin", "master"],
        &["checkout", "-q", "-b", "dev"],
        &["commit", "-q", "--allow-empty", "-m", "dev"],
        &["push", "-q", "origin", "dev"],
        &["checkout", "-q", "-b", "8-elsewhere"],
        &["commit", "-q", "--allow-empty", "-m", "started elsewhere"],
        &["push", "-q", "origin", "8-elsewhere"],
        &["checkout", "-q", "dev"],
    ] {
        ok(Some(&seed), args);
    }
    ok(
        None,
        &[
            "--git-dir",
            &remote,
            "symbolic-ref",
            "HEAD",
            "refs/heads/dev",
        ],
    );
    let clone = dir.to_string_lossy().into_owned();
    ok(None, &["clone", "-q", "--no-checkout", &remote, &clone]);
    for args in [
        &["checkout", "-q", "master"][..],
        &["config", "user.name", "t"],
        &["config", "user.email", "t@t"],
    ] {
        ok(Some(&dir), args);
    }
    ok(
        Some(&seed),
        &["commit", "-q", "--allow-empty", "-m", "dev moved on"],
    );
    ok(Some(&seed), &["push", "-q", "origin", "dev"]);
    dir
}

/// A tab on `dir`, its first read in.
fn tab(dir: &Path, ctx: &egui::Context) -> RepoTab {
    let mut tab = RepoTab::new_deferred(dir.to_path_buf(), ctx);
    tab.reload(ctx);
    wait(&mut tab, ctx);
    tab
}

/// Until the tab has read the repository again, and isn't running anything.
fn wait(tab: &mut RepoTab, ctx: &egui::Context) {
    for _ in 0..400 {
        tab.poll(ctx);
        if !tab.loading && tab.busy_op.is_none() && tab.history_gen > 0 {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
}

fn start(tab: &mut RepoTab, ctx: &egui::Context, issue: Issue, branch: &str, worktree: bool) {
    tab.issue_act(
        IssueAct::Start {
            issue: Box::new(issue),
            branch: branch.to_owned(),
            worktree,
        },
        ctx,
    );
    wait(tab, ctx);
    // The reload the operation asked for.
    tab.reload(ctx);
    wait(tab, ctx);
}

fn note(plan: &Plan) -> String {
    match &plan.start {
        Ok(_) => plan.note.clone(),
        Err(why) => format!("refused: {why}"),
    }
}

// Keep fixture helpers and shard-visible test names in this module.
include!("tests/branch_lifecycle.rs");
include!("tests/start_plans.rs");
include!("tests/forge_identity.rs");
