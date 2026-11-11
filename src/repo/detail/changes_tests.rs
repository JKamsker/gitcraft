use super::*;

/// Unstaging everything in the middle of a merge leaves the merge going on.
#[test]
fn unstage_all_keeps_the_merge_in_progress() -> Result<(), String> {
    let dir = std::env::temp_dir().join(format!("gitcraft-unstage-all-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let git = |args: &[&str]| crate::git::run(Some(&dir), args);
    let write =
        |name: &str, text: &str| std::fs::write(dir.join(name), text).map_err(|e| e.to_string());
    for args in [
        &["init", "-q", "-b", "main"][..],
        &["config", "user.name", "t"],
        &["config", "user.email", "t@t"],
        &["config", "commit.gpgSign", "false"],
    ] {
        git(args)?;
    }
    write("a", "a\n")?;
    git(&["add", "-A"])?;
    git(&["commit", "-qm", "a"])?;
    git(&["checkout", "-qb", "side"])?;
    write("b", "b\n")?;
    git(&["add", "-A"])?;
    git(&["commit", "-qm", "b"])?;
    git(&["checkout", "-q", "main"])?;
    write("c", "c\n")?;
    git(&["add", "-A"])?;
    git(&["commit", "-qm", "c"])?;
    git(&["merge", "-q", "--no-commit", "--no-ff", "side"])?;
    let head = git(&["rev-parse", "HEAD"])?;
    git(&unstage_all_args(head.trim()))?;
    let merging = git(&["rev-parse", "-q", "--verify", "MERGE_HEAD"]).is_ok();
    let staged = git(&["diff", "--cached", "--name-only"])?;
    let _ = std::fs::remove_dir_all(&dir);
    assert!(merging, "the merge was ended");
    assert_eq!(staged.trim(), "");
    Ok(())
}

fn changed(path: &str, area: Area) -> StatusFile {
    StatusFile {
        path: path.to_owned(),
        old_path: None,
        code: 'M',
        area,
    }
}

/// A file's menu left open while the status reloads is that file's: once the file is gone
/// from the list (staged in the terminal), the menu closes rather than turning into the
/// menu of the file that took its row, whose Ignore runs at once. The same path staged too
/// is a row of its own.
#[test]
fn a_file_menu_does_not_move_to_the_file_that_takes_its_row() {
    let ctx = egui::Context::default();
    crate::theme::install_fonts(&ctx);
    let mut tab = RepoTab::new_deferred("/nonexistent/file-menu-test".into(), &ctx);
    tab.work.unstaged = vec![
        changed("a.txt", Area::Unstaged),
        changed("b.txt", Area::Unstaged),
    ];
    tab.work.staged = vec![changed("b.txt", Area::Staged)];
    let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, vec2(320.0, 600.0));
    // Under the list's header (30 px), the second row's middle, away from its buttons.
    let second = pos2(100.0, 30.0 + widgets::ROW_H * 1.5);
    let frame = |tab: &RepoTab, events: Vec<egui::Event>| {
        let input = egui::RawInput {
            screen_rect: Some(screen),
            events,
            ..Default::default()
        };
        ctx.run_ui(input, |ui| {
            egui::CentralPanel::default()
                .frame(egui::Frame::NONE)
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = 0.0;
                    tab.unstaged_list(ui, &mut Vec::new());
                    tab.staged_list(ui, &mut Vec::new());
                });
        })
        .drop_without_applying_deltas();
    };
    frame(&tab, vec![egui::Event::PointerMoved(second)]);
    let click = |pressed| egui::Event::PointerButton {
        pos: second,
        button: egui::PointerButton::Secondary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    frame(&tab, vec![click(true), click(false)]);
    frame(&tab, Vec::new());
    let opened = egui::Popup::is_any_open(&ctx);
    // b.txt staged whole: c.txt, changed meanwhile, is in its row now.
    tab.work.unstaged = vec![
        changed("a.txt", Area::Unstaged),
        changed("c.txt", Area::Unstaged),
    ];
    frame(&tab, Vec::new());
    frame(&tab, Vec::new());
    assert!(opened, "the right-click opened no menu");
    assert!(!egui::Popup::is_any_open(&ctx));
}

/// Ctrl+click (Cmd on macOS) on a file picks it to stash instead of opening its diff;
/// picked, it shows selected.
#[test]
fn ctrl_click_picks_a_file_to_stash() {
    let ctx = egui::Context::default();
    crate::theme::install_fonts(&ctx);
    let mut tab = RepoTab::new_deferred("/nonexistent/pick-test".into(), &ctx);
    tab.work.unstaged = vec![
        changed("a b.txt", Area::Unstaged),
        changed("\u{fc}.txt", Area::Unstaged),
    ];
    let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, vec2(320.0, 600.0));
    let first = pos2(100.0, 30.0 + widgets::ROW_H * 0.5);
    let frame = |tab: &RepoTab, events: Vec<egui::Event>, modifiers| {
        let mut acts = Vec::new();
        let input = egui::RawInput {
            screen_rect: Some(screen),
            events: [egui::Event::ModifiersChanged(modifiers)]
                .into_iter()
                .chain(events)
                .collect(),
            ..Default::default()
        };
        ctx.run_ui(input, |ui| {
            egui::CentralPanel::default()
                .frame(egui::Frame::NONE)
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = 0.0;
                    tab.unstaged_list(ui, &mut acts);
                });
        })
        .drop_without_applying_deltas();
        acts
    };
    let click = |modifiers| {
        [true, false].map(|pressed| egui::Event::PointerButton {
            pos: first,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers,
        })
    };
    let command = egui::Modifiers::COMMAND;
    frame(&tab, vec![egui::Event::PointerMoved(first)], command);
    let acts = frame(&tab, click(command).to_vec(), command);
    assert!(
        matches!(acts.as_slice(), [Action::PickToStash(p)] if p == "a b.txt"),
        "a Ctrl+click did not pick the file"
    );
    // A plain click still opens the diff.
    let plain = egui::Modifiers::NONE;
    frame(&tab, vec![egui::Event::PointerMoved(first)], plain);
    let acts = frame(&tab, click(plain).to_vec(), plain);
    assert!(matches!(
        acts.as_slice(),
        [Action::OpenDiff(_), Action::UnfoldDiff]
    ));
}

/// The stash dialog opened from the toolbar with files picked offers them, chosen.
#[test]
fn the_stash_dialog_offers_the_files_picked() {
    let ctx = egui::Context::default();
    crate::theme::install_fonts(&ctx);
    let mut tab = RepoTab::new_deferred("/nonexistent/pick-dialog".into(), &ctx);
    tab.work.unstaged = vec![changed("a.txt", Area::Unstaged)];
    tab.stash_picks.toggle("a.txt");
    tab.dialog = Some(Dialog::stash());
    let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            vec2(800.0, 600.0),
        )),
        ..Default::default()
    };
    let mut acts = Vec::new();
    ctx.run_ui(input, |ui| tab.dialogs(ui.ctx(), &mut acts))
        .drop_without_applying_deltas();
    let files = crate::repo::partial_stash::StashMode::Files(vec!["a.txt".to_owned()]);
    assert!(
        matches!(&tab.dialog, Some(Dialog::Stash(d)) if d.request().mode == files),
        "the picked file isn't what the dialog stashes"
    );
    // Its git is being asked whether it stashes only what is staged.
    assert_ne!(
        tab.stash_picks.staged,
        crate::repo::partial_stash::StagedSupport::Unknown
    );
}
