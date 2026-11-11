use crate::repo::terminals::TerminalAct;
use std::path::{Path, PathBuf};
use std::time::Duration;

use egui::{Event, Key, Modifiers, RawInput};

use super::super::{Action, RepoTab, WIP_HASH};
use crate::git;
use crate::keymap::Command;

/// A repository with one commit in a fresh folder with spaces and accents in its name.
fn repo(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("gitcraft keys ü {name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::create_dir_all(&dir);
    for args in [
        &["init", "-q"][..],
        &["config", "user.name", "t"],
        &["config", "user.email", "t@t"],
        &["config", "commit.gpgsign", "false"],
        &["commit", "-q", "--allow-empty", "-m", "base"],
    ] {
        assert!(git::run(Some(&dir), args).is_ok(), "{args:?}");
    }
    dir
}

/// Polls `tab` until `done`, or gives up after ten seconds.
fn wait(tab: &mut RepoTab, ctx: &egui::Context, done: impl Fn(&RepoTab) -> bool) {
    for _ in 0..400 {
        tab.poll(ctx);
        if done(tab) {
            return;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
}

/// The tab on `dir`, read through.
fn loaded(dir: &Path, ctx: &egui::Context) -> RepoTab {
    let mut tab = RepoTab::new_deferred(dir.to_owned(), ctx);
    tab.start_deferred(ctx);
    wait(&mut tab, ctx, |t| !t.opening() && !t.loading);
    tab
}

/// Runs the key of `command` and waits for what it started, and the read after it.
fn press(tab: &mut RepoTab, ctx: &egui::Context, command: Command) {
    tab.run_key(command, ctx);
    wait(tab, ctx, |t| t.busy_op.is_none() && !t.loading);
}

fn status(dir: &Path) -> String {
    git::run(Some(dir), &["status", "--porcelain"]).unwrap_or_default()
}

/// What can't be done now says why, as the toolbar greys it out; the window's keys aren't
/// the tab's.
#[test]
fn keys_that_cannot_be_used_now_say_why() {
    let dir = repo("refused");
    let ctx = egui::Context::default();
    let tab = loaded(&dir, &ctx);
    let why = |c| tab.key_action(c).err().unwrap_or_default();
    let fetch = why(Command::Fetch);
    let push = why(Command::Push);
    let stash = why(Command::Stash);
    let stage = why(Command::StageAll);
    let commit = why(Command::Commit);
    let branch = matches!(
        tab.key_action(Command::NewBranch),
        Ok(Some(Action::Dialog(_)))
    );
    let terminal = matches!(
        tab.key_action(Command::Terminal),
        Ok(Some(Action::Terminal(TerminalAct::Toggle)))
    );
    let window = tab.key_action(Command::NextTab);
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(fetch, "This repository has no remote");
    assert_eq!(push, "This repository has no remote");
    assert_eq!(stash, "No changes to stash");
    assert_eq!(stage, "Nothing to stage");
    assert_eq!(commit, "Nothing to commit");
    assert!(branch);
    assert!(terminal);
    assert!(matches!(window, Ok(None)));
}

/// Every key of the tab is turned into an action or a reason, and, while an operation
/// runs, only the terminal's key still does something.
#[test]
fn every_key_resolves_and_waits_for_a_running_operation() {
    let dir = repo("busy");
    let ctx = egui::Context::default();
    let mut tab = loaded(&dir, &ctx);
    for b in crate::keymap::BINDINGS {
        let resolved = tab.key_action(b.command);
        // Graph/rebase/search/selection/settings widgets own their registry commands.
        // Reload also has a direct repository action behind its window shortcut.
        let tabs = matches!(
            b.command.scope(),
            crate::keymap::Scope::Repository
                | crate::keymap::Scope::Changes
                | crate::keymap::Scope::Diff
        ) || matches!(
            b.command,
            Command::Reload
                | Command::GraphCopyHash
                | Command::GraphCopyMessage
                | Command::GraphCopyBranch
                | Command::FileNext
                | Command::FilePrevious
                | Command::FileToggleStage
                | Command::FileOpen
        );
        match resolved {
            Ok(Some(_)) => assert!(tabs, "{:?}", b.command),
            Ok(None) => assert!(!tabs, "{:?}", b.command),
            Err(why) => assert!(tabs && !why.is_empty(), "{:?}", b.command),
        }
    }
    tab.busy_op = Some("Push".to_owned());
    let fetch = tab.key_action(Command::Fetch).err();
    let commit = tab.key_action(Command::Commit).err();
    let terminal = tab.key_action(Command::Terminal);
    tab.run_key(Command::StageAll, &ctx);
    let shown = tab.status.as_deref().cloned();
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(fetch.as_deref(), Some("Push is still running"));
    assert_eq!(commit.as_deref(), Some("Push is still running"));
    assert!(matches!(
        terminal,
        Ok(Some(Action::Terminal(TerminalAct::Toggle)))
    ));
    assert_eq!(shown, Some(("Push is still running".to_owned(), true)));
}

/// Stage all, then commit: with no summary the key goes to the message; with one it
/// commits what is staged, names and messages in any script.
#[test]
fn stage_all_and_commit_from_the_keys() {
    let dir = repo("commit");
    let ctx = egui::Context::default();
    let name = "ünï cødé 文件 with spaces.txt";
    let _ = std::fs::write(dir.join(name), "x\n");
    let mut tab = loaded(&dir, &ctx);
    wait(&mut tab, &ctx, |t| !t.work.unstaged.is_empty());

    press(&mut tab, &ctx, Command::StageAll);
    let staged = status(&dir);

    tab.commit_summary = "   ".to_owned();
    press(&mut tab, &ctx, Command::Commit);
    let focused = tab.focus_commit_summary;
    let on_changes = tab.selected == tab.index.get(WIP_HASH).copied();

    let summary = format!("feat: ✓ ünïcode {}", "long ".repeat(2000));
    tab.commit_summary = summary.clone();
    press(&mut tab, &ctx, Command::Commit);
    let message = git::run(Some(&dir), &["log", "-1", "--format=%s"]).unwrap_or_default();
    let after = status(&dir);
    let _ = std::fs::remove_dir_all(&dir);
    assert!(staged.starts_with("A "), "{staged}");
    assert!(focused);
    assert!(on_changes);
    assert_eq!(message.trim(), summary.trim());
    assert!(after.is_empty(), "{after}");
}

/// Fetch, pull and push from the keys, against a remote in a folder with spaces: each
/// shows on the remote or in the repository, not just in a status line.
#[test]
fn fetch_pull_and_push_from_the_keys() -> Result<(), String> {
    let dir = repo("remote");
    let name = dir.file_name().unwrap_or_default().to_string_lossy();
    let bare = dir.with_file_name(format!("{name} remote.git"));
    let other = dir.with_file_name(format!("{name} other clone"));
    for d in [&bare, &other] {
        let _ = std::fs::remove_dir_all(d);
    }
    let s = |p: &Path| p.to_string_lossy().into_owned();
    let run = |dir: Option<&Path>, args: &[&str]| git::run(dir, args).unwrap_or_default();
    assert!(git::run(None, &["init", "-q", "--bare", &s(&bare)]).is_ok());
    run(Some(&dir), &["remote", "add", "origin", &s(&bare)]);
    let branch = run(Some(&dir), &["branch", "--show-current"])
        .trim()
        .to_owned();
    let tracking = format!("refs/remotes/origin/{branch}");
    let ctx = egui::Context::default();
    let mut tab = loaded(&dir, &ctx);

    press(&mut tab, &ctx, Command::Push);
    assert!(tab.dialog.is_none(), "keyboard Push starts directly");
    wait(&mut tab, &ctx, |t| t.busy_op.is_none() && !t.loading);
    let pushed = run(Some(&bare), &["log", "-1", "--format=%s", &branch]);

    // A commit made on the remote from elsewhere: fetch sees it, pull brings it in.
    run(None, &["clone", "-q", &s(&bare), &s(&other)]);
    for args in [
        &["config", "user.name", "o"][..],
        &["config", "user.email", "o@o"],
        &["commit", "-q", "--allow-empty", "-m", "from élsewhere ✓"],
        &["push", "-q", "origin", "HEAD"],
    ] {
        run(Some(&other), args);
    }
    press(&mut tab, &ctx, Command::Fetch);
    let fetched = run(Some(&dir), &["log", "-1", "--format=%s", &tracking]);
    press(&mut tab, &ctx, Command::Pull);
    let pulled = run(Some(&dir), &["log", "-1", "--format=%s"]);
    let failed = tab.status.as_deref().cloned().filter(|(_, err)| *err);
    for d in [&dir, &bare, &other] {
        let _ = std::fs::remove_dir_all(d);
    }
    assert_eq!(pushed.trim(), "base");
    assert_eq!(fetched.trim(), "from élsewhere ✓");
    assert_eq!(pulled.trim(), "from élsewhere ✓");
    assert_eq!(failed, None);
    Ok(())
}

/// A merge whose conflicts were resolved back to HEAD's content has no changes, and the
/// key still concludes it; with no summary and no changes row, it says what is missing.
#[test]
fn the_commit_key_concludes_a_merge_and_never_points_at_a_missing_box() {
    let dir = repo("merge");
    let ctx = egui::Context::default();
    let mut tab = loaded(&dir, &ctx);
    tab.work.in_progress = Some(git::InProgress::Merge);
    tab.commit_summary = "Merge branch 'x'".to_owned();
    let concludes = matches!(tab.key_action(Command::Commit), Ok(Some(Action::Commit)));
    tab.work.in_progress = None;
    tab.amend = true;
    tab.commit_summary.clear();
    let nowhere = tab.key_action(Command::Commit).err();
    tab.focus_commit(&ctx);
    let stale = tab.focus_commit_summary;
    let _ = std::fs::remove_dir_all(&dir);
    assert!(concludes);
    assert_eq!(nowhere.as_deref(), Some("Write a commit summary first"));
    assert!(!stale);
}

/// Stage all would mark conflicted files resolved, markers and all.
#[test]
fn stage_all_waits_for_the_conflicts() {
    let dir = repo("conflicted");
    let ctx = egui::Context::default();
    let mut tab = loaded(&dir, &ctx);
    let file = |path: &str, area| git::StatusFile {
        path: path.to_owned(),
        old_path: None,
        code: 'U',
        area,
    };
    tab.work.conflicted = vec![file("a.txt", git::Area::Conflicted)];
    tab.work.unstaged = vec![file("b.txt", git::Area::Untracked)];
    let refused = tab.key_action(Command::StageAll).err();
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(
        refused.as_deref(),
        Some("Resolve the conflicted files first")
    );
}

/// One frame of `tab`'s keys with `events`, with a text field focused or not.
fn keys_in_frame(tab: &mut RepoTab, ctx: &egui::Context, events: Vec<Event>) -> Vec<Action> {
    let mut acts = Vec::new();
    let input = RawInput {
        events,
        ..Default::default()
    };
    ctx.run_ui(input, |_| tab.shortcuts(ctx, &mut acts))
        .textures_delta
        .clear();
    acts
}

/// Typing in a text field (the commit message, the branch filter) never runs a key's action.
#[test]
fn a_focused_text_field_keeps_the_keys() {
    let dir = repo("focus");
    let ctx = egui::Context::default();
    let mut tab = loaded(&dir, &ctx);
    let press = |key| Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: Modifiers::COMMAND,
    };
    let free = keys_in_frame(&mut tab, &ctx, vec![press(Key::J)]);

    // A text field takes the keyboard, then the key comes.
    let mut text = String::new();
    let mut field = |ctx: &egui::Context, events| {
        let input = RawInput {
            events,
            ..Default::default()
        };
        let mut acts = Vec::new();
        // As in the tab's layout: its keys first, then the views and their fields.
        ctx.run_ui(input, |ui| {
            tab.shortcuts(ctx, &mut acts);
            ui.text_edit_singleline(&mut text).request_focus();
        })
        .textures_delta
        .clear();
        acts
    };
    let _ = field(&ctx, Vec::new());
    let typing = field(&ctx, vec![press(Key::J), press(Key::Enter)]);
    // The palette or the manual is open over the tab.
    let ctx = egui::Context::default();
    tab.keys_blocked = true;
    let blocked = keys_in_frame(&mut tab, &ctx, vec![press(Key::J)]);
    let _ = std::fs::remove_dir_all(&dir);
    assert!(matches!(free.as_slice(), [Action::Key(Command::Terminal)]));
    assert!(typing.iter().all(|a| !matches!(a, Action::Key(_))));
    assert!(blocked.is_empty());
}

/// While a commit's message is edited, Ctrl+Enter saves it: it doesn't commit the changes
/// too, even when nothing has the focus. The other keys still work.
#[test]
fn saving_a_reworded_message_does_not_also_commit() {
    let dir = repo("reword");
    let ctx = egui::Context::default();
    let _ = std::fs::write(dir.join("dirty.txt"), "x\n");
    let mut tab = loaded(&dir, &ctx);
    tab.commit_summary = "not this".to_owned();
    tab.reword = Some(super::super::state::Reword {
        hash: tab.refs.head.clone(),
        subject: "new message".to_owned(),
        body: String::new(),
        focus: false,
        fill: false,
    });
    let press = |key| Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: Modifiers::COMMAND,
    };
    let acts = keys_in_frame(&mut tab, &ctx, vec![press(Key::Enter), press(Key::J)]);
    let _ = std::fs::remove_dir_all(&dir);
    assert!(matches!(acts.as_slice(), [Action::Key(Command::Terminal)]));
}
