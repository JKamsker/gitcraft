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
    let undos = tab.undo_stack.len();

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
    assert_eq!(tab.undo_stack.len(), undos + 1);

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
    let said = tab.status.clone().map(|(s, _)| s).unwrap_or_default();
    assert!(said.contains("stopped on a conflict"), "{said}");
    assert!(tab.fold_stopped_here(InProgress::Rebase).is_some());
    assert!(matches!(
        tab.abort_for(InProgress::Rebase),
        Action::AbortFold
    ));
    tab.apply(Action::AbortFold, &ctx);
    settle(&mut tab, &ctx);
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
    let undos = tab.undo_stack.len();
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
        settle(&mut tab, &ctx);
        tab.apply(Action::Commit, &ctx);
        let _ = finish(&mut tab, &ctx);
    }
    assert_eq!(git::in_progress(&dir), Ok(None));
    assert_eq!(subjects(&dir), ["c4", "c3", "c2", "c1", "root"]);
    assert_eq!(at(&dir, "HEAD~3", "f1.txt"), "1 fixed\n");
    assert_eq!(at(&dir, "HEAD", "f1.txt"), "fixed\n");
    assert_eq!(tab.undo_stack.len(), undos + 1);
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

/// The dialog, drawn: Enter folds as shown (unless that rewrites pushed commits), and ticking
/// _Keep as fixup!_ makes that what runs.
#[test]
fn the_dialog_runs_the_fold_it_shows() {
    let dir = repo("dialog", 3);
    git_ok(&dir, &["update-ref", "refs/remotes/origin/main", "HEAD"]);
    write(&dir, "f1.txt", "1 fixed\n");
    git_ok(&dir, &["add", "f1.txt"]);
    let found = plan(&dir, "HEAD~2");
    let _ = std::fs::remove_dir_all(&dir);
    assert!(found.is_ok(), "{found:?}");
    let Ok(found) = found else { return };
    let ctx = egui::Context::default();
    crate::theme::install_fonts(&ctx);
    let mut tab = RepoTab::new_deferred("/nonexistent/fold-dialog-test".into(), &ctx);
    let frame = |tab: &mut RepoTab, events: Vec<egui::Event>| {
        let mut acts = Vec::new();
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(800.0, 600.0),
            )),
            events,
            ..Default::default()
        };
        ctx.run_ui(input, |ui| tab.dialogs(ui.ctx(), &mut acts))
            .drop_without_applying_deltas();
        acts
    };
    let enter = || {
        vec![egui::Event::Key {
            key: egui::Key::Enter,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }]
    };
    let ran = |acts: Vec<Action>| {
        acts.into_iter().find_map(|a| match a {
            Action::RunFold(plan) => Some(plan.keep_fixup),
            _ => None,
        })
    };

    // Pushed commits: Enter doesn't rewrite them, a click has to.
    assert!(found.rewrites_pushed());
    tab.dialog = Some(Dialog::Fold(Box::new(found.clone())));
    assert!(frame(&mut tab, Vec::new()).is_empty());
    assert_eq!(ran(frame(&mut tab, enter())), None);
    assert!(tab.dialog.is_some());

    let unpushed = Plan {
        pushed: Vec::new(),
        ..found.clone()
    };
    tab.dialog = Some(Dialog::Fold(Box::new(unpushed)));
    let _ = frame(&mut tab, Vec::new());
    assert_eq!(ran(frame(&mut tab, enter())), Some(false));
    assert!(tab.dialog.is_none());

    let kept = Plan {
        keep_fixup: true,
        ..found
    };
    tab.dialog = Some(Dialog::Fold(Box::new(kept)));
    let _ = frame(&mut tab, Vec::new());
    assert_eq!(ran(frame(&mut tab, enter())), Some(true));
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
    let said = tab.status.clone().map(|(s, _)| s).unwrap_or_default();
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
    settle(&mut tab, &ctx);
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
    let said = tab.status.clone().map(|(s, _)| s).unwrap_or_default();
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
    settle(&mut tab, &ctx);
    let said = tab.status.clone().map(|(s, _)| s).unwrap_or_default();
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
    settle(&mut tab, &ctx);
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
//! Tests for which files fold, why, what their folded line says, and what a tab remembers of
//! their attributes; on real repositories, through what the lists and diffs use.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use super::list::group_title;
use super::rules::{glob_for_tests as glob, thousands};
use super::*;
use crate::git::{self, Area, FoldAttr, StatusFile};

