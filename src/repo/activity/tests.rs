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
    log.reveal((MAX_ENTRIES + 24) as u64);
    assert!(log.open);
    assert_eq!(log.reveal, Some((MAX_ENTRIES + 24) as u64));
    // One dropped since is not revealed.
    log.reveal(3);
    assert_eq!(log.reveal, None);
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
fn commands_read_as_typed_in_a_shell() {
    let args = |a: &[&str]| a.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>();
    assert_eq!(
        command_line(&args(&["commit", "-m", "it's a fix", "--", "a b.txt"])),
        r"git commit -m 'it'\''s a fix' -- 'a b.txt'"
    );
    let many: Vec<String> = (0..1000).map(|i| format!("f{i}")).collect();
    let short = short_command_line(&[vec!["add".to_owned(), "--".to_owned()], many].concat());
    assert!(short.ends_with("f21 … 978 more"), "{short}");
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

/// What is recorded of a call has its secrets hidden: a token URL in its arguments and output,
/// and the password typed into the tab.
#[test]
fn a_recorded_call_hides_its_secrets() -> Result<(), String> {
    let dir = repo("secrets");
    let creds = git::Credentials {
        scope: Some(git::CredentialScope::for_url(
            "https://example.invalid/o/r.git",
            "ana",
        )?),
        username: "ana".to_owned(),
        password: "s3cr3t-pass".to_owned(),
    };
    let url = "https://ghp_TOKEN123@example.invalid/o/r.git";
    let (_, added) = git::run_recorded(
        Some(&dir),
        &["remote", "add", "o", url],
        Some(&creds),
        false,
    );
    let (_, listed) = git::run_recorded(Some(&dir), &["remote", "-v"], None, false);
    let (_, echoed) = git::run_recorded(
        Some(&dir),
        &["config", "x.y", "s3cr3t-pass"],
        Some(&creds),
        false,
    );
    let _ = std::fs::remove_dir_all(&dir);
    for rec in [&added, &listed, &echoed] {
        let text = format!("{:?} {}", rec.args, rec.output);
        assert!(!text.contains("ghp_TOKEN123"), "{text}");
        assert!(!text.contains("s3cr3t-pass"), "{text}");
    }
    assert!(
        listed
            .output
            .contains("https://***@example.invalid/o/r.git"),
        "{}",
        listed.output
    );
    Ok(())
}

/// A background fetch failing the same way again and again is one entry, counted.
#[test]
fn the_same_background_failure_is_counted_not_repeated() {
    let mut log = ActivityLog::default();
    let failed = || Outcome::Failed("could not resolve host".to_owned());
    for _ in 0..5 {
        log.push(
            "Background fetch",
            Duration::ZERO,
            Vec::new(),
            failed(),
            true,
        );
    }
    log.push("Fetch", Duration::ZERO, Vec::new(), Outcome::Done, false);
    log.push(
        "Background fetch",
        Duration::ZERO,
        Vec::new(),
        failed(),
        true,
    );
    let counts: Vec<(String, u32)> = log
        .entries()
        .map(|e| (e.label.clone(), e.repeats))
        .collect();
    assert_eq!(
        counts,
        [
            ("Background fetch".to_owned(), 5),
            ("Fetch".to_owned(), 1),
            ("Background fetch".to_owned(), 1)
        ]
    );
}

/// The toolbar's message opens the entry of the operation that left it, not the newest one.
#[test]
fn the_toolbar_message_opens_its_own_operation() {
    let dir = repo("link");
    let ctx = egui::Context::default();
    let mut tab = RepoTab::new_deferred(dir.clone(), &ctx);
    tab.run_ops(
        "Check out",
        ops(&[&["checkout", "nowhere"]]),
        Refresh::All,
        OpKind::plain(),
        &ctx,
    );
    finish(&mut tab, &ctx);
    let failed = tab.status_entry();
    // Another operation after it: its message (none, for staging) is never tied to the
    // failed one's entry, nor the failed one's message to the newer entry.
    let old_message = tab.status.clone();
    tab.run_ops(
        "Stage",
        ops(&[&["status"]]),
        Refresh::Status,
        OpKind::plain(),
        &ctx,
    );
    finish(&mut tab, &ctx);
    let after = tab.status_entry();
    tab.status = old_message;
    let restored = tab.status_entry();
    let newest = tab.activity.entries().last().map(|e| e.id);
    let _ = std::fs::remove_dir_all(&dir);
    assert!(failed.is_some());
    assert_ne!(after, newest);
    assert_ne!(restored, newest);
    // A message no operation left opens nothing.
    tab.status = Some(("Something else".to_owned(), true));
    assert_eq!(tab.status_entry(), None);
}

#[test]
fn fluent_partition_actual_activity_adapter_formats_numbers_and_preserves_commands() {
    let args = [
        "push",
        "--force-with-lease=refs/heads/topic:abc",
        "origin",
        "refs/heads/topic:refs/heads/topic",
    ]
    .map(str::to_owned);
    for (language, done, duration) in [("en-US", "done", "1.2 s"), ("de", "abgeschlossen", "1,2 s")]
    {
        crate::i18n::set_language(language);
        assert_eq!(outcome_text(&Outcome::Done), done);
        assert_eq!(seconds(Duration::from_millis(1200)), duration);
        assert_eq!(
            command_line(&args),
            "git push --force-with-lease=refs/heads/topic:abc origin refs/heads/topic:refs/heads/topic"
        );
        assert_eq!(args[2], "origin");
    }
    crate::i18n::set_language("en-US");
}

#[test]
fn fluent_partition_control_error_paint_preserves_raw_cancellation_and_opaque_diagnostics() {
    let raw = "Moved-code comparison cancelled";
    for (language, expected) in [
        ("en-US", raw),
        ("de", "Vergleich verschobenen Codes abgebrochen"),
    ] {
        crate::i18n::set_language(language);
        assert_eq!(display_workflow_error(raw), expected);
        assert_eq!(raw, "Moved-code comparison cancelled");
        let external = "fatal: opaque external diagnostic refs/heads/topic --force";
        assert_eq!(display_workflow_error(external), external);
    }
    crate::i18n::set_language("en-US");
}
