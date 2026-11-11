use std::path::{Path, PathBuf};
use std::time::Duration;

use super::super::{Msg, RepoTab, rebase};
use crate::git;

#[test]
fn recording_interactive_approval_rejects_same_head_branch_and_todo_changes() -> Result<(), String>
{
    let ctx = egui::Context::default();
    let mut tab = RepoTab::new_deferred("/nonexistent/pure-rebase-approval".into(), &ctx);
    tab.refs.current = Some("approved-A".into());
    tab.refs.head = "a".repeat(40);
    let commit = git::RebaseCommit {
        hash: tab.refs.head.clone(),
        author: "Fixture".into(),
        email: "fixture@example.invalid".into(),
        summary: "One".into(),
        message: "One".into(),
        merge: false,
    };
    let mut editor = rebase::RebaseEditor::new("b".repeat(40), "base".into(), vec![commit]);
    editor.branch_guard = Some(tab.same_branch());
    editor.approve(&tab.same_branch())?;
    let id = editor.id;
    let approved = editor.clone();
    tab.rebase = Some(editor);
    tab.refs.current = Some("unexpected-B".into());
    tab.confirmed_rebase(id, &ctx);
    assert!(tab.busy_op.is_none() && tab.last_op.is_none());
    assert!(
        tab.status
            .as_deref()
            .is_some_and(|(s, _)| s.contains("changed"))
    );
    // The same branch change during asynchronous command preparation is also
    // rejected, rather than constructing a new guard for unexpected-B.
    tab.rebase_ready(
        id,
        "Interactive rebase",
        Ok((vec!["rebase".into()], tab.refs.head.clone(), vec![])),
        &ctx,
    );
    assert!(tab.busy_op.is_none() && tab.last_op.is_none());
    tab.refs.current = Some("approved-A".into());
    let mut changed = approved;
    changed.entries[0].message = "Unapproved replacement todo".into();
    tab.rebase = Some(changed);
    tab.confirmed_rebase(id, &ctx);
    assert!(tab.busy_op.is_none() && tab.last_op.is_none());
    assert!(
        !tab.rebase
            .as_ref()
            .ok_or("missing rebase editor")?
            .approved(&tab.same_branch(), &tab.refs.head)
    );
    Ok(())
}