fn rules() -> Rules {
    Rules::new(true, "")
}

#[test]
fn lockfiles_and_minified_files_fold_by_name_anywhere() {
    let r = rules();
    for path in [
        "Cargo.lock",
        "web/package-lock.json",
        "yarn.lock",
        "a/b/pnpm-lock.yaml",
        "poetry.lock",
        "go.sum",
        "Gemfile.lock",
        "composer.lock",
        "with space/Cargo.lock",
    ] {
        assert_eq!(r.reason(path, None), Some(FoldReason::Lockfile), "{path}");
    }
    for path in ["dist/app.min.js", "ünï/stÿle.min.css", "x.min.mjs"] {
        assert_eq!(r.reason(path, None), Some(FoldReason::Minified), "{path}");
    }
    for path in [
        "Cargo.toml",
        "cargo.lock.bak",
        "src/lock.rs",
        "package.json",
        ".min.js",
        "admin.js",
        "Cargo.lock/",
        "",
    ] {
        assert_eq!(r.reason(path, None), None, "{path}");
    }
}

#[test]
fn attributes_decide_before_names() {
    let r = rules();
    assert_eq!(
        r.reason("src/gen.rs", Some(FoldAttr::Generated)),
        Some(FoldReason::Generated)
    );
    assert_eq!(
        r.reason("data.bin", Some(FoldAttr::NoDiff)),
        Some(FoldReason::NoDiff)
    );
    // `linguist-generated=false` keeps even a lockfile open.
    assert_eq!(r.reason("Cargo.lock", Some(FoldAttr::NotGenerated)), None);
    assert_eq!(
        r.reason("Cargo.lock", Some(FoldAttr::Unspecified)),
        Some(FoldReason::Lockfile)
    );
}

#[test]
fn folding_turned_off_folds_nothing() {
    let off = Rules::new(false, "*.snap");
    assert_eq!(off.reason("Cargo.lock", Some(FoldAttr::Generated)), None);
    assert_eq!(off.reason("a.snap", None), None);
}

#[test]
fn patterns_of_the_settings_fold_too() {
    let r = Rules::new(true, "*.snap, dist/**\n/top.txt  docs/*.html vendor/");
    assert_eq!(r.reason("vendor/a/b.js", None), Some(FoldReason::Pattern));
    assert_eq!(r.reason("vendor", None), None);
    assert_eq!(
        r.reason("tests/__snapshots__/a b.snap", None),
        Some(FoldReason::Pattern)
    );
    assert_eq!(r.reason("dist/x/y.js", None), Some(FoldReason::Pattern));
    assert_eq!(r.reason("top.txt", None), Some(FoldReason::Pattern));
    assert_eq!(r.reason("sub/top.txt", None), None);
    assert_eq!(r.reason("docs/index.html", None), Some(FoldReason::Pattern));
    assert_eq!(r.reason("docs/api/index.html", None), None);
    assert_eq!(r.reason("src/dist/x.js", None), None);
    // Empty patterns (a stray comma) match nothing.
    assert_eq!(Rules::new(true, " , ,").reason("anything", None), None);
}

#[test]
fn globs_keep_stars_within_folders() {
    assert!(glob("*.js", "a.js"));
    assert!(!glob("*.js", "a/b.js"));
    assert!(glob("**/b.js", "b.js"));
    assert!(glob("**/b.js", "a/c/b.js"));
    assert!(!glob("**/b.js", "ab.js"));
    assert!(glob("a/**", "a/b/c"));
    assert!(glob("a?c", "abc"));
    assert!(!glob("a?c", "a/c"));
    assert!(glob("ü*", "über"));
    assert!(glob("", ""));
    assert!(!glob("", "x"));
    // Many stars over a long name end quickly.
    assert!(!glob("*a*a*a*a*a*b", &"a".repeat(200)));
}

