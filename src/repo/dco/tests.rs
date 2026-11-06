//! The sign-off against real repositories: finding the rules, committing signed off, and signing
//! off a pushed branch, then undoing and redoing that, here and on a bare remote.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use super::signoff::{self, Rules};
use crate::git;
use crate::repo::operations::run_steps;
use crate::repo::{OpKind, RepoTab, Undo, UndoEntry};

/// A fresh folder named after `name`, with a space and some unicode in it.
fn folder(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("gitcraft dco-ü {name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::create_dir_all(&dir);
    dir
}

fn run(dir: &Path, args: &[&str]) -> String {
    let out = git::run(Some(dir), args);
    assert!(out.is_ok(), "git {args:?} failed: {out:?}");
    out.unwrap_or_default()
}

/// A repository with one commit on `main`, by Zoë.
fn repo(name: &str) -> PathBuf {
    let dir = folder(name);
    for args in [
        &["init", "-q", "-b", "main"][..],
        &["config", "user.name", "Zoë Ångström"],
        &["config", "user.email", "zoe@example.org"],
        &["commit", "-q", "--allow-empty", "-m", "base"],
    ] {
        run(&dir, args);
    }
    dir
}

/// Commits a file on the branch checked out, without a sign-off unless `signed`.
fn commit(dir: &Path, file: &str, signed: bool) {
    let _ = std::fs::write(dir.join(file), format!("{file}\n"));
    run(dir, &["add", "--", file]);
    let mut args = vec!["commit", "-q", "-m"];
    let message = format!("add {file}");
    args.push(&message);
    if signed {
        args.push("--signoff");
    }
    run(dir, &args);
}

#[test]
fn the_rules_come_from_a_committed_dco_file_and_the_kept_choice() {
    let dir = repo("rules");
    let none = signoff::read_rules(&dir);
    std::fs::create_dir_all(dir.join(".github")).ok();
    let _ = std::fs::write(dir.join(".github/dco.yml"), "require:\n  members: false\n");
    // Not committed yet: not the repository's rule.
    let uncommitted = signoff::read_rules(&dir);
    run(&dir, &["add", "--", ".github/dco.yml"]);
    run(&dir, &["commit", "-q", "-m", "dco"]);
    let app = signoff::read_rules(&dir);
    let _ = std::fs::write(dir.join("DCO"), "Developer Certificate of Origin\n");
    run(&dir, &["add", "DCO"]);
    run(&dir, &["commit", "-q", "-m", "certificate"]);
    let both = signoff::read_rules(&dir);
    let off = signoff::choice_command(false);
    let off: Vec<&str> = off.iter().map(String::as_str).collect();
    run(&dir, &off);
    let chosen = signoff::read_rules(&dir);
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(none, Rules::default());
    assert_eq!(uncommitted.file, None);
    assert_eq!(app.file.as_deref(), Some(".github/dco.yml"));
    assert_eq!(both.file.as_deref(), Some("DCO"), "the certificate first");
    assert_eq!(chosen.choice, Some(false));
}

#[test]
fn a_repository_without_commits_has_no_rules() {
    let dir = folder("empty");
    run(&dir, &["init", "-q"]);
    let rules = signoff::read_rules(&dir);
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(rules, Rules::default());
}

/// Waits for the tab's background work: the operation running, and what it reads after.
fn settle(tab: &mut RepoTab, ctx: &egui::Context) {
    let until = Instant::now() + Duration::from_secs(20);
    while (tab.busy_op.is_some() || tab.loading || tab.dco.reading) && Instant::now() < until {
        std::thread::sleep(Duration::from_millis(20));
        tab.poll(ctx);
    }
}

#[test]
fn the_commit_box_signs_off_where_a_dco_file_asks_and_as_chosen() {
    let dir = repo("commit-box");
    let _ = std::fs::write(dir.join("DCO.md"), "certificate\n");
    run(&dir, &["add", "DCO.md"]);
    run(&dir, &["commit", "-q", "-m", "dco"]);
    let ctx = egui::Context::default();
    let mut tab = RepoTab::new_deferred(dir.clone(), &ctx);
    tab.reload(&ctx);
    settle(&mut tab, &ctx);
    tab.dco_frame(&ctx);
    settle(&mut tab, &ctx);
    let auto = tab.signs_off();
    let _ = std::fs::write(dir.join("a.txt"), "a\n");
    tab.commit_summary = "feat: a".to_owned();
    tab.reload_status(&ctx);
    settle(&mut tab, &ctx);
    tab.commit(&ctx);
    settle(&mut tab, &ctx);
    let first = run(&dir, &["log", "-1", "--format=%B"]);
    // Turned off by hand: kept in the repository's config, and the next commit isn't.
    tab.dco_act(super::DcoAct::SetSignOff(false), &ctx);
    std::thread::sleep(Duration::from_millis(300));
    tab.poll(&ctx);
    let _ = std::fs::write(dir.join("b.txt"), "b\n");
    tab.commit_summary = "feat: b".to_owned();
    tab.reload_status(&ctx);
    settle(&mut tab, &ctx);
    tab.commit(&ctx);
    settle(&mut tab, &ctx);
    let second = run(&dir, &["log", "-1", "--format=%B"]);
    let kept = signoff::read_rules(&dir).choice;
    let _ = std::fs::remove_dir_all(&dir);
    assert!(auto, "on by itself with a DCO.md");
    assert!(
        first.contains("Signed-off-by: Zoë Ångström <zoe@example.org>"),
        "{first}"
    );
    assert!(!second.contains("Signed-off-by"), "{second}");
    assert_eq!(kept, Some(false));
}

/// A repository with `feat` pushed to a bare `origin`: one commit signed off, two not.
fn pushed_branch(name: &str) -> (PathBuf, PathBuf) {
    let dir = repo(name);
    let bare = folder(&format!("{name}-remote.git"));
    run(&bare, &["init", "-q", "--bare"]);
    let bare_arg = bare.to_string_lossy().into_owned();
    run(&dir, &["remote", "add", "origin", &bare_arg]);
    run(&dir, &["push", "-q", "-u", "origin", "main"]);
    run(&dir, &["checkout", "-q", "-b", "feat"]);
    commit(&dir, "one file.txt", false);
    commit(&dir, "twö.txt", true);
    commit(&dir, "three.txt", false);
    run(&dir, &["push", "-q", "-u", "origin", "feat"]);
    (dir, bare)
}

fn entry(kind: OpKind) -> Option<UndoEntry> {
    match kind {
        OpKind::Normal(Undo::Custom { undo, redo }, _) => Some(UndoEntry {
            label: "sign off".to_owned(),
            undo,
            redo,
            heads: None,
        }),
        _ => None,
    }
}

fn missing_on(dir: &Path, rev: &str) -> usize {
    let list = signoff::commits(dir, &format!("main..{rev}")).unwrap_or_default();
    signoff::missing(&list).count()
}

#[test]
fn signing_off_a_branch_rewrites_and_pushes_it_and_undo_puts_it_back_everywhere() {
    let (dir, bare) = pushed_branch("fix");
    let before = git::head(&dir);
    let plan = signoff::plan(&dir, "main");
    assert!(plan.is_ok(), "{plan:?}");
    let Ok(plan) = plan else { return };
    let preview = super::fix_preview(&plan, "Zoë Ångström <zoe@example.org>", "zoe@example.org");
    let kind = OpKind::Normal(
        Undo::Custom {
            undo: plan.undo_steps(),
            redo: plan.redo_steps(),
        },
        String::new(),
    );
    let (done, kind) = run_steps(&dir, &plan.steps(), kind, None);
    let signed = git::head(&dir);
    let remote_after = git::resolve(&bare, "refs/heads/feat").unwrap_or_default();
    let missing_after = missing_on(&dir, "HEAD");
    let entry = entry(kind);
    let undone = entry
        .clone()
        .map(|e| run_steps(&dir, &e.undo.clone(), OpKind::Undo(e), None));
    let head_undone = git::head(&dir);
    let remote_undone = git::resolve(&bare, "refs/heads/feat").unwrap_or_default();
    let redone = undone.and_then(|(r, kind)| match kind {
        OpKind::Undo(e) if r.is_ok() => {
            Some(run_steps(&dir, &e.redo.clone(), OpKind::Redo(e), None))
        }
        _ => None,
    });
    let remote_redone = git::resolve(&bare, "refs/heads/feat").unwrap_or_default();
    let head_redone = git::head(&dir);
    let missing_redone = missing_on(&dir, "HEAD");
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_dir_all(&bare);
    assert_eq!(plan.unsigned().len(), 2);
    assert!(preview.contains("2 of them had no sign-off"), "{preview}");
    assert!(preview.contains("origin/feat"), "{preview}");
    assert!(done.is_ok(), "{done:?}");
    assert_ne!(signed, before);
    assert_eq!(remote_after, signed);
    assert_eq!(missing_after, 0);
    assert_eq!(head_undone, before);
    assert_eq!(remote_undone, before);
    assert!(matches!(redone, Some((Ok(_), _))), "{redone:?}");
    assert_eq!(missing_redone, 0);
    assert_eq!(remote_redone, head_redone);
    assert_ne!(head_redone, before);
}

#[test]
fn the_push_is_refused_when_someone_pushed_since_and_nothing_is_lost_there() {
    let (dir, bare) = pushed_branch("lease");
    let plan = signoff::plan(&dir, "main");
    // Someone else pushes to feat meanwhile (from a clone of their own).
    let other = folder("lease-other");
    let bare_arg = bare.to_string_lossy().into_owned();
    let other_arg = other.to_string_lossy().into_owned();
    run(
        &std::env::temp_dir(),
        &["clone", "-q", "-b", "feat", &bare_arg, &other_arg],
    );
    run(&other, &["config", "user.name", "Bob"]);
    run(&other, &["config", "user.email", "bob@example.org"]);
    commit(&other, "theirs.txt", true);
    run(&other, &["push", "-q", "origin", "feat"]);
    let theirs = git::head(&other);
    let result = plan
        .as_ref()
        .map(|p| run_steps(&dir, &p.steps(), OpKind::plain(), None).0);
    let remote = git::resolve(&bare, "refs/heads/feat").unwrap_or_default();
    for d in [&dir, &bare, &other] {
        let _ = std::fs::remove_dir_all(d);
    }
    assert!(matches!(result, Ok(Err(_))), "{result:?}");
    assert_eq!(remote, theirs);
}

#[test]
fn a_branch_that_cannot_be_signed_off_that_way_is_refused_with_why() {
    let dir = repo("refused");
    run(&dir, &["checkout", "-q", "-b", "feat"]);
    // Nothing of its own yet.
    let empty = signoff::plan(&dir, "main");
    commit(&dir, "signed.txt", true);
    let all_signed = signoff::plan(&dir, "main");
    // A merge in it.
    run(&dir, &["checkout", "-q", "-b", "side", "main"]);
    commit(&dir, "side.txt", false);
    run(&dir, &["checkout", "-q", "feat"]);
    run(&dir, &["merge", "-q", "--no-edit", "side"]);
    let merges = signoff::plan(&dir, "main");
    // Detached.
    run(&dir, &["checkout", "-q", "--detach"]);
    let detached = signoff::plan(&dir, "main");
    let _ = std::fs::remove_dir_all(&dir);
    assert!(empty.is_err_and(|e| e.contains("no commits of its own")));
    assert!(all_signed.is_err_and(|e| e.contains("already signed off")));
    assert!(merges.is_err_and(|e| e.contains("merge commits")));
    assert!(detached.is_err_and(|e| e.contains("detached")));
}

#[test]
fn a_branch_without_a_remote_is_signed_off_here_only() {
    let dir = repo("local-only");
    run(&dir, &["checkout", "-q", "-b", "feat"]);
    commit(&dir, "x.txt", false);
    let plan = signoff::plan(&dir, "main");
    let push = plan.as_ref().map(|p| p.push.clone());
    let steps = plan.as_ref().map(|p| p.steps()).unwrap_or_default();
    let (result, _) = run_steps(&dir, &steps, OpKind::plain(), None);
    let missing = missing_on(&dir, "HEAD");
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(push, Ok(None));
    assert!(!steps.iter().any(|s| s.first().is_some_and(|a| a == "push")));
    assert!(result.is_ok(), "{result:?}");
    assert_eq!(missing, 0);
}