fn repo(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("gitcraft-rewrite-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::create_dir_all(&dir);
    for args in [
        &["init", "-q", "-b", "main"][..],
        &["config", "user.name", "t"],
        &["config", "user.email", "t@t"],
        &["commit", "-q", "--allow-empty", "-m", "root"],
        &["commit", "-q", "--allow-empty", "-m", "base"],
        &["commit", "-q", "--allow-empty", "-m", "read"],
    ] {
        assert!(git::run(Some(&dir), args).is_ok());
    }
    dir
}

/// Waits for the operation the tab started, and says how it ended.
fn op_result(tab: &RepoTab) -> Option<Result<String, String>> {
    // Past the watcher, which comes on the same channel.
    loop {
        match tab.rx.recv_timeout(Duration::from_secs(30)) {
            Ok(Msg::OpDone { result, .. }) => return Some(result),
            Ok(_) => {}
            Err(_) => return None,
        }
    }
}

#[test]
fn plain_rebase_preview_pins_target_before_a_later_branch_move() -> Result<(), String> {
    let dir = repo("pin-plain-preview");
    let head = git::resolve(&dir, "HEAD")?;
    let target = git::resolve(&dir, "HEAD^")?;
    git::run(Some(&dir), &["branch", "target", &target])?;
    let ctx = egui::Context::default();
    let mut tab = RepoTab::new_deferred(dir.clone(), &ctx);
    tab.refs.head = head.clone();
    tab.refs.current = Some("main".into());
    tab.ask_plain_rebase("target".into(), "Rebase onto target".into(), &ctx);
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    while tab.dialog.is_none() && std::time::Instant::now() < deadline {
        if let Ok(Msg::Then(next)) = tab.rx.recv_timeout(Duration::from_millis(100)) {
            next(&mut tab, &ctx);
        }
    }
    git::run(Some(&dir), &["update-ref", "refs/heads/target", &head])?;
    let Some(super::super::Dialog::Confirm { ops, .. }) = &tab.dialog else {
        return Err("Missing rebase confirmation".into());
    };
    assert_eq!(ops.last().and_then(|s| s.last()), Some(&target));
    assert!(ops.iter().any(|s| s.iter().any(|word| word == &head)));
    tab.dialog = None;
    drop(tab);
    let _ = std::fs::remove_dir_all(dir);
    Ok(())
}

fn subject(dir: &Path) -> String {
    git::head_message(dir)
        .map(|m| m.trim().to_owned())
        .unwrap_or_default()
}

/// A commit made in the terminal after the editor read the branch, and before a reload
/// told the tab: the rebase refuses rather than drop it.
#[test]
fn a_rebase_does_not_drop_a_commit_the_tab_has_not_seen_yet() {
    let dir = repo("rebase-unseen");
    let ctx = egui::Context::default();
    let mut tab = RepoTab::new_deferred(dir.clone(), &ctx);
    let read = git::head(&dir);
    tab.refs.head = read.clone();
    let commits = git::rebase_commits(&dir, "HEAD~1").unwrap_or_default();
    tab.rebase = Some(rebase::RebaseEditor::new(
        "HEAD~1".to_owned(),
        "HEAD~1".to_owned(),
        commits,
    ));
    let made = git::run(
        Some(&dir),
        &["commit", "-q", "--allow-empty", "-m", "unseen"],
    );
    let unseen = git::head(&dir);
    tab.run_rebase(&ctx);
    let result = op_result(&tab);
    let head = git::head(&dir);
    let _ = std::fs::remove_dir_all(&dir);
    assert!(made.is_ok());
    assert!(matches!(result, Some(Err(_))), "{result:?}");
    assert_eq!(head, unseen);
}

/// A rebase whose files were still being written when its editor was cancelled (or another
/// opened) doesn't start; nor can one being started be started a second time.
#[test]
fn a_rebase_prepared_for_an_editor_gone_since_does_not_run() {
    let dir = repo("rebase-cancelled");
    let ctx = egui::Context::default();
    let mut tab = RepoTab::new_deferred(dir.clone(), &ctx);
    tab.refs.head = git::head(&dir);
    let commits = git::rebase_commits(&dir, "HEAD~1").unwrap_or_default();
    let editor = rebase::RebaseEditor::new("HEAD~1".to_owned(), "HEAD~1".to_owned(), commits);
    let id = editor.id;
    let ready = || {
        Ok((
            vec!["rebase".to_owned(), "HEAD~1".to_owned()],
            String::new(),
            Vec::new(),
        ))
    };

    // Being started: a second Start does nothing.
    tab.rebase = Some(editor.clone());
    if let Some(e) = tab.rebase.as_mut() {
        e.starting = true;
    }
    tab.run_rebase(&ctx);
    assert!(tab.busy_op.is_none());

    // Cancelled while its files were written.
    tab.rebase = None;
    tab.rebase_ready(id, "Rebase", ready(), &ctx);
    assert!(tab.busy_op.is_none());

    // Another editor opened meanwhile: that one is left alone.
    let other = rebase::RebaseEditor::new("HEAD~1".to_owned(), "HEAD~1".to_owned(), vec![]);
    let other_id = other.id;
    tab.rebase = Some(other);
    tab.rebase_ready(id, "Rebase", ready(), &ctx);
    let _ = std::fs::remove_dir_all(&dir);
    assert!(tab.busy_op.is_none());
    assert_eq!(tab.rebase.as_ref().map(|e| e.id), Some(other_id));
    assert_ne!(id, other_id);
}

/// A row set to Edit (its E key) stops the rebase after that commit; what is staged then
/// goes into it when Commit continues, and the commits after it are replayed on top.
#[test]
fn an_edit_step_stops_there_and_commit_amends_it() {
    let dir = repo("rebase-edit");
    let ctx = egui::Context::default();
    let mut tab = RepoTab::new_deferred(dir.clone(), &ctx);
    tab.refs.head = git::head(&dir);
    tab.refs.current = Some("main".to_owned());
    let commits = git::rebase_commits(&dir, "HEAD~2").unwrap_or_default();
    let mut editor = rebase::RebaseEditor::new("HEAD~2".to_owned(), "HEAD~2".to_owned(), commits);
    editor.selected = Some(0);
    editor.press(egui::Key::E, false);
    tab.rebase = Some(editor);
    tab.run_rebase(&ctx);
    let stopped = op_result(&tab);
    let state = git::status(&dir).map(|w| w.in_progress);
    // Not staged: Continue stages it, as a commit would.
    let file = dir.join("añadido café.txt");
    let wrote = std::fs::write(&file, "edited\n");
    tab.work = git::status(&dir).unwrap_or_default();
    tab.busy_op = None;
    tab.commit(&ctx);
    let continued = op_result(&tab);
    let after = git::status(&dir).map(|w| w.in_progress);
    let base_files = git::run(
        Some(&dir),
        &[
            "-c",
            "core.quotePath=false",
            "show",
            "--name-only",
            "--format=",
            "HEAD~1",
        ],
    );
    let top = subject(&dir);
    let _ = std::fs::remove_dir_all(&dir);
    assert!(matches!(stopped, Some(Ok(_))), "{stopped:?}");
    assert_eq!(state, Ok(Some(git::InProgress::Rebase)));
    assert!(wrote.is_ok());
    assert!(matches!(continued, Some(Ok(_))), "{continued:?}");
    assert_eq!(after, Ok(None));
    // The change went into "base", and "read" is replayed after it.
    assert!(
        base_files.is_ok_and(|f| f.contains("añadido café.txt")),
        "the edited commit holds the staged file"
    );
    assert_eq!(top, "read");
}

/// Esc pressed in a reword message takes the keyboard out of it, and leaves the editor
/// open; pressed again, it closes the editor.
#[test]
fn esc_in_a_message_does_not_close_the_rebase_editor() {
    let dir = repo("rebase-esc");
    let ctx = egui::Context::default();
    let mut tab = RepoTab::new_deferred(dir.clone(), &ctx);
    let commits = git::rebase_commits(&dir, "HEAD~1").unwrap_or_default();
    tab.rebase = Some(rebase::RebaseEditor::new(
        "HEAD~1".to_owned(),
        "HEAD~1".to_owned(),
        commits,
    ));
    let esc = |tab: &mut RepoTab| {
        let input = egui::RawInput {
            events: vec![egui::Event::Key {
                key: egui::Key::Escape,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
            ..Default::default()
        };
        let mut acts = Vec::new();
        let mut out = ctx.run_ui(input, |ui| tab.shortcuts(ui.ctx(), &mut acts));
        out.textures_delta.clear();
        acts.iter()
            .any(|a| matches!(a, super::super::Action::CloseRebase))
    };
    if let Some(e) = tab.rebase.as_mut() {
        e.typing = true;
    }
    let first = esc(&mut tab);
    if let Some(e) = tab.rebase.as_mut() {
        e.typing = false;
    }
    let second = esc(&mut tab);
    let _ = std::fs::remove_dir_all(&dir);
    assert!(!first);
    assert!(second);
}

/// Editing the message of what the tab thinks is the latest commit, after another was made
/// on top: that one keeps its own message.
#[test]
fn rewording_the_last_commit_seen_leaves_a_newer_one_alone() {
    let dir = repo("reword-unseen");
    let ctx = egui::Context::default();
    let mut tab = RepoTab::new_deferred(dir.clone(), &ctx);
    let read = git::head(&dir);
    tab.refs.head = read.clone();
    let made = git::run(
        Some(&dir),
        &["commit", "-q", "--allow-empty", "-m", "unseen"],
    );
    tab.reword_commit(read, "reworded".to_owned(), &ctx);
    let result = op_result(&tab);
    let now = subject(&dir);
    let _ = std::fs::remove_dir_all(&dir);
    assert!(made.is_ok());
    assert!(matches!(result, Some(Err(_))), "{result:?}");
    assert_eq!(now, "unseen");
}

/// An older commit's message: the commits are read right before, and the rebase replays
/// them only if HEAD is still where that read saw it.
#[test]
fn rewording_an_older_commit_refuses_once_head_moved() {
    let dir = repo("reword-older");
    let ctx = egui::Context::default();
    let mut tab = RepoTab::new_deferred(dir.clone(), &ctx);
    let base = git::resolve(&dir, "HEAD~1").unwrap_or_default();
    let commits = git::rebase_commits(&dir, &format!("{base}^"));
    let made = git::run(
        Some(&dir),
        &["commit", "-q", "--allow-empty", "-m", "unseen"],
    );
    let unseen = git::head(&dir);
    tab.reword_older(base, "reworded".to_owned(), commits, &ctx);
    let result = op_result(&tab);
    let head = git::head(&dir);
    let _ = std::fs::remove_dir_all(&dir);
    assert!(made.is_ok());
    assert!(matches!(result, Some(Err(_))), "{result:?}");
    assert_eq!(head, unseen);
}
