//! Folding staged changes into older commits of real repositories: the history and trees it
//! leaves, the changes it keeps, conflicts, undo, signing and hooks.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use super::Plan;
use crate::git::{self, InProgress};
use crate::repo::{Action, Dialog, Msg, RepoTab};

/// A repository with a root commit and `n` more, commit `i` adding `f{i}.txt` with `i` in it.
fn repo(name: &str, n: usize) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("gitcraft-fold-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::create_dir_all(&dir);
    for args in [
        &["init", "-q", "-b", "main"][..],
        &["config", "user.name", "t"],
        &["config", "user.email", "t@t"],
        &["config", "commit.gpgsign", "false"],
        &["config", "core.autocrlf", "false"],
        &["config", "merge.conflictStyle", "merge"],
    ] {
        git_ok(&dir, args);
    }
    write(&dir, "root.txt", "root\n");
    git_ok(&dir, &["add", "-A"]);
    git_ok(&dir, &["commit", "-q", "-m", "root"]);
    for i in 1..=n {
        write(&dir, &format!("f{i}.txt"), &format!("{i}\n"));
        git_ok(&dir, &["add", "-A"]);
        git_ok(&dir, &["commit", "-q", "-m", &format!("c{i}")]);
    }
    dir
}

fn write(dir: &Path, file: &str, text: &str) {
    assert!(std::fs::write(dir.join(file), text).is_ok());
}

fn read(dir: &Path, file: &str) -> String {
    std::fs::read_to_string(dir.join(file)).unwrap_or_default()
}

fn git_ok(dir: &Path, args: &[&str]) -> String {
    let out = git::run(Some(dir), args);
    assert!(out.is_ok(), "git {args:?}: {out:?}");
    out.unwrap_or_default()
}

/// The subjects of the branch, newest first.
fn subjects(dir: &Path) -> Vec<String> {
    git_ok(dir, &["log", "--format=%s"])
        .lines()
        .map(str::to_owned)
        .collect()
}

/// A file as the commit `rev` has it.
fn at(dir: &Path, rev: &str, file: &str) -> String {
    git_ok(dir, &["show", &format!("{rev}:{file}")])
}

fn staged(dir: &Path) -> String {
    git_ok(dir, &["diff", "--cached"])
}

/// A tab on `dir`, read once, as the window would have it.
fn tab(dir: &Path, ctx: &egui::Context) -> RepoTab {
    let mut tab = RepoTab::new_deferred(dir.to_path_buf(), ctx);
    tab.reload(ctx);
    settle(&mut tab, ctx);
    tab
}

/// Takes in what the tab's background work sends until it has nothing more to read or run.
fn settle(tab: &mut RepoTab, ctx: &egui::Context) {
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(30) {
        match tab.rx.recv_timeout(Duration::from_millis(300)) {
            Ok(msg) => tab.handle(msg, ctx),
            Err(_) if !tab.loading && tab.busy_op.is_none() => return,
            Err(_) => {}
        }
    }
}

/// A quiet message queue does not mean an asynchronous read has finished. Wait
/// for the state requested by the test, even when Git takes longer than 300 ms.
fn wait_for(tab: &mut RepoTab, ctx: &egui::Context, label: &str, ready: impl Fn(&RepoTab) -> bool) {
    let start = Instant::now();
    while !ready(tab) {
        assert!(
            start.elapsed() < Duration::from_secs(30),
            "waiting for {label}: loading={}, busy={:?}, status={:?}",
            tab.loading,
            tab.busy_op,
            tab.status
        );
        if let Ok(msg) = tab.rx.recv_timeout(Duration::from_millis(100)) {
            tab.handle(msg, ctx);
        }
    }
}

