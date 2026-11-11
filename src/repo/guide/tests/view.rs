// View regression coverage.

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
            .as_deref()
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
            .as_deref()
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
