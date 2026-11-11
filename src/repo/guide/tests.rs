//! The first steps of real repositories, read the way the tab reads them, the empty first
//! commit's undo and redo on them, and the guide's actions on a tab.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::empty_commit::{EMPTY_COMMIT_LABEL, empty_commit_steps};
use super::first_steps::{Facts, FirstSteps, MISSING, NEEDS_COMMIT, StepState, first_steps};
use super::hints::HintState;
use super::{GuideAct, Hint};
use crate::git;
use crate::repo::operations::run_steps;
use crate::repo::{Action, OpKind, RepoTab, Undo, UndoEntry, WIP_HASH};
use crate::settings::Settings;

/// A fresh, empty repository (`git init`, no commit) in a folder with spaces and non-ASCII
/// letters in its name.
fn empty_repo(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("gitcraft guide ü {name} {}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::create_dir_all(&dir);
    for args in [
        &["init", "-q", "-b", "main"][..],
        &["config", "user.name", "Zoë Tester"],
        &["config", "user.email", "t@t"],
    ] {
        assert!(git::run(Some(&dir), args).is_ok());
    }
    dir
}

/// A bare repository beside `dir`, to publish to.
fn bare_beside(dir: &Path) -> PathBuf {
    let bare = dir.with_file_name(format!(
        "{} bare",
        dir.file_name().unwrap_or_default().to_string_lossy()
    ));
    let _ = std::fs::remove_dir_all(&bare);
    assert!(git::run(None, &["init", "-q", "--bare", &bare.to_string_lossy()]).is_ok());
    bare
}

/// The facts of `dir`, read with the reads the tab makes.
fn read(dir: &Path) -> Facts {
    let refs = git::refs(dir).unwrap_or_default();
    let work = git::status(dir).unwrap_or_default();
    let remotes = git::remotes(dir).unwrap_or_default();
    let commits = git::run(Some(dir), &["rev-list", "--count", "--all"])
        .unwrap_or_default()
        .trim()
        .parse()
        .unwrap_or_default();
    Facts::read(commits, &refs, &work, &remotes, &git::identity(dir))
}

fn steps(dir: &Path) -> Option<FirstSteps> {
    first_steps(&read(dir))
}

/// Makes the empty first commit in `dir` as the guide does (on the branch HEAD names), and
/// returns its undo entry.
fn make_empty_commit(dir: &Path) -> (Result<String, String>, Option<UndoEntry>) {
    let branch = git::run(Some(dir), &["symbolic-ref", "-q", "HEAD"])
        .unwrap_or_default()
        .trim()
        .to_owned();
    let (commit, undo) = empty_commit_steps(&branch);
    let (made, kind) = run_steps(dir, &commit, OpKind::Normal(undo, String::new()), None);
    let entry = match kind {
        OpKind::Normal(Undo::Custom { undo, redo }, _) => Some(UndoEntry {
            id: 0,
            agent: None,
            label: EMPTY_COMMIT_LABEL.to_owned(),
            undo,
            redo,
            heads: None,
        }),
        _ => None,
    };
    (made, entry)
}

fn undo(dir: &Path, entry: &UndoEntry) -> Result<String, String> {
    run_steps(dir, &entry.undo, OpKind::Undo(entry.clone()), None).0
}

fn redo(dir: &Path, entry: &UndoEntry) -> Result<String, String> {
    run_steps(dir, &entry.redo, OpKind::Redo(entry.clone()), None).0
}

fn branch_exists(dir: &Path, full: &str) -> bool {
    git::resolve(dir, full).is_ok()
}

#[test]
fn an_empty_repository_shows_its_first_steps_with_nothing_to_commit() {
    let dir = empty_repo("empty");
    let steps = steps(&dir);
    let _ = std::fs::remove_dir_all(&dir);
    let steps = steps.unwrap_or(MISSING);
    assert!(steps.empty);
    assert!(steps.nothing_to_commit);
    assert_eq!(steps.first_commit, StepState::Todo);
    assert_eq!(steps.add_remote, StepState::Todo);
    assert_eq!(steps.publish, StepState::Blocked(NEEDS_COMMIT));
}

#[test]
fn an_untracked_file_is_something_to_commit() {
    let dir = empty_repo("untracked");
    assert!(std::fs::write(dir.join("hello wörld.txt"), "hi\n").is_ok());
    let facts = read(&dir);
    let _ = std::fs::remove_dir_all(&dir);
    assert!(facts.has_changes);
    let steps = first_steps(&facts).unwrap_or(MISSING);
    assert!(steps.empty);
    assert!(!steps.nothing_to_commit);
}

#[test]
fn a_remote_marks_add_a_remote_done() {
    let dir = empty_repo("remote");
    let remote = dir.join("elsewhere ü.git");
    let added = git::run(
        Some(&dir),
        &["remote", "add", "origin", &remote.to_string_lossy()],
    );
    let steps = steps(&dir);
    let _ = std::fs::remove_dir_all(&dir);
    assert!(added.is_ok());
    let steps = steps.unwrap_or(MISSING);
    assert_eq!(steps.add_remote, StepState::Done);
    assert_eq!(steps.publish, StepState::Blocked(NEEDS_COMMIT));
}

#[test]
fn after_the_first_commit_publish_is_offered_then_publishing_ends_the_guide() {
    let dir = empty_repo("publish");
    let bare = bare_beside(&dir);
    assert!(std::fs::write(dir.join("a.txt"), "a\n").is_ok());
    for args in [&["add", "-A"][..], &["commit", "-q", "-m", "first"]] {
        assert!(git::run(Some(&dir), args).is_ok());
    }
    let local_only = steps(&dir);
    let added = git::run(
        Some(&dir),
        &["remote", "add", "origin", &bare.to_string_lossy()],
    );
    let with_remote = steps(&dir);
    // As the toolbar's Push publishes a branch without an upstream.
    let pushed = git::run(
        Some(&dir),
        &[
            "-c",
            "push.default=simple",
            "push",
            "-u",
            "origin",
            "refs/heads/main",
        ],
    );
    let published = steps(&dir);
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_dir_all(&bare);
    let local_only = local_only.unwrap_or(MISSING);
    assert!(!local_only.empty);
    assert_eq!(local_only.first_commit, StepState::Done);
    assert_eq!(local_only.publish, StepState::Blocked("Add a remote first"));
    assert!(added.is_ok());
    assert_eq!(with_remote.unwrap_or(MISSING).publish, StepState::Todo);
    assert!(pushed.is_ok(), "{pushed:?}");
    assert_eq!(published, None);
}

#[test]
fn a_repository_with_history_shows_no_first_steps() {
    let dir = empty_repo("history");
    for n in 0..3 {
        let msg = format!("c{n}");
        assert!(git::run(Some(&dir), &["commit", "-q", "--allow-empty", "-m", &msg]).is_ok());
    }
    let steps = steps(&dir);
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(steps, None);
}

#[test]
fn an_unborn_orphan_branch_in_a_repository_with_history_shows_no_first_steps() {
    let ctx = egui::Context::default();
    let dir = empty_repo("orphan");
    for n in 0..3 {
        let msg = format!("c{n}");
        assert!(git::run(Some(&dir), &["commit", "-q", "--allow-empty", "-m", &msg]).is_ok());
    }
    assert!(git::run(Some(&dir), &["checkout", "-q", "--orphan", "fresh"]).is_ok());
    let mut tab = read_tab(&ctx, &dir, false);
    // The history the tab logs (every branch) still has the commits.
    tab.history = (0..3)
        .map(|n| {
            Arc::new(git::Commit {
                hash: format!("{n:040}"),
                ..Default::default()
            })
        })
        .collect();
    let shown = tab.first_steps();
    let _ = std::fs::remove_dir_all(&dir);
    assert!(tab.refs.head.is_empty());
    assert_eq!(shown, None);
}

// Keep fixture helpers and qualified test names in this module.
include!("tests/empty_commit.rs");

// Keep fixture helpers and qualified test names in this module.
include!("tests/view.rs");