/// Waits for the operation the tab started, takes it in (undo history, the reload after), and
/// says how it ended.
fn finish(tab: &mut RepoTab, ctx: &egui::Context) -> Option<Result<String, String>> {
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(60) {
        let Ok(msg) = tab.rx.recv_timeout(Duration::from_secs(1)) else {
            continue;
        };
        let result = match &msg {
            Msg::OpDone { result, .. } => Some(result.clone()),
            _ => None,
        };
        tab.handle(msg, ctx);
        if result.is_some() {
            settle(tab, ctx);
            return result;
        }
    }
    None
}

/// Reads the fold into `target` as the dialog gets it.
fn plan(dir: &Path, target: &str) -> Result<Plan, String> {
    git::fold_read(dir, target).and_then(Plan::from_read)
}

/// Folds into `target` (or commits a fixup! for it, `keep`) as the dialog would, and takes in
/// how it ended.
fn fold(
    tab: &mut RepoTab,
    ctx: &egui::Context,
    target: &str,
    keep: bool,
) -> Option<Result<String, String>> {
    let plan = plan(&tab.path, target);
    assert!(plan.is_ok(), "{plan:?}");
    let mut plan = plan.ok()?;
    plan.keep_fixup = keep;
    tab.apply(Action::RunFold(Box::new(plan)), ctx);
    finish(tab, ctx)
}

fn undo(tab: &mut RepoTab, ctx: &egui::Context) -> Option<Result<String, String>> {
    tab.apply(Action::Undo, ctx);
    finish(tab, ctx)
}

fn redo(tab: &mut RepoTab, ctx: &egui::Context) -> Option<Result<String, String>> {
    tab.apply(Action::Redo, ctx);
    finish(tab, ctx)
}

fn done(result: Option<Result<String, String>>) {
    assert!(matches!(result, Some(Ok(_))), "{result:?}");
}

// Keep fixture helpers and qualified test names in this module.
include!("tests/history.rs");

// Keep fixture helpers and qualified test names in this module.
include!("tests/conflicts.rs");

