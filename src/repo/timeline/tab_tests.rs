//! The timeline on real repositories, through the tab as the UI drives it: kept across a
//! restart with undo still working, changes made elsewhere seen, a rebase's old tip got back
//! after a gc, the undo tree branching and jumping, and jumps that refuse.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use super::view::TimelineAct;
use super::{Origin, snapshot, store};
use crate::git;
use crate::repo::actions::{BranchRef, rebase_action};
use crate::repo::{Action, RepoTab};

/// A repository with one commit, in a folder with spaces and accents in its name.
pub(super) fn repo(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("gitcraft tl ü {name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::create_dir_all(&dir);
    for args in [
        &["init", "-q", "-b", "main"][..],
        &["config", "user.name", "Tëst Üser"],
        &["config", "user.email", "t@t"],
        &["commit", "-q", "--allow-empty", "-m", "base"],
    ] {
        assert!(git::run(Some(&dir), args).is_ok(), "{args:?}");
    }
    // A timeline left by an earlier run of the tests.
    if let Some(file) = store::file(&dir) {
        let _ = std::fs::remove_file(file);
    }
    dir
}

pub(super) fn git_ok(dir: &Path, args: &[&str]) -> String {
    let out = git::run(Some(dir), args);
    assert!(out.is_ok(), "{args:?}: {out:?}");
    out.unwrap_or_default().trim().to_owned()
}

/// Polls the tab until `done` holds (or ten seconds went by); says whether it did.
pub(super) fn wait(
    tab: &mut RepoTab,
    ctx: &egui::Context,
    done: impl Fn(&RepoTab) -> bool,
) -> bool {
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(10) {
        tab.poll(ctx);
        if done(tab) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    false
}

/// Settled: read, nothing running, the timeline in and not waiting for a read.
pub(super) fn settled(tab: &RepoTab) -> bool {
    !tab.loading
        && tab.busy_op.is_none()
        && tab.history_gen > 0
        && tab.timeline.loaded
        && tab.timeline.awaiting == super::Awaiting::Nothing
}

pub(super) fn open(dir: &Path, ctx: &egui::Context) -> RepoTab {
    let mut tab = RepoTab::new(dir.to_owned(), ctx);
    assert!(wait(&mut tab, ctx, settled), "the tab never settled");
    tab
}

/// Runs `action` as the UI would, and waits for the read after it.
pub(super) fn act(tab: &mut RepoTab, ctx: &egui::Context, action: Action) {
    tab.apply(action, ctx);
    assert!(wait(tab, ctx, settled), "{:?}", tab.status);
}

pub(super) fn commit(tab: &mut RepoTab, ctx: &egui::Context, file: &str, summary: &str) -> String {
    let _ = std::fs::write(tab.path.join(file), format!("{summary}\n"));
    tab.commit_summary = summary.to_owned();
    act(tab, ctx, Action::Commit);
    git::head(&tab.path)
}

fn keeper_exists(dir: &Path, id: u64) -> bool {
    git::run(
        Some(dir),
        &[
            "rev-parse",
            "--verify",
            "-q",
            &snapshot::ref_name(&store::key(dir), id),
        ],
    )
    .is_ok()
}
include!("tests/tab_persistence.rs");
include!("tests/tab_navigation.rs");
include!("tests/tab_restore.rs");