#[test]
fn the_folded_line_says_what_changed_and_why() {
    assert_eq!(
        stats_line(Some(1204), Some(880), FoldReason::Generated),
        "+1,204 −880, generated"
    );
    assert_eq!(
        stats_line(Some(0), Some(0), FoldReason::Lockfile),
        "+0 −0, lockfile"
    );
    assert_eq!(stats_line(None, None, FoldReason::NoDiff), "marked -diff");
    assert_eq!(thousands(0), "0");
    assert_eq!(thousands(999), "999");
    assert_eq!(thousands(1000), "1,000");
    assert_eq!(thousands(1_234_567), "1,234,567");
    assert_eq!(thousands(u32::MAX), "4,294,967,295");
}

#[test]
fn a_group_is_named_after_what_it_holds() {
    use FoldReason::*;
    assert_eq!(group_title(&[Lockfile]), "1 lockfile");
    assert_eq!(group_title(&[Lockfile, Lockfile]), "2 lockfiles");
    assert_eq!(
        group_title(&[Generated, Generated, Generated]),
        "3 generated files"
    );
    assert_eq!(group_title(&[Minified]), "1 minified file");
    assert_eq!(group_title(&[NoDiff, NoDiff]), "2 files marked -diff");
    assert_eq!(group_title(&[Lockfile, Generated]), "2 folded files");
}

#[test]
fn attributes_are_asked_once_and_remembered() {
    let mut folding = Folding::default();
    let paths = ["src/gen.rs", "Cargo.lock", "src/gen.rs"];
    let unknown = folding.unknown(paths);
    assert_eq!(
        unknown,
        vec!["src/gen.rs".to_owned(), "Cargo.lock".to_owned()]
    );
    let (epoch, _) = folding.asking(&unknown);
    // Asked: not again while the answer is on its way.
    assert!(folding.unknown(paths).is_empty());
    let generation = folding.generation;
    let answer = HashMap::from([("src/gen.rs".to_owned(), FoldAttr::Generated)]);
    folding.arrived(epoch, unknown, answer, None);
    assert!(folding.generation > generation);
    assert!(folding.unknown(paths).is_empty());
    assert!(folding.knows("Cargo.lock"));
    let r = rules();
    assert_eq!(
        folding.reason(&r, "src/gen.rs"),
        Some(FoldReason::Generated)
    );
    assert_eq!(folding.reason(&r, "Cargo.lock"), Some(FoldReason::Lockfile));
}

#[test]
fn a_failed_answer_keeps_what_was_read_and_leaves_names_to_decide() {
    let mut folding = Folding::default();
    let unknown = folding.unknown(["gen.rs", "Cargo.lock", "a.rs"]);
    let (epoch, _) = folding.asking(&unknown);
    // The first run of paths was read before git failed on the next.
    let found = HashMap::from([("gen.rs".to_owned(), FoldAttr::Generated)]);
    folding.arrived(epoch, unknown, found, Some("git is gone".to_owned()));
    assert!(folding.unknown(["gen.rs", "Cargo.lock", "a.rs"]).is_empty());
    assert_eq!(
        folding.reason(&rules(), "gen.rs"),
        Some(FoldReason::Generated)
    );
    assert_eq!(
        folding.reason(&rules(), "Cargo.lock"),
        Some(FoldReason::Lockfile)
    );
    assert_eq!(folding.reason(&rules(), "a.rs"), None);
}

fn work_with(paths: &[(&str, char)]) -> git::WorkStatus {
    git::WorkStatus {
        unstaged: paths
            .iter()
            .map(|&(path, code)| StatusFile {
                path: path.to_owned(),
                old_path: None,
                code,
                area: Area::Unstaged,
            })
            .collect(),
        ..Default::default()
    }
}

/// Reads `paths` as git answering `attr` for each.
fn read(folding: &mut Folding, paths: &[&str], attr: FoldAttr) {
    let unknown = folding.unknown(paths.iter().copied());
    let (epoch, _) = folding.asking(&unknown);
    let found = unknown.iter().map(|p| (p.clone(), attr)).collect();
    folding.arrived(epoch, unknown, found, None);
}

