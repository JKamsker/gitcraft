//! The release dialog's steps in a tab, on a real repository with a bare remote: cut, undone
//! and redone; and a failed push resumed.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use super::{ReleaseAct, Status};
use crate::git;
use crate::repo::RepoTab;

fn folder(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "gitcraft release-tab ü {name}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::create_dir_all(&dir);
    dir
}

fn run(dir: &Path, args: &[&str]) {
    let out = git::run(Some(dir), args);
    assert!(out.is_ok(), "git {args:?}: {out:?}");
}

fn project(name: &str) -> (PathBuf, PathBuf) {
    let dir = folder(name);
    let bare = folder(&format!("{name}-origin.git"));
    run(&bare, &["init", "-q", "--bare"]);
    run(&dir, &["init", "-q", "-b", "main"]);
    run(&dir, &["config", "user.name", "Zoë"]);
    run(&dir, &["config", "user.email", "zoe@example.org"]);
    run(&dir, &["config", "tag.gpgSign", "false"]);
    run(&dir, &["config", "commit.gpgSign", "false"]);
    let _ = std::fs::write(
        dir.join("package.json"),
        "{\n  \"name\": \"app\",\n  \"version\": \"2.3.1\"\n}\n",
    );
    run(&dir, &["add", "-A"]);
    run(&dir, &["commit", "-q", "-m", "chore: start"]);
    run(&dir, &["tag", "-a", "v2.3.1", "-m", "v2.3.1"]);
    let _ = std::fs::write(dir.join("fix.txt"), "x\n");
    run(&dir, &["add", "-A"]);
    run(&dir, &["commit", "-q", "-m", "fix: the crash"]);
    run(&dir, &["remote", "add", "origin", &bare.to_string_lossy()]);
    run(&dir, &["push", "-q", "-u", "origin", "main"]);
    (dir, bare)
}

fn wait(tab: &mut RepoTab, ctx: &egui::Context, done: impl Fn(&RepoTab) -> bool) {
    let until = Instant::now() + Duration::from_secs(30);
    while !done(tab) && Instant::now() < until {
        std::thread::sleep(Duration::from_millis(20));
        tab.poll(ctx);
    }
}

fn open(dir: &Path, ctx: &egui::Context) -> RepoTab {
    let mut tab = RepoTab::new_deferred(dir.to_path_buf(), ctx);
    tab.reload(ctx);
    wait(&mut tab, ctx, |t| !t.loading && t.history_gen > 0);
    let head = tab.refs.head.clone();
    tab.release_act(ReleaseAct::Open(head), ctx);
    wait(&mut tab, ctx, |t| {
        t.release.as_ref().is_some_and(|r| r.draft.is_some())
    });
    tab
}

fn statuses(tab: &RepoTab) -> Vec<Status> {
    tab.release
        .as_ref()
        .and_then(|r| r.run.as_ref())
        .map(|r| r.steps.iter().map(|(_, s)| s.clone()).collect())
        .unwrap_or_default()
}

