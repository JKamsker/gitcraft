//! The activity log: its bounds, the text it copies, and what an operation leaves in it when
//! run as the tab runs it.

use std::time::{Duration, Instant};

use super::*;
use crate::git;
use crate::repo::{OpKind, Refresh, RepoTab, ops};

fn step(args: &[&str], code: Option<i32>, output: &str) -> Recorded {
    Recorded {
        args: args.iter().map(|a| (*a).to_owned()).collect(),
        code,
        output: output.to_owned(),
        took: Duration::from_millis(1200),
    }
}

#[test]
fn the_log_keeps_the_newest_operations() {
    let mut log = ActivityLog::default();
    for i in 0..MAX_ENTRIES + 25 {
        log.push(
            &format!("Op {i}"),
            Duration::ZERO,
            Vec::new(),
            Outcome::Done,
            false,
        );
    }
    assert_eq!(log.len(), MAX_ENTRIES);
    assert_eq!(
        log.entries().next().map(|e| e.label.clone()),
        Some("Op 25".to_owned())
    );
    assert_eq!(
        log.last().map(|e| (e.label.clone(), e.id)),
        Some((
            format!("Op {}", MAX_ENTRIES + 24),
            (MAX_ENTRIES + 24) as u64
        ))
    );
    log.reveal_last();
    assert!(log.open);
    assert_eq!(log.reveal, Some((MAX_ENTRIES + 24) as u64));
    log.clear();
    assert_eq!(log.len(), 0);
}

#[test]
fn a_huge_output_keeps_its_start_and_end() {
    // Multi-byte characters across the cut points.
    let huge = format!("start\n{}\nfatal: the end", "é".repeat(MAX_OUTPUT));
    let kept = bounded(&huge);
    assert!(kept.len() <= MAX_OUTPUT + 64, "{}", kept.len());
    assert!(kept.starts_with("start\n"));
    assert!(kept.ends_with("fatal: the end"));
    assert!(kept.contains("bytes left out"));
    assert_eq!(bounded(""), "");
    assert_eq!(bounded("small"), "small");
    let mut log = ActivityLog::default();
    log.push(
        "Fetch",
        Duration::ZERO,
        vec![step(&["fetch"], Some(1), &huge)],
        Outcome::Done,
        false,
    );
    assert!(
        log.last()
            .is_some_and(|e| e.steps[0].output.len() <= MAX_OUTPUT + 64)
    );
}

#[test]
fn commands_read_as_typed_in_a_shell_and_hide_passwords() {
    let args = |a: &[&str]| a.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>();
    assert_eq!(
        command_line(&args(&["commit", "-m", "it's a fix", "--", "a b.txt"])),
        r"git commit -m 'it'\''s a fix' -- 'a b.txt'"
    );
    assert_eq!(
        command_line(&args(&["clone", "https://ana:s3cret@host/r.git", "dépôt"])),
        "git clone 'https://ana:***@host/r.git' dépôt"
    );
    assert_eq!(
        command_line(&args(&[
            "fetch",
            "https://host/a@b",
            "",
            "ssh://git@host/r"
        ])),
        "git fetch https://host/a@b '' ssh://git@host/r"
    );
}

#[test]
fn an_entry_copies_as_text() {
    let mut log = ActivityLog::default();
    log.push(
        "Push",
        Duration::from_secs(75),
        vec![
            step(&["push", "--progress"], Some(1), "To x\n ! [rejected]"),
            step(&["status"], None, ""),
        ],
        Outcome::Failed("rejected".to_owned()),
        false,
    );
    let text = log.last().map(Entry::as_text).unwrap_or_default();
    assert_eq!(
        text,
        "Push (1 min 15 s): failed\n\n$ git push --progress\n[exit 1 · 1.2 s]\nTo x\n ! [rejected]\n\n$ git status\n[did not finish · 1.2 s]\n"
    );
}

fn repo(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("gitcraft-activity {name} {}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::create_dir_all(&dir);
    for args in [
        &["init", "-q"][..],
        &["config", "user.name", "t"],
        &["config", "user.email", "t@t"],
        &["commit", "-q", "--allow-empty", "-m", "base"],
    ] {
        assert!(git::run(Some(&dir), args).is_ok());
    }
    dir
}

fn finish(tab: &mut RepoTab, ctx: &egui::Context) {
    let start = Instant::now();
    while tab.busy_op.is_some() && start.elapsed() < Duration::from_secs(20) {
        tab.poll(ctx);
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// An operation run as the toolbar runs it leaves each command, its exit code and all it
/// printed in the log: the steps that ran, up to the one that failed.
#[test]
fn an_operation_leaves_its_commands_and_output() {
    let dir = repo("ops");
    let ctx = egui::Context::default();
    let mut tab = RepoTab::new_deferred(dir.clone(), &ctx);
    let steps = ops(&[
        &["tag", "v1-ünï"],
        &["checkout", "no-such-branch"],
        &["tag", "never"],
    ]);
    tab.run_ops(
        "Tag and check out",
        steps,
        Refresh::Status,
        OpKind::plain(),
        &ctx,
    );
    finish(&mut tab, &ctx);
    let ok_steps = ops(&[&["tag", "--list"]]);
    tab.run_ops("List", ok_steps, Refresh::Status, OpKind::plain(), &ctx);
    finish(&mut tab, &ctx);
    let _ = std::fs::remove_dir_all(&dir);
    let entries: Vec<&Entry> = tab.activity.entries().collect();
    assert_eq!(entries.len(), 2);
    let failed = entries[0];
    assert!(matches!(&failed.outcome, Outcome::Failed(e) if e.contains("no-such-branch")));
    assert_eq!(failed.steps.len(), 2, "{:?}", failed.steps);
    assert_eq!(failed.steps[0].code, Some(0));
    assert_eq!(failed.steps[1].code, Some(1));
    assert!(failed.steps[1].output.contains("no-such-branch"));
    let listed = entries[1];
    assert_eq!(listed.outcome, Outcome::Done);
    assert_eq!(listed.steps[0].output, "v1-ünï");
    assert!(listed.as_text().contains("$ git tag --list\n[exit 0"));
}

/// The background fetch is kept only when it failed.
#[test]
fn a_background_fetch_is_kept_only_when_it_fails() {
    let dir = repo("background");
    let ctx = egui::Context::default();
    let mut tab = RepoTab::new_deferred(dir.clone(), &ctx);
    tab.run_ops_ex(
        "Background fetch",
        ops(&[&["status"]]),
        Refresh::Status,
        OpKind::plain(),
        true,
        &ctx,
    );
    finish(&mut tab, &ctx);
    let after_ok = tab.activity.len();
    tab.run_ops_ex(
        "Background fetch",
        ops(&[&["fetch", "no-such-remote"]]),
        Refresh::Status,
        OpKind::plain(),
        true,
        &ctx,
    );
    finish(&mut tab, &ctx);
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(after_ok, 0);
    assert_eq!(tab.activity.len(), 1);
    assert!(tab.activity.last().is_some_and(|e| e.background
        && matches!(e.outcome, Outcome::Failed(_))
        && e.steps[0].args == ["fetch", "no-such-remote"]));
}