#[test]
fn a_changed_gitattributes_or_checkout_forgets_what_was_read() {
    let start = Instant::now();
    let mut folding = Folding::default();
    folding.saw_status(&work_with(&[("a.rs", 'M')]), "head1", start);
    read(&mut folding, &["gen.rs"], FoldAttr::Generated);
    // The same status: still known.
    folding.saw_status(&work_with(&[("a.rs", 'M')]), "head1", start);
    assert!(folding.unknown(["gen.rs"]).is_empty());
    // Another commit checked out: asked again, the last answer applying meanwhile.
    folding.saw_status(&work_with(&[("a.rs", 'M')]), "head2", start);
    assert_eq!(folding.unknown(["gen.rs"]), vec!["gen.rs".to_owned()]);
    assert_eq!(
        folding.reason(&rules(), "gen.rs"),
        Some(FoldReason::Generated)
    );
    // A `.gitattributes` starts being edited: asked again.
    read(&mut folding, &["gen.rs"], FoldAttr::Generated);
    folding.saw_status(&work_with(&[("sub/.gitattributes", 'M')]), "head2", start);
    assert_eq!(folding.unknown(["gen.rs"]), vec!["gen.rs".to_owned()]);
}

/// While a `.gitattributes` stays changed, its files are read again now and then, not at
/// every status read: a busy working tree (or a slow SSH machine) would ask without end.
#[test]
fn a_changed_gitattributes_is_read_again_now_and_then() {
    let start = Instant::now();
    let edited = work_with(&[(".gitattributes", 'M')]);
    let mut folding = Folding::default();
    folding.saw_status(&edited, "h", start);
    read(&mut folding, &["gen.rs"], FoldAttr::Generated);
    for secs in [1, 5, 29] {
        folding.saw_status(&edited, "h", start + Duration::from_secs(secs));
        assert!(folding.unknown(["gen.rs"]).is_empty(), "after {secs} s");
    }
    folding.saw_status(&edited, "h", start + Duration::from_secs(31));
    assert_eq!(folding.unknown(["gen.rs"]), vec!["gen.rs".to_owned()]);
}

#[test]
fn an_answer_asked_before_forgetting_is_taken_but_asked_again() {
    let start = Instant::now();
    let mut folding = Folding::default();
    let unknown = folding.unknown(["gen.rs"]);
    let (epoch, now) = folding.asking(&unknown);
    folding.saw_status(&work_with(&[(".gitattributes", 'M')]), "h", start);
    // The read under way is told it's no longer wanted.
    assert_ne!(now.load(std::sync::atomic::Ordering::Relaxed), epoch);
    let stale = HashMap::from([("gen.rs".to_owned(), FoldAttr::Generated)]);
    folding.arrived(epoch, unknown, stale, None);
    assert_eq!(
        folding.reason(&rules(), "gen.rs"),
        Some(FoldReason::Generated)
    );
    assert_eq!(folding.unknown(["gen.rs"]), vec!["gen.rs".to_owned()]);
}

/// A repository with a `.gitattributes`, in a temporary folder removed when dropped.
struct Repo(std::path::PathBuf);

