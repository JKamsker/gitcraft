use super::*;

#[test]
fn a_pull_request_that_cannot_open_leaves_the_builds_shown() {
    use crate::repo::{Action, RepoTab, rebase::RebaseEditor};
    let dir = repo("pr-refused");
    let ctx = egui::Context::default();
    let mut tab = RepoTab::new_deferred(dir.clone(), &ctx);
    // No forge remote: the builds view opens without asking anything.
    tab.apply(
        Action::OpenCi {
            sha: "a".repeat(40),
            run: None,
        },
        &ctx,
    );
    let shown = tab.ci_view.is_some();
    tab.rebase = Some(RebaseEditor::new("x".into(), "x".into(), Vec::new()));
    tab.apply(Action::OpenPr(7), &ctx);
    let _ = std::fs::remove_dir_all(&dir);
    assert!(shown);
    assert!(tab.pr_view.is_none());
    assert!(tab.ci_view.is_some());
}

#[cfg(unix)]
#[test]
fn an_editor_command_with_arguments_is_run_with_the_path_as_one_argument() {
    use crate::repo::apply::open_in_editor;
    let dir = std::env::temp_dir().join(format!("gitcraft-editor-args-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::create_dir_all(&dir);
    let file = dir.join("a b $(x).txt");
    // `cp /dev/null <path>`: the editor's arguments, then the path, which makes the file.
    let ran = open_in_editor("cp /dev/null", &file);
    let mut made = false;
    for _ in 0..100 {
        made = file.exists();
        if made {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    let _ = std::fs::remove_dir_all(&dir);
    assert!(ran.is_ok(), "{ran:?}");
    assert!(made);
}

/// Waits for the operation the tab runs to end, and takes in how it ended.
#[test]
fn alias_authentication_text_cannot_authorize_retry_of_remaining_steps() -> Result<(), String> {
    use crate::repo::{Action, Refresh, RepoTab};
    let dir = repo("sign-in-retry");
    let ctx = egui::Context::default();
    let mut tab = RepoTab::new_deferred(dir.clone(), &ctx);
    // A non-transport alias imitates an authentication failure.
    let push = ops(&[&[
        "-c",
        "alias.gitcraft-test-auth=!printf \"Authentication failed for 'https://fixture.invalid/team/repo'\\n\" >&2; exit 1",
        "gitcraft-test-auth",
    ]]);
    let steps: Vec<Vec<String>> = ops(&[&["tag", "v1"]])
        .into_iter()
        .chain(push.clone())
        .collect();
    tab.apply(
        Action::Ops {
            label: "Create tag v1".to_owned(),
            ops: steps,
            refresh: Refresh::All,
        },
        &ctx,
    );
    finish(&mut tab, &ctx);
    let asked = matches!(tab.dialog, Some(crate::repo::Dialog::Credentials { .. }));
    let kept = tab.last_op.as_ref().map(|op| op.ops.clone());
    let creds =
        git::Credentials::for_url("https://fixture.invalid/team/repo", "u".into(), "p".into())?;
    tab.sign_in(creds, &ctx);
    finish(&mut tab, &ctx);
    let status = tab
        .status
        .as_deref()
        .cloned()
        .map(|(s, _)| s)
        .unwrap_or_default();
    let _ = std::fs::remove_dir_all(&dir);
    assert!(!asked);
    assert_eq!(kept, Some(push));
    assert!(tab.credentials.is_none());
    assert!(tab.busy_op.is_none() && !tab.https_checking);
    assert!(
        status.contains("destination or account context changed"),
        "{status}"
    );
    Ok(())
}

/// Aborting a merge or rebase throws away the conflicts resolved so far, with no undo: it asks
/// first.
#[test]
fn aborting_asks_first() {
    use crate::repo::{Action, Dialog, abort_action};
    let asks = match abort_action(git::InProgress::Merge) {
        Action::Dialog(Dialog::Confirm { ops: steps, .. }) => Some(steps),
        _ => None,
    };
    assert_eq!(asks, Some(ops(&[&["merge", "--abort"]])));
}

/// An undo whose alias prints authentication text remains undoable, but that
/// text cannot authorize a credential-bearing retry or move the timeline.
#[test]
fn unverified_alias_authentication_keeps_undo_without_authorizing_retry() -> Result<(), String> {
    use crate::repo::timeline::{Entry, Origin, RefState};
    use crate::repo::{RepoTab, UndoEntry};
    let dir = repo("sign-in-undo");
    let ctx = egui::Context::default();
    let mut tab = RepoTab::new(dir.clone(), &ctx);
    // The timeline is read (there is none yet) before undo goes ahead.
    for _ in 0..2000 {
        tab.poll(&ctx);
        if tab.timeline.loaded {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    // An undo git answers like one that needs signing in.
    let entry = UndoEntry {
        id: 0,
        label: "Push".to_owned(),
        undo: ops(&[&[
            "-c",
            "alias.gitcraft-test-auth=!printf \"Authentication failed for 'https://fixture.invalid/team/repo'\\n\" >&2; exit 1",
            "gitcraft-test-auth",
        ]]),
        redo: Vec::new(),
        heads: None,
        agent: None,
    };
    let id = tab.timeline.push(Entry {
        id: 0,
        parent: None,
        time: 0,
        label: "Push".to_owned(),
        origin: Origin::Gitcraft,
        before: RefState::default(),
        after: RefState::default(),
        files: Vec::new(),
        files_total: 0,
        undo: Some(entry),
        redo_child: None,
        receipt: None,
    });
    tab.undo(&ctx);
    let running = tab.busy_op.is_some();
    finish(&mut tab, &ctx);
    let asked = matches!(tab.dialog, Some(crate::repo::Dialog::Credentials { .. }));
    // Cancelled.
    tab.dialog = None;
    let after_cancel = tab.timeline.current;
    let creds =
        git::Credentials::for_url("https://fixture.invalid/team/repo", "u".into(), "p".into())?;
    tab.sign_in(creds, &ctx);
    let retrying = tab.busy_op.is_some();
    finish(&mut tab, &ctx);
    let after_retry = tab.timeline.current;
    let entries = tab.timeline.entries.len();
    let _ = std::fs::remove_dir_all(&dir);
    assert!(running);
    assert!(!asked);
    assert_eq!(after_cancel, Some(id));
    assert!(!retrying);
    assert!(tab.credentials.is_none());
    // No authorized retry: still there, still only once.
    assert_eq!(after_retry, Some(id));
    assert_eq!(entries, 1);
    Ok(())
}

/// Work that panics (a bug in a dependency) still ends: the tab isn't left busy for good.
#[test]
fn an_operation_that_panics_leaves_the_tab_idle() {
    use crate::repo::messages::spawn_or;
    use crate::repo::{Msg, Refresh, RepoTab, spawn};
    let dir = repo("job-panics");
    let ctx = egui::Context::default();
    let mut tab = RepoTab::new_deferred(dir.clone(), &ctx);
    tab.busy_op = Some("Push".to_owned());
    let failed = |e| Msg::OpDone {
        label: "Push".to_owned(),
        result: Err(e),
        refresh: Refresh::Status,
        kind: OpKind::plain(),
        quiet: false,
        stopped: 0,
    };
    spawn_or(&tab.tx, &ctx, failed, || {
        std::panic::resume_unwind(Box::new("boom"))
    });
    finish(&mut tab, &ctx);
    let op_status = tab.status.as_deref().cloned().map(|(s, _)| s);
    // Work nothing waits for says so.
    tab.status = None;
    spawn(&tab.tx, &ctx, || {
        std::panic::resume_unwind(Box::new("bang".to_owned()))
    });
    for _ in 0..400 {
        tab.poll(&ctx);
        if tab.status.is_some() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    let job_status = tab.status.as_deref().cloned().map(|(s, _)| s);
    let _ = std::fs::remove_dir_all(&dir);
    assert!(tab.busy_op.is_none());
    assert_eq!(
        op_status.as_deref(),
        Some("Push failed: Internal error: boom")
    );
    assert_eq!(job_status.as_deref(), Some("Internal error: bang"));
}

/// A drop confirmed while another operation started meanwhile (the background fetch) says so,
/// rather than closing the dialog with nothing done and nothing said.
#[test]
fn a_drop_confirmed_while_busy_says_so() {
    use crate::repo::{Action, RepoTab, quick_edit::Edit};
    let dir = repo("drop-busy");
    let ctx = egui::Context::default();
    let mut tab = RepoTab::new_deferred(dir.clone(), &ctx);
    tab.busy_op = Some("Fetch".to_owned());
    tab.apply(
        Action::QuickEdit {
            edit: Edit::Drop,
            hash: "0123456789abcdef".to_owned(),
            branch_guard: crate::repo::operations::expect_symref("HEAD", "refs/heads/main"),
        },
        &ctx,
    );
    let status = tab.status.as_deref().cloned();
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(
        status,
        Some((
            "Drop commit 01234567: Fetch is still running, try again when it's done".to_owned(),
            true
        ))
    );
}
