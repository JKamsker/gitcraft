//! The first steps of real repositories, read the way the tab reads them, the empty first
//! commit's undo and redo on them, and the guide's actions on a tab.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::empty_commit::{EMPTY_COMMIT_LABEL, empty_commit_steps};
use super::first_steps::{Facts, FirstSteps, MISSING, NEEDS_COMMIT, StepState, first_steps};
use super::hints::HintState;
use super::{GuideAct, Hint};
use crate::git;
use crate::repo::operations::run_steps;
use crate::repo::{Action, OpKind, RepoTab, Undo, UndoEntry, WIP_HASH};
use crate::settings::Settings;

/// A fresh, empty repository (`git init`, no commit) in a folder with spaces and non-ASCII
/// letters in its name.
fn empty_repo(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("gitcraft guide ü {name} {}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::create_dir_all(&dir);
    for args in [
        &["init", "-q", "-b", "main"][..],
        &["config", "user.name", "Zoë Tester"],
        &["config", "user.email", "t@t"],
    ] {
        assert!(git::run(Some(&dir), args).is_ok());
    }
    dir
}

/// A bare repository beside `dir`, to publish to.
fn bare_beside(dir: &Path) -> PathBuf {
    let bare = dir.with_file_name(format!(
        "{} bare",
        dir.file_name().unwrap_or_default().to_string_lossy()
    ));
    let _ = std::fs::remove_dir_all(&bare);
    assert!(git::run(None, &["init", "-q", "--bare", &bare.to_string_lossy()]).is_ok());
    bare
}

/// The facts of `dir`, read with the reads the tab makes.
fn read(dir: &Path) -> Facts {
    let refs = git::refs(dir).unwrap_or_default();
    let work = git::status(dir).unwrap_or_default();
    let remotes = git::remotes(dir).unwrap_or_default();
    let commits = git::run(Some(dir), &["rev-list", "--count", "--all"])
        .unwrap_or_default()
        .trim()
        .parse()
        .unwrap_or_default();
    Facts::read(commits, &refs, &work, &remotes, &git::identity(dir))
}

fn steps(dir: &Path) -> Option<FirstSteps> {
    first_steps(&read(dir))
}

/// Makes the empty first commit in `dir` as the guide does (on the branch HEAD names), and
/// returns its undo entry.
fn make_empty_commit(dir: &Path) -> (Result<String, String>, Option<UndoEntry>) {
    let branch = git::run(Some(dir), &["symbolic-ref", "-q", "HEAD"])
        .unwrap_or_default()
        .trim()
        .to_owned();
    let (commit, undo) = empty_commit_steps(&branch);
    let (made, kind) = run_steps(dir, &commit, OpKind::Normal(undo, String::new()), None);
    let entry = match kind {
        OpKind::Normal(Undo::Custom { undo, redo }, _) => Some(UndoEntry {
            label: EMPTY_COMMIT_LABEL.to_owned(),
            undo,
            redo,
            heads: None,
        }),
        _ => None,
    };
    (made, entry)
}

fn undo(dir: &Path, entry: &UndoEntry) -> Result<String, String> {
    run_steps(dir, &entry.undo, OpKind::Undo(entry.clone()), None).0
}

fn redo(dir: &Path, entry: &UndoEntry) -> Result<String, String> {
    run_steps(dir, &entry.redo, OpKind::Redo(entry.clone()), None).0
}

fn branch_exists(dir: &Path, full: &str) -> bool {
    git::resolve(dir, full).is_ok()
}

#[test]
fn an_empty_repository_shows_its_first_steps_with_nothing_to_commit() {
    let dir = empty_repo("empty");
    let steps = steps(&dir);
    let _ = std::fs::remove_dir_all(&dir);
    let steps = steps.unwrap_or(MISSING);
    assert!(steps.empty);
    assert!(steps.nothing_to_commit);
    assert_eq!(steps.first_commit, StepState::Todo);
    assert_eq!(steps.add_remote, StepState::Todo);
    assert_eq!(steps.publish, StepState::Blocked(NEEDS_COMMIT));
}

#[test]
fn an_untracked_file_is_something_to_commit() {
    let dir = empty_repo("untracked");
    assert!(std::fs::write(dir.join("hello wörld.txt"), "hi\n").is_ok());
    let facts = read(&dir);
    let _ = std::fs::remove_dir_all(&dir);
    assert!(facts.has_changes);
    let steps = first_steps(&facts).unwrap_or(MISSING);
    assert!(steps.empty);
    assert!(!steps.nothing_to_commit);
}

#[test]
fn a_remote_marks_add_a_remote_done() {
    let dir = empty_repo("remote");
    let remote = dir.join("elsewhere ü.git");
    let added = git::run(
        Some(&dir),
        &["remote", "add", "origin", &remote.to_string_lossy()],
    );
    let steps = steps(&dir);
    let _ = std::fs::remove_dir_all(&dir);
    assert!(added.is_ok());
    let steps = steps.unwrap_or(MISSING);
    assert_eq!(steps.add_remote, StepState::Done);
    assert_eq!(steps.publish, StepState::Blocked(NEEDS_COMMIT));
}

#[test]
fn after_the_first_commit_publish_is_offered_then_publishing_ends_the_guide() {
    let dir = empty_repo("publish");
    let bare = bare_beside(&dir);
    assert!(std::fs::write(dir.join("a.txt"), "a\n").is_ok());
    for args in [&["add", "-A"][..], &["commit", "-q", "-m", "first"]] {
        assert!(git::run(Some(&dir), args).is_ok());
    }
    let local_only = steps(&dir);
    let added = git::run(
        Some(&dir),
        &["remote", "add", "origin", &bare.to_string_lossy()],
    );
    let with_remote = steps(&dir);
    // As the toolbar's Push publishes a branch without an upstream.
    let pushed = git::run(
        Some(&dir),
        &[
            "-c",
            "push.default=simple",
            "push",
            "-u",
            "origin",
            "refs/heads/main",
        ],
    );
    let published = steps(&dir);
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_dir_all(&bare);
    let local_only = local_only.unwrap_or(MISSING);
    assert!(!local_only.empty);
    assert_eq!(local_only.first_commit, StepState::Done);
    assert_eq!(local_only.publish, StepState::Blocked("Add a remote first"));
    assert!(added.is_ok());
    assert_eq!(with_remote.unwrap_or(MISSING).publish, StepState::Todo);
    assert!(pushed.is_ok(), "{pushed:?}");
    assert_eq!(published, None);
}

#[test]
fn a_repository_with_history_shows_no_first_steps() {
    let dir = empty_repo("history");
    for n in 0..3 {
        let msg = format!("c{n}");
        assert!(git::run(Some(&dir), &["commit", "-q", "--allow-empty", "-m", &msg]).is_ok());
    }
    let steps = steps(&dir);
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(steps, None);
}

#[test]
fn an_unborn_orphan_branch_in_a_repository_with_history_shows_no_first_steps() {
    let ctx = egui::Context::default();
    let dir = empty_repo("orphan");
    for n in 0..3 {
        let msg = format!("c{n}");
        assert!(git::run(Some(&dir), &["commit", "-q", "--allow-empty", "-m", &msg]).is_ok());
    }
    assert!(git::run(Some(&dir), &["checkout", "-q", "--orphan", "fresh"]).is_ok());
    let mut tab = read_tab(&ctx, &dir, false);
    // The history the tab logs (every branch) still has the commits.
    tab.history = (0..3)
        .map(|n| {
            Arc::new(git::Commit {
                hash: format!("{n:040}"),
                ..Default::default()
            })
        })
        .collect();
    let shown = tab.first_steps();
    let _ = std::fs::remove_dir_all(&dir);
    assert!(tab.refs.head.is_empty());
    assert_eq!(shown, None);
}

#[test]
fn the_empty_first_commit_is_undone_back_to_no_commits_and_redone() {
    let dir = empty_repo("empty-commit");
    let (made, entry) = make_empty_commit(&dir);
    let head = git::head(&dir);
    let entry = entry.unwrap_or_else(|| UndoEntry {
        label: String::new(),
        undo: Vec::new(),
        redo: Vec::new(),
        heads: None,
    });
    let undone = undo(&dir, &entry);
    let after_undo = (
        git::head(&dir),
        git::refs(&dir).unwrap_or_default().current,
        steps(&dir),
    );
    let redone = redo(&dir, &entry);
    let after_redo = git::head(&dir);
    let message = git::head_message(&dir);
    let _ = std::fs::remove_dir_all(&dir);
    assert!(made.is_ok(), "{made:?}");
    assert!(!head.is_empty());
    assert!(undone.is_ok(), "{undone:?}");
    assert_eq!(after_undo.0, "");
    assert_eq!(after_undo.1, None);
    assert!(after_undo.2.is_some_and(|s| s.empty && s.nothing_to_commit));
    assert!(redone.is_ok(), "{redone:?}");
    assert!(!after_redo.is_empty());
    assert_eq!(message.unwrap_or_default().trim(), "Initial commit");
}

#[test]
fn undoing_the_empty_commit_refuses_once_more_was_committed() {
    let dir = empty_repo("empty-commit-moved");
    let (_, entry) = make_empty_commit(&dir);
    assert!(git::run(Some(&dir), &["commit", "-q", "--allow-empty", "-m", "more"]).is_ok());
    let refused = entry.as_ref().map(|e| undo(&dir, e));
    let head = git::head(&dir);
    let _ = std::fs::remove_dir_all(&dir);
    assert!(refused.is_some_and(|r| r.is_err()));
    assert!(!head.is_empty());
}

/// Empty commit, undo, then a real first commit: redoing the empty one must not go on top
/// of it (and then undoing that must not delete the branch with the real commit).
#[test]
fn redoing_the_empty_commit_after_a_real_first_commit_is_refused() {
    let dir = empty_repo("redo-after-real");
    let (_, entry) = make_empty_commit(&dir);
    let undone = entry.as_ref().map(|e| undo(&dir, e));
    assert!(std::fs::write(dir.join("real work.txt"), "x\n").is_ok());
    for args in [&["add", "-A"][..], &["commit", "-q", "-m", "real"]] {
        assert!(git::run(Some(&dir), args).is_ok());
    }
    let real = git::head(&dir);
    let redone = entry.as_ref().map(|e| redo(&dir, e));
    let undone_again = entry.as_ref().map(|e| undo(&dir, e));
    let after = git::head(&dir);
    let count = git::run(Some(&dir), &["rev-list", "--count", "HEAD"]).unwrap_or_default();
    let _ = std::fs::remove_dir_all(&dir);
    assert!(undone.is_some_and(|r| r.is_ok()));
    assert!(redone.as_ref().is_some_and(|r| r.is_err()), "{redone:?}");
    assert!(redone.is_some_and(|r| r.is_err_and(|e| e.contains("has commits now"))));
    assert!(undone_again.is_some_and(|r| r.is_err()));
    assert_eq!(after, real);
    assert_eq!(count.trim(), "1");
}

/// The commit itself is refused when HEAD got a commit while the dialog was open.
#[test]
fn the_empty_commit_is_refused_once_the_branch_has_a_commit() {
    let dir = empty_repo("commit-meanwhile");
    let (commit, how) = empty_commit_steps("refs/heads/main");
    assert!(
        git::run(
            Some(&dir),
            &["commit", "-q", "--allow-empty", "-m", "meanwhile"]
        )
        .is_ok()
    );
    let (made, _) = run_steps(&dir, &commit, OpKind::Normal(how, String::new()), None);
    let count = git::run(Some(&dir), &["rev-list", "--count", "HEAD"]).unwrap_or_default();
    let _ = std::fs::remove_dir_all(&dir);
    assert!(made.is_err());
    assert_eq!(count.trim(), "1");
}

/// Undoing with HEAD detached would have removed `.git/HEAD` (no repository any more).
#[test]
fn undoing_the_empty_commit_with_a_detached_head_is_refused() {
    let dir = empty_repo("undo-detached");
    let (_, entry) = make_empty_commit(&dir);
    assert!(git::run(Some(&dir), &["checkout", "-q", "--detach"]).is_ok());
    let refused = entry.as_ref().map(|e| undo(&dir, e));
    let still_a_repo = git::run(Some(&dir), &["rev-parse", "--git-dir"]).is_ok();
    let head = git::head(&dir);
    let main = branch_exists(&dir, "refs/heads/main");
    let _ = std::fs::remove_dir_all(&dir);
    assert!(refused.is_some_and(|r| r.is_err()));
    assert!(still_a_repo);
    assert!(!head.is_empty());
    assert!(main);
}

/// With another branch made at the same commit and checked out, undoing would have deleted
/// that branch instead.
#[test]
fn undoing_the_empty_commit_after_switching_to_a_new_branch_is_refused() {
    let dir = empty_repo("undo-switched");
    let (_, entry) = make_empty_commit(&dir);
    assert!(git::run(Some(&dir), &["switch", "-q", "-c", "feature-ü"]).is_ok());
    let refused = entry.as_ref().map(|e| undo(&dir, e));
    let both = (
        branch_exists(&dir, "refs/heads/main"),
        branch_exists(&dir, "refs/heads/feature-ü"),
    );
    let _ = std::fs::remove_dir_all(&dir);
    assert!(refused.is_some_and(|r| r.is_err()));
    assert_eq!(both, (true, true));
}

/// Once published, Ctrl+Z on the empty first commit would delete the branch the remote has.
#[test]
fn undoing_the_empty_commit_after_publishing_it_is_refused() {
    let ctx = egui::Context::default();
    let dir = empty_repo("undo-published");
    let bare = bare_beside(&dir);
    let (_, entry) = make_empty_commit(&dir);
    for args in [
        &["remote", "add", "origin", &bare.to_string_lossy()][..],
        &["push", "-q", "-u", "origin", "refs/heads/main"],
    ] {
        assert!(git::run(Some(&dir), args).is_ok());
    }
    let mut tab = read_tab(&ctx, &dir, false);
    tab.undo_stack = entry.into_iter().collect();
    tab.apply(Action::Undo, &ctx);
    let main = branch_exists(&dir, "refs/heads/main");
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_dir_all(&bare);
    assert_eq!(tab.undo_stack.len(), 1);
    assert!(tab.busy_op.is_none());
    assert!(
        tab.status
            .as_ref()
            .is_some_and(|(m, err)| *err && m.contains("main is published"))
    );
    assert!(main);
}

/// A first commit records no undo (there is nothing before it), but what was undone before it
/// can't be redone on top of it.
#[test]
fn a_first_commit_drops_what_could_be_redone() {
    let ctx = egui::Context::default();
    let mut tab = RepoTab::new_deferred("/nonexistent/redo-test".into(), &ctx);
    tab.redo_stack.push(UndoEntry {
        label: EMPTY_COMMIT_LABEL.to_owned(),
        undo: Vec::new(),
        redo: Vec::new(),
        heads: None,
    });
    let after = "b".repeat(40);
    // HEAD did not move: the redo stays.
    tab.record_undo(
        "Commit",
        OpKind::Normal(Undo::HeadMove { mode: "--soft" }, "\u{0}\u{0}".to_owned()),
    );
    let kept = tab.redo_stack.len();
    tab.record_undo(
        "Commit",
        OpKind::Normal(
            Undo::HeadMove { mode: "--soft" },
            format!("\u{0}{after}\u{0}refs/heads/main"),
        ),
    );
    assert_eq!(kept, 1);
    assert!(tab.redo_stack.is_empty());
    assert!(tab.undo_stack.is_empty());
}

/// A tab of `dir` as if its first read were in: no history, the changes row when `dirty`.
fn read_tab(ctx: &egui::Context, dir: &Path, dirty: bool) -> RepoTab {
    let mut tab = RepoTab::new_deferred(dir.to_path_buf(), ctx);
    tab.history_gen = 1;
    tab.refs = git::refs(dir).unwrap_or_default();
    tab.work = git::status(dir).unwrap_or_default();
    tab.remotes = git::remotes(dir).unwrap_or_default();
    tab.identity = git::identity(dir);
    if dirty {
        tab.commits = vec![Arc::new(git::Commit {
            hash: WIP_HASH.to_owned(),
            ..Default::default()
        })];
        tab.graph = crate::graph::layout(&tab.commits);
        tab.index = [(WIP_HASH.to_owned(), 0)].into_iter().collect();
    }
    tab
}

#[test]
fn the_first_steps_and_the_tip_line_draw_alone_and_under_the_changes_row() {
    let ctx = egui::Context::default();
    crate::theme::install_fonts(&ctx);
    let dir = empty_repo("draw");
    let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(900.0, 600.0));
    let mut drawn = Vec::new();
    for dirty in [false, true] {
        let mut tab = read_tab(&ctx, &dir, dirty);
        let mut acts = Vec::new();
        for _ in 0..3 {
            let input = egui::RawInput {
                screen_rect: Some(screen),
                ..Default::default()
            };
            ctx.run_ui(input, |ui| {
                egui::CentralPanel::default()
                    .frame(egui::Frame::NONE)
                    .show(ui, |ui| tab.graph_view(ui, &mut acts));
            })
            .drop_without_applying_deltas();
        }
        drawn.push((tab.first_steps().is_some(), acts.len()));
    }
    let _ = std::fs::remove_dir_all(&dir);
    // Drawing alone asks for nothing.
    assert_eq!(drawn, vec![(true, 0), (true, 0)]);
}