impl Repo {
    fn new(name: &str) -> Result<Self, String> {
        let dir = std::env::temp_dir().join(format!("gitcraft-fold-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let repo = Repo(dir);
        for args in [
            &["init", "-q"][..],
            &["config", "user.name", "t"],
            &["config", "user.email", "t@t"],
            &["config", "commit.gpgSign", "false"],
        ] {
            git::run(Some(&repo.0), args)?;
        }
        Ok(repo)
    }

    fn write(&self, path: &str, body: &str) -> Result<(), String> {
        let file = self.0.join(path);
        if let Some(dir) = file.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        std::fs::write(file, body).map_err(|e| e.to_string())
    }
}

impl Drop for Repo {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn gitattributes_are_read_from_a_real_repository() -> Result<(), String> {
    let repo = Repo::new("attrs")?;
    repo.write(
        ".gitattributes",
        "gen/** linguist-generated\n*.dat -diff\nCargo.lock linguist-generated=false\n\
         \"with space.rs\" linguist-generated=true\n",
    )?;
    repo.write("sub/.gitattributes", "ünï.txt linguist-generated\n")?;
    let paths: Vec<String> = [
        "gen/a/b.rs",
        "blob.dat",
        "Cargo.lock",
        "with space.rs",
        "sub/ünï.txt",
        "plain.rs",
        "-starts-with-dash.rs",
    ]
    .iter()
    .map(|p| (*p).to_owned())
    .collect();
    let (found, error) = git::fold_attributes(&repo.0, &paths, &|| true);
    assert_eq!(error, None);
    assert_eq!(found["gen/a/b.rs"], FoldAttr::Generated);
    assert_eq!(found["blob.dat"], FoldAttr::NoDiff);
    assert_eq!(found["Cargo.lock"], FoldAttr::NotGenerated);
    assert_eq!(found["with space.rs"], FoldAttr::Generated);
    assert_eq!(found["sub/ünï.txt"], FoldAttr::Generated);
    assert_eq!(found["plain.rs"], FoldAttr::Unspecified);
    assert_eq!(found["-starts-with-dash.rs"], FoldAttr::Unspecified);
    // Nothing asked, nothing run.
    assert!(git::fold_attributes(&repo.0, &[], &|| true).0.is_empty());
    // Not wanted any more: nothing is read.
    let (none, error) = git::fold_attributes(&repo.0, &paths, &|| false);
    assert!(none.is_empty() && error.is_none());
    Ok(())
}

#[test]
fn thousands_of_paths_are_asked_in_a_few_commands() -> Result<(), String> {
    let repo = Repo::new("many")?;
    repo.write(".gitattributes", "*.gen linguist-generated\n")?;
    let paths: Vec<String> = (0..5000)
        .map(|i| format!("some/rather/deep/folder/file number {i}.gen"))
        .collect();
    let (found, error) = git::fold_attributes(&repo.0, &paths, &|| true);
    assert_eq!(error, None);
    assert_eq!(found.len(), paths.len());
    assert!(found.values().all(|a| *a == FoldAttr::Generated));
    Ok(())
}

#[test]
fn a_folder_that_is_no_repository_says_so() {
    let dir = std::env::temp_dir().join(format!("gitcraft-fold-none-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&dir);
    let (found, error) = git::fold_attributes(&dir.join("missing"), &["a".to_owned()], &|| true);
    let _ = std::fs::remove_dir_all(&dir);
    assert!(found.is_empty());
    let e = error.unwrap_or_default();
    assert!(e.starts_with("Couldn't read .gitattributes"), "{e}");
}

/// A lockfile's diff, read as the diff pane reads it: its folded line counts its lines.
#[test]
fn a_folded_diff_counts_the_lines_of_its_change() -> Result<(), String> {
    let repo = Repo::new("counts")?;
    let lock: String = (0..1500).map(|i| format!("pkg-{i} = \"1.0\"\n")).collect();
    repo.write("Cargo.lock", &lock)?;
    git::run(Some(&repo.0), &["add", "-A"])?;
    git::run(Some(&repo.0), &["commit", "-qm", "one"])?;
    let changed = lock.replace("= \"1.0\"\npkg-1", "= \"2.0\"\npkg-1");
    repo.write("Cargo.lock", &changed)?;
    let key = crate::repo::DiffKey::Work(StatusFile {
        path: "Cargo.lock".to_owned(),
        old_path: None,
        code: 'M',
        area: Area::Unstaged,
    });
    let data = crate::repo::diff::load(&repo.0, &key, git::DiffOptions::default(), true)?;
    let (added, removed) = data.line_counts;
    let rules = rules();
    let reason = Folding::default()
        .reason(&rules, "Cargo.lock")
        .ok_or("not folded")?;
    let changed_lines = lock
        .lines()
        .zip(changed.lines())
        .filter(|(a, b)| a != b)
        .count();
    let n = u32::try_from(changed_lines).map_err(|e| e.to_string())?;
    assert_eq!((added, removed), (Some(n), Some(n)));
    assert_eq!(
        stats_line(added, removed, reason),
        format!("+{n} −{n}, lockfile")
    );
    Ok(())
}

/// A list replaced by another of the same length and ends, in the same memory (as a reloaded
/// status can be), is told apart: the folds of the old one don't stick to the new one's files.
#[test]
fn a_list_changed_in_place_is_worked_out_again() {
    use super::list::list_fingerprint;
    let mut files: Vec<String> = ["README.md", "Cargo.lock", "z.rs"]
        .map(String::from)
        .to_vec();
    let before = list_fingerprint(&files, String::as_str);
    files[1] = "src/b.rs".to_owned();
    assert_ne!(list_fingerprint(&files, String::as_str), before);
    // A long list too, changed far from its ends.
    let mut many: Vec<String> = (0..20_000).map(|i| format!("f{i}")).collect();
    let before = list_fingerprint(&many, String::as_str);
    many[10_000] = "changed".to_owned();
    assert_ne!(list_fingerprint(&many, String::as_str), before);
    assert_eq!(
        list_fingerprint(&many, String::as_str),
        list_fingerprint(&many, String::as_str)
    );
}

/// A read that failed is tried again once the interval is over, keeping its last answer
/// meanwhile, and not asked again before.
#[test]
fn a_failed_read_is_tried_again_later() {
    let start = Instant::now();
    let mut folding = Folding::default();
    read(&mut folding, &["Cargo.lock"], FoldAttr::NotGenerated);
    // Forgotten (another commit), and the read again fails.
    folding.saw_status(&work_with(&[]), "other", start);
    let unknown = folding.unknown(["Cargo.lock"]);
    let (epoch, _) = folding.asking(&unknown);
    folding.arrived(epoch, unknown, HashMap::new(), Some("busy".to_owned()));
    assert_eq!(
        folding.reason(&rules(), "Cargo.lock"),
        None,
        "last answer kept"
    );
    assert!(
        folding.unknown(["Cargo.lock"]).is_empty(),
        "not asked again at once"
    );
    let wait = folding.tick(Instant::now()).unwrap_or_default();
    assert!(wait > Duration::from_secs(20));
    assert!(folding.tick(Instant::now() + REREAD_EVERY).is_some());
    assert_eq!(
        folding.unknown(["Cargo.lock"]),
        vec!["Cargo.lock".to_owned()]
    );
    assert_eq!(folding.tick(Instant::now() + REREAD_EVERY * 2), None);
}

/// An edit of a `.gitattributes` seen too soon after the last read is read once the interval
/// is over, though nothing else happens by then.
#[test]
fn an_edit_seen_too_soon_is_read_when_the_interval_is_over() {
    let start = Instant::now();
    let edited = work_with(&[(".gitattributes", 'M')]);
    let mut folding = Folding::default();
    folding.saw_status(&edited, "h", start);
    read(&mut folding, &["gen.rs"], FoldAttr::Generated);
    folding.saw_status(&edited, "h", start + Duration::from_secs(5));
    assert!(folding.unknown(["gen.rs"]).is_empty());
    assert!(
        folding
            .tick(start + Duration::from_secs(10))
            .is_some_and(|w| w > Duration::ZERO)
    );
    assert_eq!(
        folding.tick(start + Duration::from_secs(31)),
        Some(Duration::ZERO)
    );
    assert_eq!(folding.unknown(["gen.rs"]), vec!["gen.rs".to_owned()]);
}

/// A `.gitattributes` renamed away forgets what it said.
#[test]
fn a_gitattributes_renamed_away_forgets_what_was_read() {
    let start = Instant::now();
    let mut folding = Folding::default();
    folding.saw_status(&work_with(&[]), "h", start);
    read(&mut folding, &["gen.rs"], FoldAttr::Generated);
    let mut renamed = git::WorkStatus::default();
    renamed.staged.push(StatusFile {
        path: "attributes.backup".to_owned(),
        old_path: Some(".gitattributes".to_owned()),
        code: 'R',
        area: Area::Staged,
    });
    folding.saw_status(&renamed, "h", start);
    assert_eq!(folding.unknown(["gen.rs"]), vec!["gen.rs".to_owned()]);
}
