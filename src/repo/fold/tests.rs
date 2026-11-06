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

/// Takes in what the tab's background work sends until it has nothing more to read.
fn settle(tab: &mut RepoTab, ctx: &egui::Context) {
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(30) {
        match tab.rx.recv_timeout(Duration::from_millis(300)) {
            Ok(msg) => tab.handle(msg, ctx),
            Err(_) if !tab.loading => return,
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
    let steps = match tab.abort_for(InProgress::Rebase) {
        Action::Dialog(Dialog::Confirm { ops, title, .. }) => Some((title, ops)),
        _ => None,
    };
    assert!(steps.as_ref().is_some_and(|(_, ops)| ops.len() == 3));
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
    assert!(refused("HEAD").contains("Merge commits"));
    git_ok(&dir, &["checkout", "-q", "-b", "other", "HEAD~2"]);
    assert!(refused(&side).contains("not an ancestor"));
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

/// The dialog, drawn: Enter folds as shown, and ticking _Keep as fixup!_ makes that what runs.
#[test]
fn the_dialog_runs_the_fold_it_shows() {
    let dir = repo("dialog", 3);
    git_ok(&dir, &["update-ref", "refs/remotes/origin/main", "HEAD"]);
    write(&dir, "f1.txt", "1 fixed\n");
    git_ok(&dir, &["add", "f1.txt"]);
    let found = plan(&dir, "HEAD~2");
    let _ = std::fs::remove_dir_all(&dir);
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

    tab.dialog = Some(Dialog::Fold(Box::new(found.clone())));
    assert!(frame(&mut tab, Vec::new()).is_empty());
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