#[test]
fn a_tab_of_an_empty_repository_shows_the_first_steps_once_read() {
    let ctx = egui::Context::default();
    let dir = empty_repo("tab");
    let mut tab = read_tab(&ctx, &dir, false);
    let shown = tab.first_steps();
    // Not before the first read is in.
    tab.history_gen = 0;
    let opening = tab.first_steps();
    let _ = std::fs::remove_dir_all(&dir);
    assert!(shown.is_some_and(|s| s.empty));
    assert_eq!(opening, None);
}

#[test]
fn make_the_first_commit_opens_the_changes_at_the_summary() {
    let ctx = egui::Context::default();
    let dir = empty_repo("start-commit");
    assert!(std::fs::write(dir.join("new file.txt"), "x\n").is_ok());
    let mut tab = read_tab(&ctx, &dir, true);
    Settings {
        show_detail: false,
        ..Settings::default()
    }
    .set(&ctx);
    tab.apply(Action::Guide(GuideAct::StartCommit), &ctx);
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(tab.selected, Some(0));
    assert!(tab.guide.focus_summary.is_some());
    assert!(Settings::get(&ctx).show_detail);
}

#[test]
fn the_summary_takes_the_focus_only_just_after_it_was_asked_for() {
    let mut guide = super::Guide {
        focus_summary: Some(10),
        ..Default::default()
    };
    assert!(guide.take_focus_summary(11));
    // Used up.
    assert!(!guide.take_focus_summary(11));
    // Drawn only much later (the pane was closed meanwhile): the cursor stays where it went.
    guide.focus_summary = Some(10);
    assert!(!guide.take_focus_summary(500));
    assert_eq!(guide.focus_summary, None);
}