/// A commit made after the dialog read the branch (in the terminal): the fold refuses rather
/// than rewrite what it didn't show, and nothing changes.
#[test]
fn a_commit_made_since_the_dialog_stops_the_fold() {
    let dir = repo("moved", 2);
    let ctx = egui::Context::default();
    let mut tab = tab(&dir, &ctx);
    write(&dir, "f1.txt", "1 fixed\n");
    git_ok(&dir, &["add", "f1.txt"]);
    let read = plan(&dir, "HEAD~1");
    // `--only` with no paths: an empty commit, the staged change left staged.
    git_ok(
        &dir,
        &["commit", "-q", "--only", "--allow-empty", "-m", "since"],
    );
    let since = git::head(&dir);
    assert!(read.is_ok(), "{read:?}");
    let Ok(read) = read else { return };
    tab.apply(Action::RunFold(Box::new(read)), &ctx);
    let result = finish(&mut tab, &ctx);
    assert!(matches!(result, Some(Err(_))), "{result:?}");
    assert_eq!(git::head(&dir), since);
    assert!(!staged(&dir).is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}

/// Merges after the commit (which a rebase would flatten) are refused before anything runs,
/// as are commits off the branch.
#[test]
fn merges_after_the_commit_and_other_branches_are_refused() {
    let dir = repo("merges", 2);
    git_ok(&dir, &["checkout", "-q", "-b", "side", "HEAD~1"]);
    write(&dir, "side.txt", "side\n");
    git_ok(&dir, &["add", "-A"]);
    git_ok(&dir, &["commit", "-q", "-m", "side"]);
    let side = git::head(&dir);
    git_ok(&dir, &["checkout", "-q", "main"]);
    git_ok(&dir, &["merge", "-q", "--no-ff", "-m", "merge", "side"]);
    write(&dir, "f1.txt", "1 fixed\n");
    git_ok(&dir, &["add", "f1.txt"]);
    let refused = |target: &str| plan(&dir, target).err().unwrap_or_default();
    assert!(refused("HEAD~2").contains("Merges came after"));
    assert!(refused("HEAD").contains("A merge commit"));
    git_ok(&dir, &["checkout", "-q", "-b", "other", "HEAD~2"]);
    assert!(refused(&side).contains("not on the checked-out branch"));
    git_ok(&dir, &["checkout", "-q", "--detach"]);
    assert!(refused("HEAD~1").contains("detached"));
    let _ = std::fs::remove_dir_all(&dir);
    let clean = repo("nothing-staged", 2);
    assert!(
        plan(&clean, "HEAD~1")
            .err()
            .is_some_and(|e| e.contains("Nothing is staged"))
    );
    let _ = std::fs::remove_dir_all(&clean);
}

/// The commits it rewrites that a remote already has are named, for the force-push warning.
#[test]
fn says_which_commits_are_pushed() {
    let dir = repo("pushed", 3);
    git_ok(&dir, &["update-ref", "refs/remotes/origin/main", "HEAD~1"]);
    write(&dir, "f1.txt", "1 fixed\n");
    git_ok(&dir, &["add", "f1.txt"]);
    let found = plan(&dir, "HEAD~2");
    let pushed = found.as_ref().map(|p| p.pushed.len());
    assert_eq!(pushed, Ok(2));
    assert!(found.is_ok_and(|p| p.rewrites_pushed()));
    let _ = std::fs::remove_dir_all(&dir);
}

// Keep fixture helpers and qualified test names in this module.
include!("tests/hooks.rs");

/// The production dispatcher submits the displayed plan only after primary activation.
/// Unfocused Enter cannot borrow the dialog-wide shortcut, including for pushed plans.
#[test]
fn the_dialog_runs_the_fold_it_shows() {
    use egui::accesskit::Role;
    use egui_kittest::{Harness, kittest::Queryable};

    let dir = repo("dialog", 3);
    git_ok(&dir, &["update-ref", "refs/remotes/origin/main", "HEAD"]);
    write(&dir, "f1.txt", "1 fixed\n");
    git_ok(&dir, &["add", "f1.txt"]);
    let found = plan(&dir, "HEAD~2");
    let _ = std::fs::remove_dir_all(&dir);
    assert!(found.is_ok(), "{found:?}");
    let Ok(found) = found else { return };
    let harness = |plan: Plan| {
        let ctx = egui::Context::default();
        let mut tab = RepoTab::new_deferred("/nonexistent/fold-dialog-test".into(), &ctx);
        tab.dialog = Some(Dialog::Fold(Box::new(plan)));
        let fonts = std::cell::Cell::new(false);
        let mut h = Harness::new_ui_state(
            move |ui, state: &mut (RepoTab, Vec<Action>)| {
                if !fonts.replace(true) {
                    crate::theme::install_fonts(ui.ctx());
                    return;
                }
                state.0.dialogs(ui.ctx(), &mut state.1);
            },
            (tab, Vec::new()),
        );
        h.run_steps(3);
        h
    };
    let ran = |acts: &[Action]| {
        acts.iter().find_map(|a| match a {
            Action::RunFold(plan) => Some(plan.keep_fixup),
            _ => None,
        })
    };

    assert!(found.rewrites_pushed());
    let mut pushed = harness(found.clone());
    pushed.key_press(egui::Key::Enter);
    pushed.run_steps(3);
    assert_eq!(ran(&pushed.state().1), None);
    assert!(pushed.state().0.dialog.is_some());

    for keep_fixup in [false, true] {
        let shown = Plan {
            keep_fixup,
            pushed: if keep_fixup {
                found.pushed.clone()
            } else {
                Vec::new()
            },
            ..found.clone()
        };
        let mut h = harness(shown);
        h.key_press(egui::Key::Enter);
        h.run_steps(3);
        assert_eq!(ran(&h.state().1), None);
        assert!(h.state().0.dialog.is_some());
        let label = crate::i18n::text(if keep_fixup {
            "repo-core-1314"
        } else {
            "repo-core-1315"
        });
        h.get_by_role_and_label(Role::Button, label).focus();
        h.key_press(egui::Key::Enter);
        h.run_steps(3);
        assert_eq!(ran(&h.state().1), Some(keep_fixup));
        assert!(h.state().0.dialog.is_none());
    }
}

/// The fixup commit went in but the rebase failed without starting: the commit gets an undo of
/// its own, which stages the changes again, and redo commits them again.
#[test]
fn a_rebase_that_never_started_leaves_an_undoable_fixup_commit() {
    let dir = repo("never-started", 3);
    let ctx = egui::Context::default();
    let mut tab = tab(&dir, &ctx);
    let before = git::head(&dir);
    write(&dir, "f2.txt", "2 fixed\n");
    git_ok(&dir, &["add", "f2.txt"]);
    let was_staged = staged(&dir);
    let found = plan(&dir, "HEAD~1");
    assert!(found.is_ok(), "{found:?}");
    let Ok(found) = found else { return };
    // A rebase git refuses outright, as one that can't begin would be.
    let refused = vec!["rebase".to_owned(), "--no-such-option".to_owned()];
    tab.fold_ready(&found, Ok((refused, String::new())), &ctx);
    let result = finish(&mut tab, &ctx);
    assert!(matches!(result, Some(Err(_))), "{result:?}");
    assert_eq!(subjects(&dir)[0], "fixup! c2");
    assert!(tab.fold.stopped.is_none());
    let fixup = git::head(&dir);
    let recovery = tab
        .timeline
        .current_entry()
        .ok_or("missing recovery entry")
        .cloned();
    assert!(recovery.is_ok());
    let Ok(recovery) = recovery else { return };
    assert_eq!(
        recovery.before.head, before,
        "recovery starts before the fixup commit"
    );
    assert_eq!(recovery.after.head, fixup);
    assert_eq!(
        tab.timeline.entries.len(),
        1,
        "one operation, including its recovery"
    );

    done(undo(&mut tab, &ctx));
    assert_eq!(git::head(&dir), before);
    assert_eq!(staged(&dir), was_staged);
    done(redo(&mut tab, &ctx));
    assert_eq!(git::head(&dir), fixup);
    let _ = std::fs::remove_dir_all(&dir);
}

/// Staged changes that undo what a later commit does would meld into nothing, the later commit
/// bringing its change back: the fold is taken back, the branch, files and index as before.
#[test]
fn staged_changes_a_later_commit_would_undo_are_never_lost() {
    // c4 changes f1.txt; staged, it goes back to what c1 made. And c2's file deleted.
    let dir = repo("undone-later", 3);
    write(&dir, "f1.txt", "three\n");
    git_ok(&dir, &["commit", "-q", "-am", "c4"]);
    let ctx = egui::Context::default();
    let mut tab = tab(&dir, &ctx);
    let before = git::head(&dir);
    write(&dir, "f1.txt", "1\n");
    git_ok(&dir, &["add", "f1.txt"]);
    git_ok(&dir, &["rm", "-q", "f2.txt"]);
    write(&dir, "f3.txt", "3 unstaged\n");
    let was_staged = staged(&dir);

    let result = fold(&mut tab, &ctx, "HEAD~3", false);
    assert!(matches!(result, Some(Err(_))), "{result:?}");
    // The going back ran on its own, right after (taken in while settling).
    assert_eq!(git::head(&dir), before);
    assert_eq!(staged(&dir), was_staged);
    assert_eq!(read(&dir, "f1.txt"), "1\n");
    assert!(!dir.join("f2.txt").exists());
    assert_eq!(read(&dir, "f3.txt"), "3 unstaged\n");
    assert_eq!(git::in_progress(&dir), Ok(None));
    let said = tab
        .status
        .as_deref()
        .cloned()
        .map(|(s, _)| s)
        .unwrap_or_default();
    assert!(said.contains("nothing was changed"), "{said}");
    let _ = std::fs::remove_dir_all(&dir);
}

// Keep fixture helpers and qualified test names in this module.
include!("tests/collision_guards.rs");
