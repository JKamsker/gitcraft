//! A detached HEAD on real repositories, through the tab: the banner, the branch to go back
//! to, and a checkout that would leave commits on no branch.

use std::path::{Path, PathBuf};
use std::time::Duration;

use super::{DetachedAct, Leave, banner};
use crate::git;
use crate::repo::{Action, Dialog, RepoTab};

fn git_ok(dir: &Path, args: &[&str]) {
    let ran = git::run(Some(dir), args);
    assert!(ran.is_ok(), "{args:?}: {ran:?}");
}

/// `main` with two commits and `feature/ünï` from the first, in a folder with a space in its
/// name; HEAD detached at `main~1`.
fn detached_repo(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("gitcraft-detached {name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::create_dir_all(&dir);
    for args in [
        &["init", "-q", "-b", "main"][..],
        &["config", "user.name", "t"],
        &["config", "user.email", "t@t"],
        &["commit", "-q", "--allow-empty", "-m", "base"],
        &["branch", "feature/ünï"],
        &["commit", "-q", "--allow-empty", "-m", "two"],
        &["switch", "-q", "--detach", "main~1"],
    ] {
        git_ok(&dir, args);
    }
    dir
}

/// Polls `tab` until `done` holds (or a few seconds have gone), applying what it queued for
/// the next frame as a frame would.
fn wait(tab: &mut RepoTab, ctx: &egui::Context, done: impl Fn(&RepoTab) -> bool) -> bool {
    for _ in 0..400 {
        tab.poll(ctx);
        for act in std::mem::take(&mut tab.queued) {
            tab.apply(act, ctx);
        }
        if done(tab) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    false
}

/// A tab on `dir`, read, with its detached HEAD's banner read too (when detached).
fn opened(dir: &Path, ctx: &egui::Context) -> RepoTab {
    let mut tab = RepoTab::new(dir.to_owned(), ctx);
    let read = wait(&mut tab, ctx, |t| {
        !t.loading && !t.refs.head.is_empty() && (!t.is_detached() || t.detached.info.is_some())
    });
    assert!(read, "the tab never read the repository");
    tab
}

fn symbolic_head(dir: &Path) -> String {
    git::run_ok_codes(Some(dir), &["symbolic-ref", "-q", "HEAD"], &[0, 1])
        .unwrap_or_default()
        .trim()
        .to_owned()
}

#[test]
fn the_banner_shows_while_detached_and_offers_the_branch_left() {
    let dir = detached_repo("banner");
    let ctx = egui::Context::default();
    let tab = opened(&dir, &ctx);
    let (shows, back) = (
        tab.detached_banner_shows(),
        tab.back_to().map(str::to_owned),
    );
    git_ok(&dir, &["switch", "-q", "feature/ünï"]);
    git_ok(&dir, &["switch", "-q", "--detach", "HEAD"]);
    let again = opened(&dir, &ctx);
    let back_again = again.back_to().map(str::to_owned);
    git_ok(&dir, &["switch", "-q", "main"]);
    let attached = opened(&dir, &ctx);
    let _ = std::fs::remove_dir_all(&dir);
    assert!(shows);
    assert_eq!(back.as_deref(), Some("main"));
    assert_eq!(back_again.as_deref(), Some("feature/ünï"));
    assert!(!attached.detached_banner_shows());
    assert!(crate::i18n::text(banner::SAYS).contains("belong to no branch"));
}

/// A bisect detaches HEAD and has a banner of its own: this one stays away meanwhile.
#[test]
fn no_banner_during_a_bisect() {
    let dir = detached_repo("bisect");
    git_ok(&dir, &["switch", "-q", "main"]);
    for n in ["three", "four", "five"] {
        git_ok(&dir, &["commit", "-q", "--allow-empty", "-m", n]);
    }
    git_ok(&dir, &["bisect", "start", "main", "main~4"]);
    let ctx = egui::Context::default();
    let tab = opened(&dir, &ctx);
    let (detached, shows) = (tab.is_detached(), tab.detached_banner_shows());
    let _ = git::run(Some(&dir), &["bisect", "reset"]);
    let _ = std::fs::remove_dir_all(&dir);
    assert!(detached);
    assert!(!shows);
}

/// Nothing committed on the detached HEAD: checking out goes straight through.
#[test]
fn leaving_without_commits_checks_out_at_once() {
    let dir = detached_repo("nothing");
    let ctx = egui::Context::default();
    let mut tab = opened(&dir, &ctx);
    tab.apply(Action::Checkout("main".to_owned()), &ctx);
    let back = wait(&mut tab, &ctx, |t| {
        t.busy_op.is_none() && !t.detached.checking
    });
    let head = symbolic_head(&dir);
    let _ = std::fs::remove_dir_all(&dir);
    assert!(back);
    assert!(tab.dialog.is_none());
    assert_eq!(head, "refs/heads/main");
}

/// Commits made on the detached HEAD: checking out asks first, listing them; keeping them puts
/// them on the new branch, then checks out what was asked for.
#[test]
fn commits_on_a_detached_head_are_kept_on_a_branch_before_checking_out() {
    let dir = detached_repo("keep");
    git_ok(
        &dir,
        &["commit", "-q", "--allow-empty", "-m", "lost work ünï ✓"],
    );
    let lost = git::head(&dir);
    let ctx = egui::Context::default();
    let mut tab = opened(&dir, &ctx);
    tab.apply(Action::Checkout("main".to_owned()), &ctx);
    let asked = wait(&mut tab, &ctx, |t| {
        matches!(t.dialog, Some(Dialog::KeepDetached(_)))
    });
    let (listed, head) = match &tab.dialog {
        Some(Dialog::KeepDetached(d)) => (d.commits.clone(), d.head.clone()),
        _ => (Vec::new(), String::new()),
    };
    let still = symbolic_head(&dir);
    tab.dialog = None;
    tab.apply(
        Action::Detached(DetachedAct::Keep {
            name: "saved/ünï".to_owned(),
            head,
            leave: Leave::Checkout("main".to_owned()),
        }),
        &ctx,
    );
    let done = wait(&mut tab, &ctx, |t| {
        t.busy_op.is_none() && t.queued.is_empty() && symbolic_head(&dir) == "refs/heads/main"
    });
    let kept = git::resolve(&dir, "refs/heads/saved/ünï");
    let _ = std::fs::remove_dir_all(&dir);
    assert!(asked, "no dialog: {:?}", tab.status);
    assert_eq!(listed, [(lost.clone(), "lost work ünï ✓".to_owned())]);
    assert_eq!(still, "", "it checked out before asking");
    assert!(done, "{:?}", tab.status);
    assert_eq!(kept, Ok(lost));
}

/// Leaving them checks out as asked: the commits are on no branch after.
#[test]
fn commits_left_behind_stay_on_no_branch() {
    let dir = detached_repo("leave");
    git_ok(&dir, &["commit", "-q", "--allow-empty", "-m", "throwaway"]);
    let lost = git::head(&dir);
    let ctx = egui::Context::default();
    let mut tab = opened(&dir, &ctx);
    tab.apply(Action::Checkout("feature/ünï".to_owned()), &ctx);
    let asked = wait(&mut tab, &ctx, |t| t.dialog.is_some());
    tab.dialog = None;
    let leave = Leave::Checkout("feature/ünï".to_owned());
    tab.apply(Action::Detached(DetachedAct::Leave(leave)), &ctx);
    let done = wait(&mut tab, &ctx, |t| t.busy_op.is_none());
    let head = symbolic_head(&dir);
    let on_a_branch = git::run(Some(&dir), &["branch", "--contains", &lost]);
    let _ = std::fs::remove_dir_all(&dir);
    assert!(asked && done);
    assert_eq!(head, "refs/heads/feature/ünï");
    assert_eq!(on_a_branch.map(|b| b.trim().to_owned()), Ok(String::new()));
}

/// Checking out a commit made after them (detached again) leaves nothing behind; a commit
/// already on a branch or a tag is never listed.
#[test]
fn only_commits_on_nothing_else_are_left_behind() -> Result<(), String> {
    let dir = detached_repo("left");
    git_ok(&dir, &["commit", "-q", "--allow-empty", "-m", "one"]);
    let one = git::head(&dir);
    git_ok(&dir, &["commit", "-q", "--allow-empty", "-m", "two"]);
    let two = git::head(&dir);
    let both = git::left_behind(&dir, "main")?;
    git_ok(&dir, &["switch", "-q", "--detach", &one]);
    let ahead = git::left_behind(&dir, &two)?;
    git_ok(&dir, &["tag", "kept", &one]);
    let tagged = git::left_behind(&dir, "main")?;
    let _ = std::fs::remove_dir_all(&dir);
    let listed: Vec<&str> = both.iter().map(|(h, _)| h.as_str()).collect();
    assert_eq!(listed, [two.as_str(), one.as_str()]);
    assert!(ahead.is_empty());
    assert!(tagged.is_empty());
    Ok(())
}

/// Something started while the commits were being listed (a commit from the box): the answer
/// may be out of date, so nothing is checked out and nothing asked; it says why.
#[test]
fn a_listing_that_answers_while_busy_does_nothing() {
    let dir = detached_repo("busy");
    git_ok(&dir, &["commit", "-q", "--allow-empty", "-m", "loose"]);
    let ctx = egui::Context::default();
    let mut tab = opened(&dir, &ctx);
    tab.apply(Action::Checkout("main".to_owned()), &ctx);
    tab.busy_op = Some("Commit".to_owned());
    let answered = wait(&mut tab, &ctx, |t| !t.detached.checking);
    let status = tab.status.clone().map(|(s, _)| s).unwrap_or_default();
    let head = symbolic_head(&dir);
    let _ = std::fs::remove_dir_all(&dir);
    assert!(answered);
    assert!(tab.dialog.is_none());
    assert_eq!(head, "");
    assert!(status.contains("Commit started meanwhile"), "{status}");
}
