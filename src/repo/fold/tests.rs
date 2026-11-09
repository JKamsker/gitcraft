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

/// Folding into the commit before HEAD: that commit gets the change, every commit keeps its
/// message, nothing stays staged, and one undo puts the branch and the staged change back.
#[test]
fn folds_into_the_commit_before_head_and_undoes_in_one_step() {
    let dir = repo("head-1", 3);
    let ctx = egui::Context::default();
    let mut tab = tab(&dir, &ctx);
    let before = git::head(&dir);
    write(&dir, "f2.txt", "2 fixed\n");
    git_ok(&dir, &["add", "f2.txt"]);
    let was_staged = staged(&dir);
    let undos = tab
        .timeline
        .entries
        .iter()
        .filter(|e| e.undo.is_some())
        .count();

    done(fold(&mut tab, &ctx, "HEAD~1", false));
    let folded = git::head(&dir);
    assert_eq!(subjects(&dir), ["c3", "c2", "c1", "root"]);
    assert_eq!(at(&dir, "HEAD~1", "f2.txt"), "2 fixed\n");
    assert_eq!(at(&dir, "HEAD~2", "f1.txt"), "1\n");
    assert_eq!(git_ok(&dir, &["rev-parse", "HEAD~2"]).trim(), {
        git_ok(&dir, &["rev-parse", &format!("{before}~2")])
            .trim()
            .to_owned()
    });
    assert!(staged(&dir).is_empty());
    assert_eq!(git_ok(&dir, &["status", "--porcelain"]), "");
    assert_eq!(
        tab.timeline
            .entries
            .iter()
            .filter(|e| e.undo.is_some())
            .count(),
        undos + 1
    );

    done(undo(&mut tab, &ctx));
    assert_eq!(git::head(&dir), before);
    assert_eq!(staged(&dir), was_staged);
    assert_eq!(read(&dir, "f2.txt"), "2 fixed\n");

    done(redo(&mut tab, &ctx));
    assert_eq!(git::head(&dir), folded);
    assert!(staged(&dir).is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}

/// Five commits back, with the four after it replayed unchanged.
#[test]
fn folds_into_a_commit_five_back() {
    let dir = repo("head-5", 6);
    let ctx = egui::Context::default();
    let mut tab = tab(&dir, &ctx);
    write(&dir, "f1.txt", "1 fixed\n");
    git_ok(&dir, &["add", "f1.txt"]);

    done(fold(&mut tab, &ctx, "HEAD~5", false));
    assert_eq!(subjects(&dir), ["c6", "c5", "c4", "c3", "c2", "c1", "root"]);
    assert_eq!(at(&dir, "HEAD~5", "f1.txt"), "1 fixed\n");
    for (rev, file, text) in [("HEAD", "f6.txt", "6\n"), ("HEAD~2", "f4.txt", "4\n")] {
        assert_eq!(at(&dir, rev, file), text);
    }
    assert!(staged(&dir).is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}

/// The first commit, which has no parent to rebase from (`--root`).
#[test]
fn folds_into_the_root_commit() {
    let dir = repo("root", 2);
    let ctx = egui::Context::default();
    let mut tab = tab(&dir, &ctx);
    let root = git_ok(&dir, &["rev-list", "--max-parents=0", "HEAD"])
        .trim()
        .to_owned();
    write(&dir, "root.txt", "root fixed\n");
    git_ok(&dir, &["add", "root.txt"]);

    let found = plan(&dir, &root);
    assert!(found.as_ref().is_ok_and(|p| p.root && p.after.len() == 2));
    done(fold(&mut tab, &ctx, &root, false));
    assert_eq!(subjects(&dir), ["c2", "c1", "root"]);
    assert_eq!(at(&dir, "HEAD~2", "root.txt"), "root fixed\n");
    let roots = git_ok(&dir, &["rev-list", "--max-parents=0", "HEAD"]);
    assert_eq!(roots.lines().count(), 1);
    assert!(staged(&dir).is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}

/// What isn't staged stays as it was: a change to another file, a second change to the file
/// folded (only its staged part goes in), and a new file git doesn't track.
#[test]
fn unstaged_and_untracked_changes_survive() {
    let dir = repo("keeps", 3);
    let ctx = egui::Context::default();
    let mut tab = tab(&dir, &ctx);
    write(&dir, "f1.txt", "1 fixed\n");
    git_ok(&dir, &["add", "f1.txt"]);
    write(&dir, "f1.txt", "1 fixed\nand more\n");
    write(&dir, "f3.txt", "3 edited\n");
    write(&dir, "new.txt", "untracked\n");

    done(fold(&mut tab, &ctx, "HEAD~2", false));
    assert_eq!(at(&dir, "HEAD~2", "f1.txt"), "1 fixed\n");
    assert_eq!(at(&dir, "HEAD", "f3.txt"), "3\n");
    assert_eq!(read(&dir, "f1.txt"), "1 fixed\nand more\n");
    assert_eq!(read(&dir, "f3.txt"), "3 edited\n");
    assert_eq!(read(&dir, "new.txt"), "untracked\n");
    assert!(staged(&dir).is_empty());
    let status = git_ok(&dir, &["status", "--porcelain"]);
    assert!(status.contains(" M f1.txt") && status.contains(" M f3.txt"));
    assert!(status.contains("?? new.txt"));
    // No stash left behind by the rebase.
    assert_eq!(git_ok(&dir, &["stash", "list"]), "");
    let _ = std::fs::remove_dir_all(&dir);
}

/// Kept as a fixup! commit: nothing is rewritten, the commit is on top, undo stages it again.
#[test]
fn keeps_a_fixup_commit_for_later() {
    let dir = repo("keep", 3);
    let ctx = egui::Context::default();
    let mut tab = tab(&dir, &ctx);
    let before = git::head(&dir);
    write(&dir, "f2.txt", "2 fixed\n");
    git_ok(&dir, &["add", "f2.txt"]);
    let was_staged = staged(&dir);

    done(fold(&mut tab, &ctx, "HEAD~1", true));
    assert_eq!(subjects(&dir), ["fixup! c2", "c3", "c2", "c1", "root"]);
    assert_eq!(git_ok(&dir, &["rev-parse", "HEAD~1"]).trim(), before);
    assert!(staged(&dir).is_empty());

    done(undo(&mut tab, &ctx));
    assert_eq!(git::head(&dir), before);
    assert_eq!(staged(&dir), was_staged);
    let _ = std::fs::remove_dir_all(&dir);
}

/// A repository where folding `f1.txt` into c1 conflicts with c3, which changed it since.
fn conflicting(name: &str) -> (PathBuf, String, String) {
    let dir = repo(name, 3);
    write(&dir, "f1.txt", "three\n");
    git_ok(&dir, &["commit", "-q", "-am", "c4"]);
    let before = git::head(&dir);
    write(&dir, "f1.txt", "fixed\n");
    git_ok(&dir, &["add", "f1.txt"]);
    let was_staged = staged(&dir);
    (dir, before, was_staged)
}

/// The fold whose rebase stopped hands over to the conflict flow; aborting goes back to before
/// the fold, the change staged again (not left in a fixup! commit).
#[test]
fn a_conflict_aborted_goes_back_to_before_the_fold() {
    let (dir, before, was_staged) = conflicting("abort");
    let ctx = egui::Context::default();
    let mut tab = tab(&dir, &ctx);
    write(&dir, "notes.txt", "untracked\n");

    let result = fold(&mut tab, &ctx, "HEAD~3", false);
    assert!(
        matches!(&result, Some(Err(e)) if e.contains("CONFLICT")),
        "{result:?}"
    );
    assert_eq!(tab.work.in_progress, Some(InProgress::Rebase));
    assert!(tab.fold.stopped.is_some());
    let said = tab
        .status
        .as_deref()
        .cloned()
        .map(|(s, _)| s)
        .unwrap_or_default();
    assert!(said.contains("stopped on a conflict"), "{said}");
    assert!(tab.fold_stopped_here(InProgress::Rebase).is_some());
    assert!(matches!(
        tab.abort_for(InProgress::Rebase),
        Action::AbortFold
    ));
    tab.apply(Action::AbortFold, &ctx);
    wait_for(&mut tab, &ctx, "abort confirmation", |t| t.dialog.is_some());
    let steps = match tab.dialog.take() {
        Some(Dialog::Confirm { ops, title, .. }) => Some((title, ops)),
        _ => None,
    };
    assert!(
        steps.as_ref().is_some_and(|(_, ops)| ops.len() == 4),
        "the fold's abort"
    );
    let Some((label, ops)) = steps else { return };
    tab.apply(
        Action::Ops {
            label,
            ops,
            refresh: crate::repo::Refresh::All,
        },
        &ctx,
    );
    done(finish(&mut tab, &ctx));
    assert_eq!(git::in_progress(&dir), Ok(None));
    assert_eq!(git::head(&dir), before);
    assert_eq!(staged(&dir), was_staged);
    assert_eq!(read(&dir, "notes.txt"), "untracked\n");
    assert!(tab.fold.stopped.is_none());
    let _ = std::fs::remove_dir_all(&dir);
}

/// Resolved and continued (twice: the fixup, then the later commit that changed the file),
/// the fold is one undo, back to before it with the change staged.
#[test]
fn a_conflict_continued_is_one_undo() {
    let (dir, before, was_staged) = conflicting("continue");
    let ctx = egui::Context::default();
    let mut tab = tab(&dir, &ctx);

    let result = fold(&mut tab, &ctx, "HEAD~3", false);
    assert!(matches!(result, Some(Err(_))), "{result:?}");
    let undos = tab
        .timeline
        .entries
        .iter()
        .filter(|e| e.undo.is_some())
        .count();
    for _ in 0..4 {
        if tab.work.in_progress != Some(InProgress::Rebase) {
            break;
        }
        // Each side's line made one.
        let text = read(&dir, "f1.txt");
        let resolved = if text.contains("three") {
            "fixed\n"
        } else {
            "1 fixed\n"
        };
        write(&dir, "f1.txt", resolved);
        git_ok(&dir, &["add", "f1.txt"]);
        tab.reload_status(&ctx);
        let read = tab.reads;
        wait_for(&mut tab, &ctx, "resolved status", |t| {
            t.work_read >= read && t.work.conflicted.is_empty()
        });
        tab.apply(Action::Commit, &ctx);
        assert!(
            finish(&mut tab, &ctx).is_some(),
            "continuing the rebase did not start an operation"
        );
    }
    assert_eq!(git::in_progress(&dir), Ok(None));
    assert_eq!(subjects(&dir), ["c4", "c3", "c2", "c1", "root"]);
    assert_eq!(at(&dir, "HEAD~3", "f1.txt"), "1 fixed\n");
    assert_eq!(at(&dir, "HEAD", "f1.txt"), "fixed\n");
    assert_eq!(
        tab.timeline
            .entries
            .iter()
            .filter(|e| e.undo.is_some())
            .count(),
        undos + 1
    );
    assert!(tab.fold.stopped.is_none());
    let folded = git::head(&dir);

    done(undo(&mut tab, &ctx));
    assert_eq!(git::head(&dir), before);
    assert_eq!(staged(&dir), was_staged);
    done(redo(&mut tab, &ctx));
    assert_eq!(git::head(&dir), folded);
    let _ = std::fs::remove_dir_all(&dir);
}

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

/// A shell script git runs (a hook, a signing program).
#[cfg(unix)]
fn script(path: &Path, body: &str) {
    use std::os::unix::fs::PermissionsExt;
    assert!(std::fs::write(path, format!("#!/bin/sh\n{body}")).is_ok());
    assert!(std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).is_ok());
}

/// Commits signed by default: the fixup commit and the rewritten ones are signed (here by a
/// stand-in for gpg), and the fold goes through.
#[cfg(unix)]
#[test]
fn signed_commits_stay_signed() {
    let dir = repo("signed", 3);
    let gpg = dir.join(".git").join("fake-gpg");
    script(
        &gpg,
        "cat >/dev/null\nprintf '\\n[GNUPG:] SIG_CREATED D 1 8 00 0 X\\n' >&2\n\
         printf -- '-----BEGIN PGP SIGNATURE-----\\n\\nAA==\\n-----END PGP SIGNATURE-----\\n'\n",
    );
    let gpg = gpg.to_string_lossy().into_owned();
    for args in [
        &["config", "commit.gpgsign", "true"][..],
        &["config", "gpg.program", &gpg],
        &["config", "user.signingkey", "X"],
    ] {
        git_ok(&dir, args);
    }
    let ctx = egui::Context::default();
    let mut tab = tab(&dir, &ctx);
    write(&dir, "f2.txt", "2 fixed\n");
    git_ok(&dir, &["add", "f2.txt"]);

    done(fold(&mut tab, &ctx, "HEAD~1", false));
    assert_eq!(at(&dir, "HEAD~1", "f2.txt"), "2 fixed\n");
    for rev in ["HEAD", "HEAD~1"] {
        assert!(git_ok(&dir, &["cat-file", "commit", rev]).contains("gpgsig"));
    }
    let _ = std::fs::remove_dir_all(&dir);
}

/// Commit hooks run, and one that changes the fixup commit's message (a ticket number in front,
/// a trailer after) doesn't stop it melding in: the fixup commit isn't found by its message.
#[cfg(unix)]
#[test]
fn commit_hooks_dont_break_it() {
    let dir = repo("hooks", 3);
    let hooks = dir.join(".git").join("hooks");
    let _ = std::fs::create_dir_all(&hooks);
    let ran = dir.join(".git").join("pre-commit-ran");
    script(
        &hooks.join("pre-commit"),
        &format!("touch '{}'\n", ran.display()),
    );
    script(
        &hooks.join("prepare-commit-msg"),
        "case \"$(head -n 1 \"$1\")\" in fixup!*) \
         printf '[T-1] %s' \"$(cat \"$1\")\" > \"$1.new\" && mv \"$1.new\" \"$1\";; esac\n",
    );
    script(
        &hooks.join("commit-msg"),
        "printf '\\nChange-Id: I123\\n' >> \"$1\"\n",
    );
    let ctx = egui::Context::default();
    let mut tab = tab(&dir, &ctx);
    write(&dir, "f2.txt", "2 fixed\n");
    git_ok(&dir, &["add", "f2.txt"]);

    done(fold(&mut tab, &ctx, "HEAD~1", false));
    assert!(ran.exists());
    // The hook did rename the fixup commit before it melded in.
    let reflog = git_ok(&dir, &["log", "-g", "--format=%gs"]);
    assert!(reflog.contains("[T-1] fixup! c2"), "{reflog}");
    assert_eq!(subjects(&dir), ["c3", "c2", "c1", "root"]);
    assert_eq!(at(&dir, "HEAD~1", "f2.txt"), "2 fixed\n");
    let _ = std::fs::remove_dir_all(&dir);
}

/// A pre-commit hook that refuses: nothing is committed or rewritten, the change stays
/// staged, and no fold is left waiting to be aborted.
#[cfg(unix)]
#[test]
fn a_refusing_pre_commit_hook_changes_nothing() {
    let dir = repo("hook-refuses", 2);
    let hooks = dir.join(".git").join("hooks");
    let _ = std::fs::create_dir_all(&hooks);
    script(
        &hooks.join("pre-commit"),
        "echo 'lint failed' >&2\nexit 1\n",
    );
    let ctx = egui::Context::default();
    let mut tab = tab(&dir, &ctx);
    let before = git::head(&dir);
    write(&dir, "f1.txt", "1 fixed\n");
    git_ok(&dir, &["add", "f1.txt"]);
    let was_staged = staged(&dir);

    let result = fold(&mut tab, &ctx, "HEAD~1", false);
    assert!(
        matches!(&result, Some(Err(e)) if e.contains("lint failed")),
        "{result:?}"
    );
    assert_eq!(git::head(&dir), before);
    assert_eq!(staged(&dir), was_staged);
    assert!(tab.fold.stopped.is_none());
    let _ = std::fs::remove_dir_all(&dir);
}

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

/// A fold stopped, then aborted in the terminal and another branch's rebase stopped in its
/// place: Abort is the ordinary one for that rebase, and resets no branch to the fold's start.
#[test]
fn abort_is_the_folds_only_while_its_rebase_is_going_on() {
    let (dir, before, _) = conflicting("abort-other");
    let ctx = egui::Context::default();
    let mut tab = tab(&dir, &ctx);
    let result = fold(&mut tab, &ctx, "HEAD~3", false);
    assert!(matches!(result, Some(Err(_))), "{result:?}");
    git_ok(&dir, &["rebase", "--abort"]);
    git_ok(&dir, &["reset", "-q", "--hard", &before]);
    // Another branch, whose rebase stops on a conflict too.
    git_ok(&dir, &["checkout", "-q", "-b", "other", "HEAD~1"]);
    write(&dir, "f1.txt", "other\n");
    git_ok(&dir, &["commit", "-q", "-am", "other"]);
    let other = git::head(&dir);
    let _ = git::run(Some(&dir), &["rebase", "main"]);
    assert_eq!(git::in_progress(&dir), Ok(Some(InProgress::Rebase)));
    tab.reload(&ctx);
    settle(&mut tab, &ctx);

    tab.apply(tab.abort_for(InProgress::Rebase), &ctx);
    wait_for(&mut tab, &ctx, "abort confirmation", |t| t.dialog.is_some());
    let ops = match tab.dialog.take() {
        Some(Dialog::Confirm { ops, .. }) => ops,
        _ => Vec::new(),
    };
    assert_eq!(ops, crate::repo::ops(&[&["rebase", "--abort"]]));
    assert_eq!(git::resolve(&dir, "refs/heads/other"), Ok(other));
    let _ = git::run(Some(&dir), &["rebase", "--abort"]);
    let _ = std::fs::remove_dir_all(&dir);
}

/// An ignored file where a commit to replay has a file would be written over by the rebase
/// (git doesn't ask for ignored files): the fold is refused before anything runs.
#[test]
fn an_ignored_file_in_the_way_is_refused() {
    let dir = repo("ignored", 1);
    write(&dir, "build.log", "tracked once\n");
    git_ok(&dir, &["add", "build.log"]);
    git_ok(&dir, &["commit", "-q", "-m", "log"]);
    git_ok(&dir, &["rm", "-q", "--cached", "build.log"]);
    write(&dir, ".gitignore", "build.log\n");
    git_ok(&dir, &["add", ".gitignore"]);
    git_ok(&dir, &["commit", "-q", "-m", "ignore it"]);
    write(&dir, "build.log", "mine, not to lose\n");
    write(&dir, "f1.txt", "1 fixed\n");
    git_ok(&dir, &["add", "f1.txt"]);
    let refused = plan(&dir, "HEAD~2").err().unwrap_or_default();
    assert!(refused.contains("build.log"), "{refused}");
    assert_eq!(read(&dir, "build.log"), "mine, not to lose\n");
    let _ = std::fs::remove_dir_all(&dir);
}

/// A merge started after the dialog read the branch (the same branch, the same HEAD): the
/// fold refuses rather than commit the merge as its fixup.
#[test]
fn a_merge_started_since_the_dialog_stops_the_fold() {
    let dir = repo("merge-since", 2);
    git_ok(&dir, &["checkout", "-q", "-b", "side", "HEAD~1"]);
    write(&dir, "side.txt", "side\n");
    git_ok(&dir, &["add", "-A"]);
    git_ok(&dir, &["commit", "-q", "-m", "side"]);
    git_ok(&dir, &["checkout", "-q", "main"]);
    let ctx = egui::Context::default();
    let mut tab = tab(&dir, &ctx);
    write(&dir, "f1.txt", "1 fixed\n");
    git_ok(&dir, &["add", "f1.txt"]);
    let found = plan(&dir, "HEAD~1");
    assert!(found.is_ok(), "{found:?}");
    let Ok(found) = found else { return };
    // git merges only with nothing staged: the change staged again once it has started.
    git_ok(&dir, &["reset", "-q", "f1.txt"]);
    git_ok(&dir, &["merge", "-q", "--no-ff", "--no-commit", "side"]);
    git_ok(&dir, &["add", "f1.txt"]);
    let before = git::head(&dir);
    tab.apply(Action::RunFold(Box::new(found.clone())), &ctx);
    settle(&mut tab, &ctx);
    let said = tab
        .status
        .as_deref()
        .cloned()
        .map(|(s, _)| s)
        .unwrap_or_default();
    assert!(said.contains("merge is in progress"), "{said}");
    assert_eq!(git::head(&dir), before);
    // And should it start between that check and the commit, the steps' own checks refuse.
    let Ok(steps) = found.editor().and_then(|e| e.command(&dir)) else {
        return;
    };
    tab.fold_ready(&found, Ok((steps, String::new())), &ctx);
    let result = finish(&mut tab, &ctx);
    assert!(matches!(result, Some(Err(_))), "{result:?}");
    assert_eq!(git::head(&dir), before);
    assert_eq!(git::in_progress(&dir), Ok(Some(InProgress::Merge)));
    let _ = std::fs::remove_dir_all(&dir);
}

/// A `git am` stopped after the dialog read the branch (no ref says so, only its folder): the
/// fold sees it right before it runs, and commits nothing.
#[test]
fn a_patch_stopped_since_the_dialog_stops_the_fold() {
    let dir = repo("am-since", 2);
    git_ok(&dir, &["checkout", "-q", "-b", "side", "HEAD~1"]);
    write(&dir, "f2.txt", "side\n");
    git_ok(&dir, &["add", "-A"]);
    git_ok(&dir, &["commit", "-q", "-m", "side"]);
    let patch = git_ok(&dir, &["format-patch", "-1", "--stdout"]);
    write(&dir, ".git/side.patch", &patch);
    git_ok(&dir, &["checkout", "-q", "main"]);
    let ctx = egui::Context::default();
    let mut tab = tab(&dir, &ctx);
    write(&dir, "f1.txt", "1 fixed\n");
    git_ok(&dir, &["add", "f1.txt"]);
    let found = plan(&dir, "HEAD~1");
    assert!(found.is_ok(), "{found:?}");
    let Ok(found) = found else { return };
    // f2.txt differs on main: the patch stops. Staged meanwhile, the fix stays staged.
    git_ok(&dir, &["reset", "-q", "f1.txt"]);
    let stopped = git::run(Some(&dir), &["am", "-q", ".git/side.patch"]);
    git_ok(&dir, &["add", "f1.txt"]);
    assert!(stopped.is_err());
    assert_eq!(git::in_progress(&dir), Ok(Some(InProgress::Am)));
    let before = git::head(&dir);
    tab.apply(Action::RunFold(Box::new(found)), &ctx);
    assert!(
        tab.busy_op.is_some(),
        "the preflight check holds the operation busy"
    );
    settle(&mut tab, &ctx);
    let said = tab
        .status
        .as_deref()
        .cloned()
        .map(|(s, _)| s)
        .unwrap_or_default();
    assert!(said.contains("patch is in progress"), "{said}");
    assert_eq!(git::head(&dir), before);
    let _ = git::run(Some(&dir), &["am", "--abort"]);
    let _ = std::fs::remove_dir_all(&dir);
}

/// The fold's abort asked for, then (while its dialog was open) the rebase replaced in the
/// terminal by one of another branch: confirming refuses, and that rebase goes on untouched.
#[test]
fn an_abort_confirmed_after_the_rebase_was_replaced_resets_no_other_branch() {
    let (dir, before, _) = conflicting("abort-replaced");
    let ctx = egui::Context::default();
    let mut tab = tab(&dir, &ctx);
    let result = fold(&mut tab, &ctx, "HEAD~3", false);
    assert!(matches!(result, Some(Err(_))), "{result:?}");
    tab.apply(Action::AbortFold, &ctx);
    wait_for(&mut tab, &ctx, "abort confirmation", |t| t.dialog.is_some());
    let asked = match tab.dialog.take() {
        Some(Dialog::Confirm { ops, title, .. }) => Some((title, ops)),
        _ => None,
    };
    assert!(asked.is_some());
    let Some((label, ops)) = asked else { return };
    // Meanwhile, in the terminal: the fold's rebase aborted, another branch's started.
    git_ok(&dir, &["rebase", "--abort"]);
    git_ok(&dir, &["checkout", "-q", "-b", "other", &before]);
    write(&dir, "f1.txt", "other\n");
    git_ok(&dir, &["commit", "-q", "-am", "other"]);
    let other = git::head(&dir);
    git_ok(&dir, &["checkout", "-q", "-b", "third", "HEAD~1"]);
    write(&dir, "f1.txt", "third\n");
    git_ok(&dir, &["commit", "-q", "-am", "third"]);
    let _ = git::run(Some(&dir), &["rebase", "other"]);
    assert_eq!(git::in_progress(&dir), Ok(Some(InProgress::Rebase)));
    let third = git::resolve(&dir, "refs/heads/third");

    tab.apply(
        Action::Ops {
            label,
            ops,
            refresh: crate::repo::Refresh::All,
        },
        &ctx,
    );
    let result = finish(&mut tab, &ctx);
    assert!(matches!(result, Some(Err(_))), "{result:?}");
    // The other rebase goes on, its branches as they were.
    assert_eq!(git::in_progress(&dir), Ok(Some(InProgress::Rebase)));
    assert_eq!(
        git::rebasing_branch(&dir),
        Ok(Some("refs/heads/third".to_owned()))
    );
    assert_eq!(git::resolve(&dir, "refs/heads/third"), third);
    assert_eq!(git::resolve(&dir, "refs/heads/other"), Ok(other));
    let _ = std::fs::remove_dir_all(&dir);
}

/// The same for an ignored file whose name has spaces around it, and one where a commit to
/// replay renamed a file away from (its old name is written when the commit before is).
#[test]
fn ignored_files_named_with_spaces_or_renamed_away_are_refused() {
    // Windows takes trailing spaces off file names: such a file can't be there.
    let cases: &[(&str, bool)] = if cfg!(windows) {
        &[("old-name", true)]
    } else {
        &[(" collision ", false), ("old-name", true)]
    };
    for &(name, rename) in cases {
        let dir = repo(
            if rename {
                "ignored-renamed"
            } else {
                "ignored-spaces"
            },
            1,
        );
        write(&dir, name, "tracked once\n");
        git_ok(&dir, &["add", "-A"]);
        git_ok(&dir, &["commit", "-q", "-m", "add it"]);
        if rename {
            git_ok(&dir, &["mv", name, "new-name"]);
            git_ok(&dir, &["commit", "-q", "-m", "rename it"]);
            git_ok(&dir, &["rm", "-q", "new-name"]);
        } else {
            git_ok(&dir, &["rm", "-q", name]);
        }
        git_ok(&dir, &["commit", "-q", "-m", "remove it"]);
        let exclude = dir.join(".git").join("info").join("exclude");
        // Spaces escaped: a pattern's trailing ones would be dropped.
        let pattern = name.replace(' ', "\\ ");
        assert!(std::fs::write(&exclude, format!("{pattern}\n")).is_ok());
        write(&dir, name, "mine, not to lose\n");
        write(&dir, "f1.txt", "1 fixed\n");
        git_ok(&dir, &["add", "f1.txt"]);
        let target = if rename { "HEAD~1" } else { "HEAD~2" };
        let refused = plan(&dir, target).err().unwrap_or_default();
        assert!(refused.contains(name), "{name:?}: {refused}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}

#[test]
fn ignored_file_created_after_dialog_is_refused_by_the_mutation_worker() -> Result<(), String> {
    let dir = repo("ignored-since-dialog", 1);
    write(&dir, "build.log", "tracked once\n");
    git_ok(&dir, &["add", "build.log"]);
    git_ok(&dir, &["commit", "-qm", "log"]);
    git_ok(&dir, &["rm", "-q", "build.log"]);
    write(&dir, ".gitignore", "build.log\n");
    git_ok(&dir, &["add", ".gitignore"]);
    git_ok(&dir, &["commit", "-qm", "ignore"]);
    write(&dir, "f1.txt", "fixed\n");
    git_ok(&dir, &["add", "f1.txt"]);
    let found = plan(&dir, "HEAD~2")?;
    let before = git::head(&dir);
    write(&dir, "build.log", "private ignored content\n");
    let (steps, _) = found.fold_steps(vec!["--must-never-run".into()]);
    let (result, _) =
        crate::repo::operations::run_steps(&dir, &steps, crate::repo::OpKind::plain(), None);
    assert!(result.is_err());
    assert!(result.err().unwrap_or_default().contains("build.log"));
    assert_eq!(git::head(&dir), before);
    assert_eq!(read(&dir, "build.log"), "private ignored content\n");
    let _ = std::fs::remove_dir_all(&dir);
    Ok(())
}

#[test]
fn staged_deletion_is_included_in_fold_collision_paths() -> Result<(), String> {
    let dir = repo("staged-delete-collision", 2);
    write(&dir, ".git/info/exclude", "root.txt\n");
    git_ok(&dir, &["rm", "--cached", "root.txt"]);
    let read = git::fold_read(&dir, "HEAD~1")?;
    assert_eq!(read.ignored_in_way.as_deref(), Some("root.txt"));
    assert!(Plan::from_read(read).is_err());
    assert_eq!(self::read(&dir, "root.txt"), "root\n");
    let _ = std::fs::remove_dir_all(dir);
    Ok(())
}

#[cfg(unix)]
#[test]
fn fixup_hook_ignored_collision_stops_before_rebase() -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    let dir = repo("hook-ignored-collision", 1);
    write(&dir, "build.log", "historical\n");
    git_ok(&dir, &["add", "build.log"]);
    git_ok(&dir, &["commit", "-qm", "historical log"]);
    git_ok(&dir, &["rm", "build.log"]);
    write(&dir, ".gitignore", "build.log\n");
    git_ok(&dir, &["add", ".gitignore"]);
    git_ok(&dir, &["commit", "-qm", "ignore log"]);
    write(&dir, "f1.txt", "fixed\n");
    git_ok(&dir, &["add", "f1.txt"]);
    let found = plan(&dir, "HEAD~2")?;
    let before = git::head(&dir);
    write(
        &dir,
        ".git/hooks/pre-commit",
        "#!/bin/sh\nprintf 'private hook output\\n' > build.log\n",
    );
    std::fs::set_permissions(
        dir.join(".git/hooks/pre-commit"),
        std::fs::Permissions::from_mode(0o700),
    )
    .map_err(|e| e.to_string())?;
    let (steps, _) = found.fold_steps(vec!["--must-never-run".into()]);
    let (result, _) =
        crate::repo::operations::run_steps(&dir, &steps, crate::repo::OpKind::plain(), None);
    assert!(result.err().unwrap_or_default().contains("build.log"));
    assert_ne!(git::head(&dir), before);
    assert_eq!(git_ok(&dir, &["rev-parse", "HEAD^"]).trim(), before);
    assert_eq!(read(&dir, "build.log"), "private hook output\n");
    assert!(git::in_progress(&dir)?.is_none());
    let _ = std::fs::remove_dir_all(dir);
    Ok(())
}