#[test]
fn make_the_first_commit_with_nothing_to_commit_says_so() {
    let ctx = egui::Context::default();
    let dir = empty_repo("start-nothing");
    let mut tab = read_tab(&ctx, &dir, false);
    tab.apply(Action::Guide(GuideAct::StartCommit), &ctx);
    let _ = std::fs::remove_dir_all(&dir);
    // A notice, not an error.
    assert!(
        tab.status
            .as_ref()
            .is_some_and(|(m, err)| !*err && m.contains("Nothing to commit"))
    );
    assert!(tab.guide.focus_summary.is_none());
}

#[test]
fn asking_for_the_empty_commit_opens_its_confirmation_for_the_unborn_branch() {
    let ctx = egui::Context::default();
    let dir = empty_repo("ask-empty");
    let mut tab = read_tab(&ctx, &dir, false);
    tab.apply(Action::Guide(GuideAct::EmptyCommit), &ctx);
    let _ = std::fs::remove_dir_all(&dir);
    let dialog = match &tab.dialog {
        Some(crate::repo::Dialog::Confirm {
            ops, safe, undo, ..
        }) => Some((ops.clone(), *safe, matches!(undo, Undo::Custom { .. }))),
        _ => None,
    };
    let (expected, _) = empty_commit_steps("refs/heads/main");
    assert_eq!(dialog, Some((expected, true, true)));
}

