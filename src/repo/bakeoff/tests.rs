//! The bake-off through the tab, as its page drives it, on a real repository: compared, a
//! file and a hunk picked, assembled into a branch, an attempt built in a real terminal.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use super::BakeAct;
use crate::agent::bakeoff::Attempt;
use crate::agent::bakeoff::build::Built;
use crate::git;
use crate::repo::{Action, RepoTab};

fn git_ok(dir: &Path, args: &[&str]) {
    assert!(git::run(Some(dir), args).is_ok(), "git {args:?}");
}

/// `main` with `a.txt`; branch `one` changes its first line and adds `x.txt`; worktree `two`
/// changes its last line, uncommitted.
fn repo(name: &str) -> (PathBuf, PathBuf, PathBuf) {
    let root =
        std::env::temp_dir().join(format!("gitcraft-bake-tab-{name}-{} ö", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let dir = root.join("app");
    let _ = std::fs::create_dir_all(&dir);
    for args in [
        &["init", "-q", "-b", "main"][..],
        &["config", "user.name", "t"],
        &["config", "user.email", "t@t"],
        &["config", "commit.gpgSign", "false"],
        &["config", "core.autocrlf", "false"],
    ] {
        git_ok(&dir, args);
    }
    let _ = std::fs::write(dir.join("a.txt"), "a\nb\nc\nd\ne\n");
    git_ok(&dir, &["add", "-A"]);
    git_ok(&dir, &["commit", "-qm", "base"]);
    git_ok(&dir, &["checkout", "-q", "-b", "one"]);
    let _ = std::fs::write(dir.join("a.txt"), "A\nb\nc\nd\ne\n");
    let _ = std::fs::write(dir.join("x.txt"), "x\n");
    git_ok(&dir, &["add", "-A"]);
    git_ok(&dir, &["commit", "-qm", "one"]);
    git_ok(&dir, &["checkout", "-q", "main"]);
    let wt = root.join("two");
    git_ok(
        &dir,
        &[
            "worktree",
            "add",
            "-q",
            "-b",
            "two",
            &wt.display().to_string(),
        ],
    );
    let _ = std::fs::write(wt.join("a.txt"), "a\nb\nc\nd\nE\n");
    (root, dir, wt)
}

fn pump(tab: &mut RepoTab, ctx: &egui::Context, done: impl Fn(&RepoTab) -> bool) -> bool {
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(60) {
        tab.poll(ctx);
        if let Some(view) = &tab.bakeoff.view
            && view.builds.contains(&Some(Built::Running))
        {
            tab.poll_builds(ctx);
        }
        if done(tab) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    false
}

#[test]
fn a_branch_is_assembled_from_the_page_and_an_attempt_is_built() {
    let (root, dir, wt) = repo("page");
    let ctx = egui::Context::default();
    let mut tab = RepoTab::new_deferred(dir.clone(), &ctx);
    let attempts = vec![
        Attempt {
            name: "one".to_owned(),
            rev: "one".to_owned(),
            worktree: None,
        },
        Attempt {
            name: "two".to_owned(),
            rev: "two".to_owned(),
            worktree: Some(wt.clone()),
        },
    ];
    tab.apply(Action::Bake(BakeAct::Compare(attempts)), &ctx);
    let compared = pump(&mut tab, &ctx, |tab| {
        tab.bakeoff
            .view
            .as_ref()
            .is_some_and(|v| v.comparison.is_some())
    });
    tab.apply(Action::Bake(BakeAct::OpenFile("a.txt".to_owned())), &ctx);
    let opened = pump(&mut tab, &ctx, |tab| {
        tab.bakeoff
            .view
            .as_ref()
            .is_some_and(|v| matches!(&v.file, Some((_, Some(Ok(_))))))
    });
    for act in [
        BakeAct::ToggleHunk("a.txt".to_owned(), 0, 0),
        BakeAct::ToggleHunk("a.txt".to_owned(), 1, 0),
        BakeAct::TakeFile("x.txt".to_owned(), 0),
    ] {
        tab.apply(Action::Bake(act), &ctx);
    }
    if let Some(view) = &mut tab.bakeoff.view {
        view.branch = "bakeoff/best ü".to_owned();
    }
    // Not a branch name: said, nothing made.
    tab.apply(Action::Bake(BakeAct::Assemble), &ctx);
    let refused = pump(&mut tab, &ctx, |tab| {
        tab.bakeoff
            .view
            .as_ref()
            .is_some_and(|v| v.note.as_ref().is_some_and(|(_, e)| *e))
    });
    if let Some(view) = &mut tab.bakeoff.view {
        view.branch = "bakeoff/best".to_owned();
    }
    tab.apply(Action::Bake(BakeAct::Assemble), &ctx);
    let made = pump(&mut tab, &ctx, |tab| {
        tab.bakeoff
            .view
            .as_ref()
            .is_some_and(|v| v.note.as_ref().is_some_and(|(_, e)| !*e))
    });
    let a = git::run(Some(&dir), &["show", "bakeoff/best:a.txt"]).unwrap_or_default();
    let x = git::run(Some(&dir), &["show", "bakeoff/best:x.txt"]).unwrap_or_default();

    // A real build in a terminal of the worktree: its status comes back to the grid.
    if let Some(view) = &mut tab.bakeoff.view {
        view.build_command = "test -f a.txt".to_owned();
    }
    tab.apply(Action::Bake(BakeAct::Build(1)), &ctx);
    let built = pump(&mut tab, &ctx, |tab| {
        tab.bakeoff
            .view
            .as_ref()
            .is_some_and(|v| v.builds.get(1) == Some(&Some(Built::Passed)))
    });
    // Taking the file taken already takes it back; closing with picks asks to close again.
    tab.apply(Action::Bake(BakeAct::TakeFile("x.txt".to_owned(), 0)), &ctx);
    let untaken = tab
        .bakeoff
        .view
        .as_ref()
        .is_some_and(|v| !v.picks.contains_key("x.txt"));
    tab.apply(Action::Bake(BakeAct::Close), &ctx);
    let kept_once = tab.bakeoff.view.is_some();
    tab.apply(Action::Bake(BakeAct::Close), &ctx);
    let closed = tab.bakeoff.view.is_none();
    let terminal_error = tab.terminal_error.clone();
    tab.terminals.clear();
    let note = tab.bakeoff.view.as_ref().and_then(|v| v.note.clone());
    let _ = std::fs::remove_dir_all(&root);
    assert!(compared && opened, "{note:?}");
    assert!(refused, "{note:?}");
    assert!(made, "{note:?}");
    assert_eq!(a, "A\nb\nc\nd\nE\n");
    assert_eq!(x, "x\n");
    assert!(built, "{terminal_error:?} {note:?}");
    assert!(untaken);
    assert!(kept_once && closed);
}
