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
    let undo_label = tab
        .timeline
        .current_entry()
        .and_then(|e| e.undo.as_ref())
        .map(|e| e.label.clone());
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
    let entries = tab
        .timeline
        .entries
        .iter()
        .filter(|e| e.undo.is_some())
        .count();
    let _ = std::fs::rename(&moved, &bare);
    tab.release_act(ReleaseAct::Run, &ctx);
    wait(&mut tab, &ctx, |t| t.busy_op.is_none() && !t.loading);
    let second = statuses(&tab);
    let remote_tag = git::resolve(&bare, "refs/tags/v2.3.2").is_ok();
    let entries_after = tab
        .timeline
        .entries
        .iter()
        .filter(|e| e.undo.is_some())
        .count();
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
    let refused = tab.status.as_deref().cloned();
    let remote_tag = git::resolve(&bare, "refs/tags/v2.3.2").is_ok();
    tab.release_act(ReleaseAct::Skip, &ctx);
    let after_skip = statuses(&tab);
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_dir_all(&bare);
    assert!(refused.is_some_and(|(s, err)| err && s.contains("isn't the tag")));
    assert!(!remote_tag);
    assert_eq!(after_skip, [Status::Done, Status::Skipped]);
}

#[test]
fn adversarial_unrelated_catalog_edit_keeps_release_completion_and_undo() {
    let _pause = crate::network::test_pause::set(Some(crate::network::Pause::Switch));
    let (dir, bare) = project("catalog-edit");
    let ctx = egui::Context::default();
    let mut tab = open(&dir, &ctx);
    // The initial check is in flight; its continuation must still own the run.
    tab.release_act(ReleaseAct::Run, &ctx);
    tab.remotes.push(crate::git::Remote {
        name: "unrelated".into(),
        url: "/unused".into(),
    });
    tab.take_forge_selection(true, &ctx);
    assert!(
        tab.release.as_ref().is_some_and(|v| v.run.is_some()),
        "catalog edit discarded the active release and its undo bookkeeping"
    );
    wait(&mut tab, &ctx, |t| t.busy_op.is_none() && !t.loading);
    assert_eq!(statuses(&tab), [Status::Done, Status::Done, Status::Done]);
    assert!(
        tab.release
            .as_ref()
            .and_then(|v| v.run.as_ref())
            .is_some_and(|run| run.done.bump_commit.is_some() && run.done.tag_object.is_some())
    );
    assert!(git::resolve(&bare, "refs/tags/v2.3.2").is_ok());
    assert_eq!(
        tab.timeline
            .current_entry()
            .and_then(|e| e.undo.as_ref())
            .map(|e| e.label.as_str()),
        Some("Cut release v2.3.2")
    );
    tab.undo(&ctx);
    wait(&mut tab, &ctx, |t| t.busy_op.is_none() && !t.loading);
    assert!(git::resolve(&dir, "refs/tags/v2.3.2").is_err());
    assert!(
        std::fs::read_to_string(dir.join("package.json"))
            .unwrap_or_default()
            .contains("2.3.1")
    );
    let _ = std::fs::remove_dir_all(dir);
    let _ = std::fs::remove_dir_all(bare);
}

#[test]
fn adversarial_changed_primary_stops_later_publication_but_keeps_local_undo() {
    let _pause = crate::network::test_pause::set(Some(crate::network::Pause::Switch));
    let (dir, bare) = project("catalog-forge-move");
    let ctx = egui::Context::default();
    let mut tab = open(&dir, &ctx);
    let original = crate::forge::Hosting {
        forge: crate::forge::Forge::GitHub,
        host: "github.com".into(),
        path: "original/project".into(),
        web: "https://github.com".into(),
    };
    tab.hosting = Some(original.clone());
    if let Some(view) = &mut tab.release {
        view.form.publish = false;
    }
    tab.release_act(ReleaseAct::Run, &ctx);
    wait(&mut tab, &ctx, |t| t.busy_op.is_none() && !t.loading);
    // Seed a release paused after its completed local phase, before publication. This
    // isolates resumption after a primary change without requiring a live forge push.
    if let Some(run) = tab.release.as_mut().and_then(|v| v.run.as_mut()) {
        run.plan.publish = true;
        run.steps
            .push((crate::release::Step::Publish, Status::Waiting));
    }
    let replacement = crate::forge::Hosting {
        path: "replacement/project".into(),
        ..original.clone()
    };
    tab.remotes.push(crate::git::Remote {
        name: "forge".into(),
        url: "https://github.com/replacement/project".into(),
    });
    tab.forge_remotes = vec![("forge".into(), replacement)];
    tab.take_forge_selection(true, &ctx);
    assert!(
        tab.release
            .as_ref()
            .and_then(|v| v.run.as_ref())
            .is_some_and(|run| run.hosting.as_ref() == Some(&original)),
        "publication destination was discarded or retargeted"
    );
    tab.next_release_step(&ctx);
    wait(&mut tab, &ctx, |t| t.busy_op.is_none() && !t.loading);
    let steps = statuses(&tab);
    assert_eq!(&steps[..3], &[Status::Done, Status::Done, Status::Done]);
    assert!(
        matches!(steps.get(3), Some(Status::Failed(why)) if why.contains("primary forge changed")),
        "{steps:?}"
    );
    assert!(
        git::resolve(&bare, "refs/tags/v2.3.2").is_ok(),
        "the original Git push completed"
    );
    assert!(
        tab.release
            .as_ref()
            .and_then(|v| v.run.as_ref())
            .is_some_and(|run| run.recorded
                && run.done.bump_commit.is_some()
                && run.done.tag_object.is_some())
    );
    assert_eq!(
        tab.timeline
            .entries
            .iter()
            .filter(|e| e.undo.is_some())
            .count(),
        1
    );
    tab.undo(&ctx);
    wait(&mut tab, &ctx, |t| t.busy_op.is_none() && !t.loading);
    assert!(git::resolve(&dir, "refs/tags/v2.3.2").is_err());
    assert!(
        std::fs::read_to_string(dir.join("package.json"))
            .unwrap_or_default()
            .contains("2.3.1")
    );
    let _ = std::fs::remove_dir_all(dir);
    let _ = std::fs::remove_dir_all(bare);
}