#[test]
fn a_hint_asked_for_by_the_tab_shows_once_and_is_kept_as_seen_once_drawn() {
    let ctx = egui::Context::default();
    let mut tab = RepoTab::new_deferred("/nonexistent/hint-test".into(), &ctx);
    tab.apply(Action::Guide(GuideAct::Hint(Hint::DoubleClickPill)), &ctx);
    let now = std::time::Instant::now();
    assert!(matches!(tab.guide.hints.state(now), HintState::Settling(_)));
    let mut settings = Settings::get(&ctx);
    assert!(!settings.seen_hints.contains("double-click-pill"));
    assert!(tab.guide.hints.drawn(&mut settings));
    settings.set(&ctx);
    tab.guide.hints.close();
    tab.apply(Action::Guide(GuideAct::Hint(Hint::DoubleClickPill)), &ctx);
    assert_eq!(tab.guide.hints.state(now), HintState::None);
}

#[test]
fn the_compare_hint_waits_for_a_second_commit() {
    let ctx = egui::Context::default();
    let mut tab = RepoTab::new_deferred("/nonexistent/compare-hint".into(), &ctx);
    let commit = |n: usize| {
        Arc::new(git::Commit {
            hash: format!("{n:040}"),
            ..Default::default()
        })
    };
    tab.history = vec![commit(1)];
    tab.commits = tab.history.clone();
    tab.hint_after_select(0, &ctx);
    let one = tab.guide.hints.state(std::time::Instant::now());
    tab.history = vec![commit(2), commit(1)];
    tab.commits = tab.history.clone();
    tab.hint_after_select(0, &ctx);
    let two = tab.guide.hints.state(std::time::Instant::now());
    assert_eq!(one, HintState::None);
    assert!(matches!(two, HintState::Settling(_)));
}