#[test]
fn a_release_cut_from_the_dialog_is_undone_and_redone_here() {
    let (dir, bare) = project("cut");
    let ctx = egui::Context::default();
    let mut tab = open(&dir, &ctx);
    let suggested = tab
        .release
        .as_ref()
        .map(|r| (r.form.tag.clone(), r.form.bump, r.form.publish));
    tab.release_act(ReleaseAct::Run, &ctx);
    wait(&mut tab, &ctx, |t| t.busy_op.is_none() && !t.loading);
    let steps = statuses(&tab);
    let bumped = std::fs::read_to_string(dir.join("package.json")).unwrap_or_default();
    let remote_tag = git::resolve(&bare, "refs/tags/v2.3.2").is_ok();
    let undo_label = tab.undo_stack.last().map(|e| e.label.clone());
    tab.release_act(ReleaseAct::Close, &ctx);
    let closed = tab.release.is_none();
    tab.undo(&ctx);
    wait(&mut tab, &ctx, |t| t.busy_op.is_none() && !t.loading);
    let tag_gone = git::resolve(&dir, "refs/tags/v2.3.2").is_err();
    let subject = git::run(Some(&dir), &["log", "-1", "--format=%s"]).unwrap_or_default();
    tab.redo(&ctx);
    wait(&mut tab, &ctx, |t| t.busy_op.is_none() && !t.loading);
    let tag_back = git::resolve(&dir, "refs/tags/v2.3.2").is_ok();
    // Put back as the annotated tag it was, its notes with it.
    let kind = git::run(Some(&dir), &["cat-file", "-t", "refs/tags/v2.3.2"]).unwrap_or_default();
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_dir_all(&bare);
    // No forge here: nothing to publish.
    assert_eq!(suggested, Some(("v2.3.2".to_owned(), true, false)));
    assert_eq!(steps, [Status::Done, Status::Done, Status::Done]);
    assert!(bumped.contains("\"version\": \"2.3.2\""), "{bumped}");
    assert!(remote_tag);
    assert_eq!(undo_label.as_deref(), Some("Cut release v2.3.2"));
    assert!(closed);
    assert!(tag_gone);
    assert_eq!(subject.trim(), "fix: the crash");
    assert!(tag_back);
    assert_eq!(kind.trim(), "tag");
}

#[test]
fn a_failed_push_is_resumed_and_the_undo_covers_it_all() {
    let (dir, bare) = project("resume");
    let ctx = egui::Context::default();
    let mut tab = open(&dir, &ctx);
    if let Some(r) = &mut tab.release {
        r.form.bump = false;
    }
    let moved = bare.with_extension("moved");
    let _ = std::fs::rename(&bare, &moved);
    tab.release_act(ReleaseAct::Run, &ctx);
    wait(&mut tab, &ctx, |t| t.busy_op.is_none() && !t.loading);
    let first = statuses(&tab);
    let entries = tab.undo_stack.len();
    let _ = std::fs::rename(&moved, &bare);
    tab.release_act(ReleaseAct::Run, &ctx);
    wait(&mut tab, &ctx, |t| t.busy_op.is_none() && !t.loading);
    let second = statuses(&tab);
    let remote_tag = git::resolve(&bare, "refs/tags/v2.3.2").is_ok();
    let entries_after = tab.undo_stack.len();
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_dir_all(&bare);
    assert_eq!(first.len(), 2);
    assert_eq!(first[0], Status::Done);
    assert!(matches!(first[1], Status::Failed(_)), "{first:?}");
    assert_eq!(second, [Status::Done, Status::Done]);
    assert!(remote_tag);
    assert_eq!(entries, 1);
    assert_eq!(entries_after, 1, "resumed, the same entry");
}

/// Undone while the dialog waits on a failed step, Resume says so rather than push what is
/// gone; Skip leaves a step out and goes on.
#[test]
fn resume_after_an_undo_refuses_and_skip_goes_on() {
    let (dir, bare) = project("undone");
    let ctx = egui::Context::default();
    let mut tab = open(&dir, &ctx);
    if let Some(r) = &mut tab.release {
        r.form.bump = false;
    }
    let moved = bare.with_extension("moved");
    let _ = std::fs::rename(&bare, &moved);
    tab.release_act(ReleaseAct::Run, &ctx);
    wait(&mut tab, &ctx, |t| t.busy_op.is_none() && !t.loading);
    // The tag taken back by hand meanwhile.
    run(&dir, &["tag", "-d", "v2.3.2"]);
    let _ = std::fs::rename(&moved, &bare);
    tab.release_act(ReleaseAct::Run, &ctx);
    wait(&mut tab, &ctx, |t| t.busy_op.is_none() && !t.loading);
    let refused = tab.status.clone();
    let remote_tag = git::resolve(&bare, "refs/tags/v2.3.2").is_ok();
    tab.release_act(ReleaseAct::Skip, &ctx);
    let after_skip = statuses(&tab);
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_dir_all(&bare);
    assert!(refused.is_some_and(|(s, err)| err && s.contains("isn't the tag")));
    assert!(!remote_tag);
    assert_eq!(after_skip, [Status::Done, Status::Skipped]);
}
