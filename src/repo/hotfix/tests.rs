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
fn a_hotfix_started_from_a_tag_is_finished_pushed_undone_and_redone() -> Result<(), String> {
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
    // Automatic atomic multi-ref publication is deliberately refused by this checklist.
    // Complete the existing per-ref review flow for each exact resulting object instead.
    wait(&mut tab, &ctx, |t| {
        matches!(t.dialog, Some(Dialog::PushError(_)))
    });
    assert!(matches!(&tab.dialog, Some(Dialog::PushError(e)) if e.contains("atomic")));
    assert_eq!(
        git::resolve(&bare, "refs/heads/main").ok().as_deref(),
        Some(main_before.as_str())
    );
    assert_eq!(
        git::resolve(&bare, "refs/heads/develop").ok().as_deref(),
        Some(develop_before.as_str())
    );
    assert!(git::resolve(&bare, "refs/tags/v2.3.2").is_err());
    tab.dialog = None;
    for reference in ["refs/heads/main", "refs/heads/develop", "refs/tags/v2.3.2"] {
        tab.run_ops(
            "Push reviewed hotfix ref",
            vec![vec![
                "push".into(),
                "origin".into(),
                format!("{reference}:{reference}"),
            ]],
            crate::repo::Refresh::All,
            crate::repo::OpKind::plain(),
            &ctx,
        );
        crate::repo::push_preflight::test_support::approve(&mut tab, &ctx)?;
        wait(&mut tab, &ctx, |t| t.busy_op.is_none() && !t.loading);
        assert!(
            tab.status.as_ref().is_some_and(|(_, failed)| !failed),
            "{:?}",
            tab.status
        );
    }
    let current = git::run(Some(&dir), &["branch", "--show-current"]).unwrap_or_default();
    let tag_at = git::resolve(&dir, "v2.3.2").ok();
    let tag_object = git::run(Some(&dir), &["rev-parse", "refs/tags/v2.3.2"]).unwrap_or_default();
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
    // The very tag and merges that were pushed, not new ones.
    let redone_tag = git::run(Some(&dir), &["rev-parse", "refs/tags/v2.3.2"]).unwrap_or_default();
    let redone_main = git::resolve(&dir, "refs/heads/main").ok();
    let redone_branch = git::run(Some(&dir), &["branch", "--show-current"]).unwrap_or_default();
    let redone_gone = git::resolve(&dir, "refs/heads/hotfix/2.3.2").is_err();
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_dir_all(&bare);
    assert_eq!(named, Some(("hotfix/2.3.2".to_owned(), tag_target)));
    assert!(
        choices
            .as_ref()
            .is_some_and(|c| c.tag == "v2.3.2" && c.develop && c.notes.contains("the crash"))
    );
    assert_eq!(current.trim(), "main");
    // On main's merge, as git-flow tags it.
    assert_eq!(tag_at, local_main);
    assert!(!tag_object.trim().is_empty());
    assert!(in_main && in_develop && gone);
    assert_eq!(remote_main, local_main);
    assert!(remote_tag.is_some());
    assert_eq!(undone_branch.trim(), "hotfix/2.3.2");
    assert_eq!(main_back.as_deref(), Some(main_before.as_str()));
    assert_eq!(develop_back.as_deref(), Some(develop_before.as_str()));
    assert!(tag_gone);
    assert_eq!(redone_tag.trim(), tag_object.trim());
    assert_eq!(redone_main, local_main);
    assert_eq!(redone_branch.trim(), "main");
    assert!(redone_gone);
    Ok(())
}