#[test]
fn turning_tips_off_says_how_to_get_them_back() {
    let ctx = egui::Context::default();
    let mut tab = RepoTab::new_deferred("/nonexistent/tips-off".into(), &ctx);
    tab.turn_tips_off(&ctx);
    assert!(!Settings::get(&ctx).tips);
    assert!(
        tab.status
            .as_ref()
            .is_some_and(|(m, err)| !*err && m.contains("Settings → General → Tips"))
    );
}

#[test]
fn only_hiding_a_branch_hints_at_solo() {
    use crate::repo::branch_visibility::{Change, Visibility};
    let mut visibility = Visibility::default();
    let main = "refs/heads/main".to_owned();
    let side = "refs/heads/side".to_owned();
    let hint = |change: &Change, v: &Visibility| super::visibility_hint(change, v, Some("main"));
    assert_eq!(
        hint(&Change::Toggle(side.clone()), &visibility),
        Some(Hint::SoloBranch)
    );
    // The checked-out branch always shows: its eye hides nothing.
    assert_eq!(hint(&Change::Toggle(main.clone()), &visibility), None);
    visibility.toggle(&side);
    // Showing it again.
    assert_eq!(hint(&Change::Toggle(side.clone()), &visibility), None);
    assert_eq!(hint(&Change::ShowAll, &visibility), None);
    assert_eq!(hint(&Change::Solo(side.clone()), &visibility), None);
    // While soloing, a toggle takes a branch out of the soloed ones: soloing is known.
    let mut soloing = Visibility::default();
    soloing.toggle_solo(&side);
    assert_eq!(hint(&Change::Toggle(side), &soloing), None);
}
