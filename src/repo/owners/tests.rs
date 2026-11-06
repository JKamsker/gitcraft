//! CODEOWNERS in a tab, on a real repository: the rows' owners, the summary of a change, the
//! reviewers suggested for a pull request and the check of the file.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use super::{Suggested, asked, summary_text};
use crate::codeowners::Reviewer;
use crate::git;
use crate::repo::{Dialog, RepoTab};

#[test]
fn the_summary_says_who_a_change_needs() {
    let owners = ["@acme/payments".to_owned(), "@acme/infra".to_owned()];
    assert_eq!(
        summary_text(&owners, 0, 3).as_deref(),
        Some("Needs payments and infra")
    );
    assert_eq!(
        summary_text(&owners, 1, 3).as_deref(),
        Some("Needs payments and infra · 1 file without an owner")
    );
    assert_eq!(
        summary_text(&[], 2, 2).as_deref(),
        Some("None of them has an owner")
    );
    assert_eq!(summary_text(&[], 1, 1).as_deref(), Some("No owner"));
    assert_eq!(summary_text(&[], 0, 0), None);
}

#[test]
fn only_the_ticked_reviewers_are_asked() {
    let s = |name: &str, ask| Suggested {
        reviewer: Reviewer {
            name: name.to_owned(),
            team: name.contains('/'),
        },
        ask,
    };
    assert_eq!(
        asked(&[s("ann", true), s("acme/infra", false), s("bob", true)]),
        ["ann", "bob"]
    );
}

fn run(dir: &Path, args: &[&str]) {
    let out = git::run(Some(dir), args);
    assert!(out.is_ok(), "git {args:?}: {out:?}");
}

fn write(dir: &Path, path: &str, text: &str) {
    let file = dir.join(path);
    if let Some(parent) = file.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(file, text);
}

/// A repository with a GitHub CODEOWNERS, `main` and a branch `feat` changing payments.
fn repo(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "gitcraft owners-tab ü {name}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::create_dir_all(&dir);
    run(&dir, &["init", "-q", "-b", "main"]);
    run(&dir, &["config", "user.name", "t"]);
    run(&dir, &["config", "user.email", "t@t"]);
    write(
        &dir,
        ".github/CODEOWNERS",
        "/pay\\ ment/ @acme/payments\n/infra/ @acme/infra @ann\n/gone/ @old\n",
    );
    write(&dir, "pay ment/ü.rs", "1\n");
    write(&dir, "infra/main.tf", "1\n");
    write(&dir, "README.md", "1\n");
    run(&dir, &["add", "-A"]);
    run(&dir, &["commit", "-q", "-m", "base"]);
    run(&dir, &["checkout", "-q", "-b", "feat"]);
    write(&dir, "pay ment/ü.rs", "2\n");
    run(&dir, &["commit", "-q", "-am", "pay"]);
    dir
}

fn wait(tab: &mut RepoTab, ctx: &egui::Context, done: impl Fn(&RepoTab) -> bool) {
    let until = Instant::now() + Duration::from_secs(30);
    while !done(tab) && Instant::now() < until {
        std::thread::sleep(Duration::from_millis(20));
        tab.poll(ctx);
        tab.owners_frame(ctx);
    }
}

fn open(dir: &Path, ctx: &egui::Context) -> RepoTab {
    let mut tab = RepoTab::new_deferred(dir.to_path_buf(), ctx);
    tab.reload(ctx);
    wait(&mut tab, ctx, |tab| {
        !tab.loading && tab.owners.read_for == tab.history_gen && tab.history_gen > 0
    });
    tab
}

#[test]
fn rows_show_their_owners_and_the_file_is_read_again_when_it_changes() {
    let dir = repo("rows");
    let ctx = egui::Context::default();
    let mut tab = open(&dir, &ctx);
    let pay = tab.owner_tag("pay ment/ü.rs").map(|t| t.label);
    let infra = tab.owner_tag("infra/main.tf").map(|t| (t.label, t.full));
    let none = tab.owner_tag("README.md").map(|t| t.label);
    let needed = tab
        .owners
        .needed(["pay ment/ü.rs", "infra/main.tf", "README.md"]);
    // A new CODEOWNERS committed: read again on the next reload, the old answers dropped.
    write(&dir, ".github/CODEOWNERS", "* @everyone\n");
    run(&dir, &["commit", "-q", "-am", "owners"]);
    tab.reload(&ctx);
    wait(&mut tab, &ctx, |tab| {
        tab.owner_tag("README.md")
            .is_some_and(|t| t.label == "everyone")
    });
    let after = tab.owner_tag("README.md").map(|t| t.label);
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(pay.as_deref(), Some("payments"));
    assert_eq!(
        infra,
        Some(("infra +1".to_owned(), "@acme/infra, @ann".to_owned()))
    );
    assert_eq!(none, None);
    assert_eq!(
        needed,
        (
            vec![
                "@acme/payments".to_owned(),
                "@acme/infra".to_owned(),
                "@ann".to_owned()
            ],
            1
        )
    );
    assert_eq!(after.as_deref(), Some("everyone"));
}

#[test]
fn the_create_pull_request_dialog_suggests_the_owners_of_what_the_branch_changes() {
    let dir = repo("suggest");
    let ctx = egui::Context::default();
    let mut tab = open(&dir, &ctx);
    tab.dialog = Some(Dialog::create_pr("pull request", "feat", "main", "Pay"));
    wait(
        &mut tab,
        &ctx,
        |tab| matches!(&tab.dialog, Some(Dialog::CreatePr { reviewers, .. }) if !reviewers.is_empty()),
    );
    let suggested = match &tab.dialog {
        Some(Dialog::CreatePr { reviewers, .. }) => asked(reviewers),
        _ => Vec::new(),
    };
    // Another base, with nothing to compare: no suggestion, and no error in the dialog.
    if let Some(Dialog::CreatePr { base, .. }) = &mut tab.dialog {
        *base = "nope".to_owned();
    }
    tab.owners_frame(&ctx);
    wait(
        &mut tab,
        &ctx,
        |tab| matches!(&tab.dialog, Some(Dialog::CreatePr { reviewers, .. }) if reviewers.is_empty()),
    );
    let cleared =
        matches!(&tab.dialog, Some(Dialog::CreatePr { reviewers, .. }) if reviewers.is_empty());
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(suggested, ["acme/payments"]);
    assert!(cleared);
}

#[test]
fn the_check_lists_patterns_matching_nothing_and_files_without_an_owner() {
    let dir = repo("lint");
    let ctx = egui::Context::default();
    let mut tab = open(&dir, &ctx);
    tab.owners_act(super::OwnersAct::Check, &ctx);
    wait(&mut tab, &ctx, |tab| {
        tab.owners.lint.as_ref().is_some_and(|l| l.result.is_some())
    });
    let lint = tab
        .owners
        .lint
        .as_ref()
        .and_then(|l| l.result.clone())
        .and_then(Result::ok);
    let _ = std::fs::remove_dir_all(&dir);
    assert!(lint.is_some(), "no result");
    let Some(lint) = lint else { return };
    assert_eq!(lint.unmatched, [(3, "/gone/".to_owned())]);
    assert_eq!(lint.unowned, [".github/CODEOWNERS", "README.md"]);
}
