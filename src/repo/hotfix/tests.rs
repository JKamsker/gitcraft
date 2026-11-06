//! The hotfix flow against real repositories with a bare remote: started from a tag, finished
//! into main and develop and pushed, undone and redone; refused where it would conflict.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use super::HotfixAct;
use super::plan::{self, Choices};
use crate::git;
use crate::repo::{Dialog, RepoTab};

fn folder(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("gitcraft hotfix-ü {name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::create_dir_all(&dir);
    dir
}

fn run(dir: &Path, args: &[&str]) {
    let out = git::run(Some(dir), args);
    assert!(out.is_ok(), "git {args:?}: {out:?}");
}

fn commit(dir: &Path, file: &str, text: &str, message: &str) {
    let _ = std::fs::write(dir.join(file), text);
    run(dir, &["add", "--", file]);
    run(dir, &["commit", "-q", "-m", message]);
}

/// `main` released as v2.3.1, `develop` ahead of it; pushed to a bare `origin`.
fn project(name: &str) -> (PathBuf, PathBuf) {
    let dir = folder(name);
    let bare = folder(&format!("{name}-origin.git"));
    run(&bare, &["init", "-q", "--bare"]);
    run(&dir, &["init", "-q", "-b", "main"]);
    for (k, v) in [
        ("user.name", "Zoë"),
        ("user.email", "zoe@example.org"),
        ("tag.gpgSign", "false"),
        ("commit.gpgSign", "false"),
    ] {
        run(&dir, &["config", k, v]);
    }
    commit(&dir, "app.txt", "one\ntwo\n", "feat: start");
    run(&dir, &["tag", "-a", "v2.3.1", "-m", "v2.3.1"]);
    run(&dir, &["checkout", "-q", "-b", "develop"]);
    commit(&dir, "next.txt", "next\n", "feat: next");
    run(&dir, &["checkout", "-q", "main"]);
    run(&dir, &["remote", "add", "origin", &bare.to_string_lossy()]);
    run(
        &dir,
        &["push", "-q", "-u", "origin", "main", "develop", "v2.3.1"],
    );
    (dir, bare)
}

fn wait(tab: &mut RepoTab, ctx: &egui::Context, done: impl Fn(&RepoTab) -> bool) {
    let until = Instant::now() + Duration::from_secs(30);
    while !done(tab) && Instant::now() < until {
        std::thread::sleep(Duration::from_millis(20));
        tab.poll(ctx);
    }
}

fn settle(tab: &mut RepoTab, ctx: &egui::Context) {
    wait(tab, ctx, |t| t.busy_op.is_none() && !t.loading);
}

#[test]
fn a_hotfix_started_from_a_tag_is_finished_pushed_undone_and_redone() {
    let (dir, bare) = project("flow");
    let ctx = egui::Context::default();
    let mut tab = RepoTab::new_deferred(dir.clone(), &ctx);
    tab.reload(&ctx);
    settle(&mut tab, &ctx);
    tab.hotfix_frame(&ctx);
    // Started from the tag: the branch dialog, named for the next patch.
    let tag_target = git::resolve(&dir, "v2.3.1").unwrap_or_default();
    tab.hotfix_act(
        HotfixAct::Start {
            tag: "v2.3.1".into(),
            target: tag_target.clone(),
        },
        &ctx,
    );
    let named = match &tab.dialog {
        Some(Dialog::CreateBranch { name, at, .. }) => Some((name.clone(), at.clone())),
        _ => None,
    };
    run(&dir, &["checkout", "-q", "-b", "hotfix/2.3.2", "v2.3.1"]);
    commit(&dir, "app.txt", "one\nTWO\n", "fix: the crash (#7)");
    tab.dialog = None;
    tab.reload(&ctx);
    settle(&mut tab, &ctx);
    let main_before = git::resolve(&dir, "refs/heads/main").unwrap_or_default();
    let develop_before = git::resolve(&dir, "refs/heads/develop").unwrap_or_default();
    let hotfix_head = git::head(&dir);
    tab.hotfix_act(HotfixAct::OpenFinish, &ctx);
    wait(&mut tab, &ctx, |t| {
        t.hotfix.finish.as_ref().is_some_and(|f| f.plan.is_some())
    });
    let choices = tab.hotfix.finish.as_ref().map(|f| f.choices.clone());
    tab.hotfix_act(HotfixAct::Finish, &ctx);
    settle(&mut tab, &ctx);
    // Then the push, which follows.
    settle(&mut tab, &ctx);
    let current = git::run(Some(&dir), &["branch", "--show-current"]).unwrap_or_default();
    let tag_at = git::resolve(&dir, "v2.3.2").ok();
    let in_main = git::run(
        Some(&dir),
        &["merge-base", "--is-ancestor", &hotfix_head, "main"],
    )
    .is_ok();
    let in_develop = git::run(
        Some(&dir),
        &["merge-base", "--is-ancestor", &hotfix_head, "develop"],
    )
    .is_ok();
    let gone = git::resolve(&dir, "refs/heads/hotfix/2.3.2").is_err();
    let remote_main = git::resolve(&bare, "refs/heads/main").ok();
    let remote_tag = git::resolve(&bare, "refs/tags/v2.3.2").ok();
    let local_main = git::resolve(&dir, "refs/heads/main").ok();
    // Undone here: the hotfix branch back and checked out, main and develop back, no tag.
    tab.undo(&ctx);
    settle(&mut tab, &ctx);
    let undone_branch = git::run(Some(&dir), &["branch", "--show-current"]).unwrap_or_default();
    let main_back = git::resolve(&dir, "refs/heads/main").ok();
    let develop_back = git::resolve(&dir, "refs/heads/develop").ok();
    let tag_gone = git::resolve(&dir, "refs/tags/v2.3.2").is_err();
    tab.redo(&ctx);
    settle(&mut tab, &ctx);
    let redone = git::resolve(&dir, "refs/tags/v2.3.2").is_ok()
        && git::run(
            Some(&dir),
            &["merge-base", "--is-ancestor", &hotfix_head, "main"],
        )
        .is_ok();
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_dir_all(&bare);
    assert_eq!(named, Some(("hotfix/2.3.2".to_owned(), tag_target)));
    assert!(
        choices
            .as_ref()
            .is_some_and(|c| c.tag == "v2.3.2" && c.develop && c.notes.contains("the crash"))
    );
    assert_eq!(current.trim(), "main");
    assert_eq!(tag_at.as_deref(), Some(hotfix_head.as_str()));
    assert!(in_main && in_develop && gone);
    assert_eq!(remote_main, local_main);
    assert!(remote_tag.is_some());
    assert_eq!(undone_branch.trim(), "hotfix/2.3.2");
    assert_eq!(main_back.as_deref(), Some(main_before.as_str()));
    assert_eq!(develop_back.as_deref(), Some(develop_before.as_str()));
    assert!(tag_gone);
    assert!(redone);
}

#[test]
fn a_hotfix_that_would_conflict_is_refused_beforehand() {
    let (dir, bare) = project("conflict");
    run(&dir, &["checkout", "-q", "develop"]);
    commit(&dir, "app.txt", "one\ndevelop's two\n", "feat: other two");
    run(&dir, &["checkout", "-q", "-b", "hotfix/2.3.2", "v2.3.1"]);
    commit(&dir, "app.txt", "one\nhotfix two\n", "fix: two");
    let plan = plan::plan(&dir, Some("origin".into()));
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_dir_all(&bare);
    assert!(plan.is_ok(), "{plan:?}");
    let Ok(plan) = plan else { return };
    let mut c = Choices {
        tag: plan.tag.clone(),
        notes: String::new(),
        develop: true,
        delete: true,
        push: false,
        sign: false,
    };
    // git older than 2.38 can't tell: then nothing is refused beforehand.
    if plan.conflicts.is_some() {
        assert!(
            plan.problem(&c)
                .is_some_and(|p| p.contains("develop") && p.contains("app.txt"))
        );
        c.develop = false;
        assert_eq!(plan.problem(&c), None);
    }
}

#[test]
fn finishing_is_refused_off_a_hotfix_branch_or_with_changes() {
    let (dir, bare) = project("refused");
    let off = plan::plan(&dir, None);
    run(&dir, &["checkout", "-q", "-b", "hotfix/2.3.2"]);
    let empty = plan::plan(&dir, None);
    commit(&dir, "x.txt", "x\n", "fix: x");
    let _ = std::fs::write(dir.join("app.txt"), "changed\n");
    let dirty = plan::plan(&dir, None);
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_dir_all(&bare);
    assert!(off.is_err_and(|e| e.contains("hotfix branch")));
    assert!(empty.is_err_and(|e| e.contains("no commits of its own")));
    assert!(dirty.is_err_and(|e| e.contains("Commit or stash")));
}

#[test]
fn git_flows_names_are_used() {
    let (dir, bare) = project("gitflow");
    run(&dir, &["branch", "production"]);
    run(&dir, &["config", "gitflow.branch.master", "production"]);
    run(&dir, &["config", "gitflow.prefix.hotfix", "hf-"]);
    let names = plan::names(&dir);
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_dir_all(&bare);
    assert_eq!(names.main.as_deref(), Some("production"));
    assert_eq!(names.develop.as_deref(), Some("develop"));
    assert_eq!(names.prefix, "hf-");
}