#[test]
fn a_finish_that_stops_part_way_is_undoable() {
    let (dir, bare) = project("stopped");
    run(&dir, &["checkout", "-q", "-b", "hotfix/2.3.2", "v2.3.1"]);
    commit(&dir, "app.txt", "one\nTWO\n", "fix: two");
    run(&dir, &["branch", "--set-upstream-to=origin/main"]);
    // Checking main out fails on this file, after develop is merged into.
    let _ = std::fs::write(dir.join("only-main.txt"), "mine\n");
    run(&dir, &["checkout", "-q", "main"]);
    commit(&dir, "only-main.txt", "main's\n", "chore: main's file");
    run(&dir, &["checkout", "-q", "hotfix/2.3.2"]);
    let _ = std::fs::write(dir.join("only-main.txt"), "in the way\n");
    let main_before = git::resolve(&dir, "refs/heads/main").unwrap_or_default();
    let develop_before = git::resolve(&dir, "refs/heads/develop").unwrap_or_default();
    let ctx = egui::Context::default();
    let mut tab = RepoTab::new_deferred(dir.clone(), &ctx);
    tab.reload(&ctx);
    settle(&mut tab, &ctx);
    tab.hotfix_act(HotfixAct::OpenFinish, &ctx);
    wait(&mut tab, &ctx, |t| {
        t.hotfix.finish.as_ref().is_some_and(|f| f.plan.is_some())
    });
    let planned = tab
        .hotfix
        .finish
        .as_ref()
        .is_some_and(|f| matches!(f.plan, Some(Ok(_))));
    if let Some(f) = &mut tab.hotfix.finish {
        f.choices.push = false;
    }
    tab.hotfix_act(HotfixAct::Finish, &ctx);
    settle(&mut tab, &ctx);
    wait(&mut tab, &ctx, |t| {
        t.timeline
            .current_entry()
            .and_then(|e| e.undo.as_ref())
            .is_some_and(|e| e.label.contains("stopped part-way"))
    });
    let develop_merged =
        git::resolve(&dir, "refs/heads/develop").ok() != Some(develop_before.clone());
    let status = tab.status.clone().map(|(s, _)| s).unwrap_or_default();
    tab.undo(&ctx);
    settle(&mut tab, &ctx);
    let branch = git::run(Some(&dir), &["branch", "--show-current"]).unwrap_or_default();
    let develop_back = git::resolve(&dir, "refs/heads/develop").ok();
    let main_back = git::resolve(&dir, "refs/heads/main").ok();
    let no_tag = git::resolve(&dir, "refs/tags/v2.3.2").is_err();
    let upstream =
        git::run(Some(&dir), &["config", "branch.hotfix/2.3.2.merge"]).unwrap_or_default();
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_dir_all(&bare);
    assert!(planned);
    assert!(develop_merged);
    assert!(status.contains("Ctrl+Z puts back what it did"), "{status}");
    assert_eq!(branch.trim(), "hotfix/2.3.2");
    assert_eq!(develop_back.as_deref(), Some(develop_before.as_str()));
    assert_eq!(main_back.as_deref(), Some(main_before.as_str()));
    assert!(no_tag);
    assert_eq!(upstream.trim(), "refs/heads/main");
}

#[test]
fn a_branch_checked_out_in_another_worktree_is_refused() {
    let (dir, bare) = project("worktree");
    run(&dir, &["checkout", "-q", "-b", "hotfix/2.3.2", "v2.3.1"]);
    commit(&dir, "app.txt", "one\nTWO\n", "fix: two");
    let other = folder("worktree-develop");
    let _ = std::fs::remove_dir_all(&other);
    run(
        &dir,
        &["worktree", "add", "-q", &other.to_string_lossy(), "develop"],
    );
    let plan = plan::plan(&dir, None);
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_dir_all(&bare);
    let _ = std::fs::remove_dir_all(&other);
    assert!(plan.is_err_and(|e| e.contains("develop is checked out in another worktree")));
}

#[test]
fn an_unticked_signature_is_not_made_where_tags_are_signed_by_default() {
    let (dir, bare) = project("unsigned");
    run(&dir, &["checkout", "-q", "-b", "hotfix/2.3.2", "v2.3.1"]);
    commit(&dir, "app.txt", "one\nTWO\n", "fix: two");
    run(&dir, &["config", "tag.gpgSign", "true"]);
    run(&dir, &["config", "gpg.program", "false"]);
    let plan = plan::plan(&dir, None);
    assert!(plan.is_ok(), "{plan:?}");
    let Ok(plan) = plan else { return };
    let c = Choices {
        tag: plan.tag.clone(),
        notes: String::new(),
        develop: false,
        delete: false,
        push: false,
        sign: false,
    };
    assert!(plan.sign);
    let steps = plan.steps(&c);
    let mut ok = true;
    for step in steps.iter().filter(|s| !s[0].starts_with('\u{0}')) {
        let args: Vec<&str> = step.iter().map(String::as_str).collect();
        ok &= git::run(Some(&dir), &args).is_ok();
    }
    let tagged = git::resolve(&dir, "refs/tags/v2.3.2").is_ok();
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_dir_all(&bare);
    assert!(ok && tagged);
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