#[test]
fn review9_release_push_uses_selected_forge_remote_and_preserves_normal_upstream() {
    let _pause = crate::network::test_pause::set(Some(crate::network::Pause::Switch));
    let (dir, origin) = project("selected-forge");
    let fork = folder("selected-forge-fork.git");
    run(&fork, &["init", "-q", "--bare"]);
    run(
        &dir,
        &["remote", "add", "fork", "https://github.com/fork/project"],
    );
    // Redirect only this fixture's Git writes to a local bare repository; no forge process
    // executes, and publication is disabled before running the release.
    run(
        &dir,
        &[
            "remote",
            "set-url",
            "--push",
            "fork",
            &fork.to_string_lossy(),
        ],
    );
    let config = std::fs::read(dir.join(".git/config")).unwrap_or_default();
    let ctx = egui::Context::default();
    let mut tab = open(&dir, &ctx);
    assert_eq!(tab.primary_forge_remote(), Some("fork"));
    assert_eq!(tab.default_remote().as_deref(), Some("origin"));
    let draft = tab
        .release
        .as_ref()
        .and_then(|r| r.draft.as_ref())
        .and_then(|d| d.as_ref().ok());
    assert_eq!(
        draft.and_then(|d| d.remote.as_deref()),
        Some("fork"),
        "release preview must push to the same repository it publishes on"
    );
    if let Some(view) = &mut tab.release {
        view.form.publish = false;
    }
    tab.release_act(ReleaseAct::Run, &ctx);
    wait(&mut tab, &ctx, |t| t.busy_op.is_none() && !t.loading);
    assert_eq!(statuses(&tab), [Status::Done, Status::Done, Status::Done]);
    assert!(
        git::resolve(&fork, "refs/tags/v2.3.2").is_ok(),
        "selected fork must receive the tag"
    );
    assert!(
        git::resolve(&origin, "refs/tags/v2.3.2").is_err(),
        "ordinary origin must not receive the release tag"
    );
    assert_eq!(
        git::run(Some(&dir), &["config", "branch.main.remote"]).map(|s| s.trim().to_owned()),
        Ok("origin".into())
    );
    assert_eq!(
        std::fs::read(dir.join(".git/config")).unwrap_or_default(),
        config
    );
    assert_eq!(
        tab.release
            .as_ref()
            .and_then(|v| v.run.as_ref())
            .and_then(|r| r.hosting.as_ref())
            .map(|h| h.path.as_str()),
        Some("fork/project")
    );
    let _ = std::fs::remove_dir_all(dir);
    let _ = std::fs::remove_dir_all(origin);
    let _ = std::fs::remove_dir_all(fork);
}

#[test]
fn review10_dialog_preflight_refuses_before_bump_tag_or_push() {
    let _pause = crate::network::test_pause::set(Some(crate::network::Pause::Switch));
    let (dir, receiver) = project("publication-preflight");
    let before_head = git::head(&dir);
    let before_remote = git::resolve(&receiver, "refs/heads/main");
    run(
        &dir,
        &[
            "remote",
            "set-url",
            "origin",
            "https://github.com/selected/project",
        ],
    );
    run(
        &dir,
        &[
            "remote",
            "set-url",
            "--push",
            "origin",
            &receiver.to_string_lossy(),
        ],
    );
    assert_eq!(
        git::run(Some(&dir), &["remote", "get-url", "--push", "origin"])
            .map(|s| s.trim().to_owned()),
        Ok(receiver.to_string_lossy().into_owned())
    );
    let ctx = egui::Context::default();
    let mut tab = open(&dir, &ctx);
    assert!(tab.release.as_ref().is_some_and(|v| v.form.publish));
    tab.release_act(ReleaseAct::Run, &ctx);
    wait(&mut tab, &ctx, |t| t.busy_op.is_none() && !t.loading);
    let after_head = git::head(&dir);
    let local_tag_absent = git::resolve(&dir, "refs/tags/v2.3.2").is_err();
    let remote_tag_absent = git::resolve(&receiver, "refs/tags/v2.3.2").is_err();
    let remote_head = git::resolve(&receiver, "refs/heads/main");
    let done = tab
        .release
        .as_ref()
        .and_then(|v| v.run.as_ref())
        .map(|r| r.done.clone());
    let refused = tab
        .status
        .as_ref()
        .is_some_and(|n| n.1 && n.0.contains("does not match the selected forge repository"));
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_dir_all(&receiver);
    assert!(refused);
    assert_eq!(
        after_head, before_head,
        "preflight must run before a bump commit"
    );
    assert!(local_tag_absent && remote_tag_absent);
    assert_eq!(remote_head, before_remote);
    assert_eq!(done, Some(crate::release::Done::default()));
}
