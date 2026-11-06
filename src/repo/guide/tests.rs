//! The first steps of real repositories, read the way the tab reads them, and the guide's
//! actions on a tab.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::first_steps::{Facts, FirstSteps, MISSING, StepState, empty_commit_steps, first_steps};
use super::{GuideAct, Hint};
use crate::git;
use crate::repo::operations::run_steps;
use crate::repo::{OpKind, RepoTab, UndoEntry, WIP_HASH};
use crate::settings::Settings;

/// A fresh, empty repository (`git init`, no commit) in a folder with spaces and non-ASCII
/// letters in its name.
fn empty_repo(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("gitcraft guide ü {name} {}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::create_dir_all(&dir);
    for args in [
        &["init", "-q"][..],
        &["config", "user.name", "Zoë Tester"],
        &["config", "user.email", "t@t"],
    ] {
        assert!(git::run(Some(&dir), args).is_ok());
    }
    dir
}

/// The facts of `dir`, read with the reads the tab makes.
fn read(dir: &Path) -> Facts {
    let refs = git::refs(dir).unwrap_or_default();
    let work = git::status(dir).unwrap_or_default();
    let remotes = git::remotes(dir).unwrap_or_default();
    let commits = if git::head(dir).is_empty() {
        0
    } else {
        git::run(Some(dir), &["rev-list", "--count", "--all"])
            .unwrap_or_default()
            .trim()
            .parse()
            .unwrap_or_default()
    };
    Facts::read(commits, &refs, &work, &remotes)
}

fn steps(dir: &Path) -> Option<FirstSteps> {
    first_steps(&read(dir))
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
    assert!(matches!(steps.publish, StepState::Blocked(_)));
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
    assert_eq!(
        steps.publish,
        StepState::Blocked("Make the first commit first")
    );
}

#[test]
fn after_the_first_commit_publish_is_offered_then_publishing_ends_the_guide() {
    let dir = empty_repo("publish");
    let bare = dir.with_file_name(format!(
        "{} bare",
        dir.file_name().unwrap_or_default().to_string_lossy()
    ));
    let _ = std::fs::remove_dir_all(&bare);
    assert!(git::run(None, &["init", "-q", "--bare", &bare.to_string_lossy()]).is_ok());
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
    let branch = git::refs(&dir)
        .unwrap_or_default()
        .current
        .unwrap_or_default();
    // As the toolbar's Push publishes a branch without an upstream.
    let pushed = git::run(
        Some(&dir),
        &[
            "-c",
            "push.default=simple",
            "push",
            "-u",
            "origin",
            &format!("refs/heads/{branch}"),
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
fn the_empty_first_commit_is_undone_back_to_no_commits_and_redone() {
    let dir = empty_repo("empty-commit");
    let (commit, undo) = empty_commit_steps();
    let (made, kind) = run_steps(&dir, &commit, OpKind::Normal(undo, String::new()), None);
    let head = git::head(&dir);
    assert!(matches!(
        kind,
        OpKind::Normal(crate::repo::Undo::Custom { .. }, _)
    ));
    let OpKind::Normal(crate::repo::Undo::Custom { undo, redo }, _) = kind else {
        return;
    };
    let entry = UndoEntry {
        label: "Empty first commit".to_owned(),
        undo,
        redo,
        heads: None,
    };
    let (undone, _) = run_steps(&dir, &entry.undo, OpKind::Undo(entry.clone()), None);
    let after_undo = (
        git::head(&dir),
        git::refs(&dir).unwrap_or_default().current,
        steps(&dir),
    );
    let (redone, _) = run_steps(&dir, &entry.redo, OpKind::Redo(entry.clone()), None);
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
    let (commit, undo) = empty_commit_steps();
    let (_, kind) = run_steps(&dir, &commit, OpKind::Normal(undo, String::new()), None);
    assert!(matches!(
        kind,
        OpKind::Normal(crate::repo::Undo::Custom { .. }, _)
    ));
    let OpKind::Normal(crate::repo::Undo::Custom { undo, redo }, _) = kind else {
        return;
    };
    assert!(git::run(Some(&dir), &["commit", "-q", "--allow-empty", "-m", "more"]).is_ok());
    let entry = UndoEntry {
        label: "Empty first commit".to_owned(),
        undo,
        redo,
        heads: None,
    };
    let (refused, _) = run_steps(&dir, &entry.undo, OpKind::Undo(entry.clone()), None);
    let head = git::head(&dir);
    let _ = std::fs::remove_dir_all(&dir);
    assert!(refused.is_err());
    assert!(!head.is_empty());
}

/// A tab of `dir` as if its first read were in: no commits, the changes row when `dirty`.
fn read_tab(ctx: &egui::Context, dir: &Path, dirty: bool) -> RepoTab {
    let mut tab = RepoTab::new_deferred(dir.to_path_buf(), ctx);
    tab.history_gen = 1;
    tab.refs = git::refs(dir).unwrap_or_default();
    tab.work = git::status(dir).unwrap_or_default();
    tab.remotes = git::remotes(dir).unwrap_or_default();
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
    tab.apply(crate::repo::Action::Guide(GuideAct::StartCommit), &ctx);
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(tab.selected, Some(0));
    assert!(tab.guide.focus_summary);
    assert!(Settings::get(&ctx).show_detail);
}

#[test]
fn make_the_first_commit_with_nothing_to_commit_says_so() {
    let ctx = egui::Context::default();
    let dir = empty_repo("start-nothing");
    let mut tab = read_tab(&ctx, &dir, false);
    tab.apply(crate::repo::Action::Guide(GuideAct::StartCommit), &ctx);
    let _ = std::fs::remove_dir_all(&dir);
    assert!(
        tab.status
            .as_ref()
            .is_some_and(|(m, err)| *err && m.contains("Nothing to commit"))
    );
    assert!(!tab.guide.focus_summary);
}

#[test]
fn a_hint_asked_for_by_the_tab_shows_once_and_is_kept_as_seen() {
    let ctx = egui::Context::default();
    let mut tab = RepoTab::new_deferred("/nonexistent/hint-test".into(), &ctx);
    tab.apply(
        crate::repo::Action::Guide(GuideAct::Hint(Hint::DoubleClickPill)),
        &ctx,
    );
    let up = tab.guide.hints.current(std::time::Instant::now());
    assert_eq!(up.map(|(h, _)| h), Some(Hint::DoubleClickPill));
    assert!(Settings::get(&ctx).seen_hints.contains("double-click-pill"));
    tab.guide.hints.close();
    tab.apply(
        crate::repo::Action::Guide(GuideAct::Hint(Hint::DoubleClickPill)),
        &ctx,
    );
    assert_eq!(tab.guide.hints.current(std::time::Instant::now()), None);
}

#[test]
fn hiding_a_branch_hints_at_solo_and_showing_it_again_does_not() {
    use crate::repo::branch_visibility::{Change, Visibility};
    let mut visibility = Visibility::default();
    let main = "refs/heads/main".to_owned();
    assert_eq!(
        super::visibility_hint(&Change::Toggle(main.clone()), &visibility),
        Some(Hint::SoloBranch)
    );
    visibility.toggle(&main);
    assert_eq!(
        super::visibility_hint(&Change::Toggle(main.clone()), &visibility),
        None
    );
    assert_eq!(super::visibility_hint(&Change::ShowAll, &visibility), None);
    assert_eq!(
        super::visibility_hint(&Change::Solo(main), &visibility),
        None
    );
}
