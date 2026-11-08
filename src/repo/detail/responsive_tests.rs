//! Geometry, eligibility and state preservation for responsive details and composition.
use super::*;
use egui_kittest::{Harness, kittest::Queryable};

fn tab() -> RepoTab {
    let ctx = egui::Context::default();
    let mut tab = RepoTab::new_deferred("/nonexistent/responsive-detail".into(), &ctx);
    tab.refs.head = "a".repeat(40);
    tab.refs.current = Some("main".into());
    tab.commit_summary = "feat: message stays when the panel folds".into();
    tab.commit_body = "A description that stays editable.".into();
    tab.identity.name = "An author with an exceptionally long name".into();
    tab.identity.email = "an-exceptionally-long-email-address@example.invalid".into();
    let file = crate::git::StatusFile {
        path: "src/a-very-long-folder/another-long-folder/a-very-long-file-name.rs".into(),
        old_path: None,
        area: crate::git::Area::Staged,
        code: 'M',
    };
    tab.work.staged.push(file.clone());
    tab.work.unstaged.push(crate::git::StatusFile {
        area: crate::git::Area::Unstaged,
        ..file
    });
    tab
}

fn harness(width: f32, scale: f32) -> Harness<'static, (RepoTab, Vec<Action>)> {
    let mut styled = false;
    Harness::builder()
        .with_size([width / scale, 750.0])
        .with_pixels_per_point(scale)
        .build_ui_state(
            move |ui, (tab, acts): &mut (RepoTab, Vec<Action>)| {
                if !styled {
                    crate::theme::install_fonts(ui.ctx());
                    crate::theme::apply(
                        ui.ctx(),
                        &crate::theme::ThemeId::default(),
                        &crate::theme::Tweaks::default(),
                    );
                    egui_extras::install_image_loaders(ui.ctx());
                    styled = true;
                    return;
                }
                egui::CentralPanel::default().show(ui, |ui| tab.changes_panel(ui, acts));
            },
            (tab(), Vec::new()),
        )
}

pub(in crate::repo) fn evidence<T>(h: &mut Harness<'_, T>, name: &str) {
    if let Ok(dir) = std::env::var("GITCRAFT_UI_EVIDENCE") {
        let directory = std::fs::create_dir_all(&dir);
        assert!(
            directory.is_ok(),
            "create screenshot directory: {directory:?}"
        );
        let rendered = h.render();
        assert!(
            rendered.is_ok(),
            "render {name}: {:?}",
            rendered.as_ref().err()
        );
        if let Ok(image) = rendered {
            let saved = image.save(std::path::Path::new(&dir).join(format!("{name}.png")));
            assert!(saved.is_ok(), "save {name}: {saved:?}");
        }
    }
}

#[test]
fn responsive_composer_matrix_reserves_identity_and_keeps_one_toolbar() {
    for width in [360.0, 520.0, 900.0] {
        for scale in [1.0, 1.25, 1.5] {
            let mut h = harness(width, scale);
            for _ in 0..4 {
                h.step();
            }
            let heading = h.get_by_label("COMMIT").rect();
            let mut controls = Vec::new();
            for label in [
                "Amend last commit",
                "Sign off",
                "Fold staged changes into an older commit",
                "Skip hooks",
                "Templates and prepare-commit-msg",
                "Type and scope (Conventional Commits)",
                "Add co-author",
                "Message options",
            ] {
                let r = h.get_by_label(label).rect();
                assert!(
                    !r.intersects(heading),
                    "{label} crosses heading at {width}/{scale}"
                );
                assert!(
                    r.left() >= 0.0 && r.right() <= width / scale,
                    "{label} outside panel at {width}/{scale}: {r:?}"
                );
                for other in &controls {
                    assert!(!r.intersects(*other), "tools overlap");
                }
                controls.push(r);
            }
            let toolbar_bottom = controls
                .iter()
                .map(|r| r.bottom())
                .fold(heading.bottom(), f32::max);
            assert!(
                toolbar_bottom - heading.top() <= 60.0,
                "toolbar stopped being compact at {width}/{scale}: {heading:?} to {toolbar_bottom}"
            );
            let icon = h.get_by_label("Commit as…").rect();
            let identity = h.get_by_label("as An author with an exceptionally long name <an-exceptionally-long-email-address@example.invalid> · global").rect();
            assert!(identity.right() <= icon.left());
            let summary = h.get_by_role(egui::accesskit::Role::TextInput).rect();
            assert!(
                summary.right() <= width / scale,
                "file scrolling widened editor"
            );
            let distribute = h.get_by_label("Distribute staged fixes…").rect();
            assert!(
                distribute.top() < 240.0,
                "distribute moved away from files header"
            );
            evidence(&mut h, &format!("compose-{width}-{scale}"));
        }
    }
}

#[test]
fn responsive_composer_toggles_keep_keyboard_and_continuation_guards() {
    let mut h = harness(520.0, 1.0);
    h.step();
    h.get_by_label("Amend last commit").focus();
    h.key_press(egui::Key::Space);
    h.step();
    assert!(h.state().0.amend);
    h.get_by_label("Sign off").click();
    h.step();
    assert!(
        h.state()
            .1
            .iter()
            .any(|a| matches!(a, Action::Dco(crate::repo::dco::DcoAct::SetSignOff(true))))
    );
    h.get_by_label("Skip hooks").click();
    h.step();
    assert!(h.state().0.skip_hooks.commit);
    h.state_mut().0.work.in_progress = Some(crate::git::InProgress::Rebase);
    h.step();
    h.get_by_label("Amend last commit").click();
    h.get_by_label("Skip hooks").click();
    h.step();
    assert!(h.state().0.amend);
    assert!(!h.state().0.skip_hooks.commit);
    h.state_mut().1.clear();
    h.get_by_label("Distribute staged fixes…").click();
    h.step();
    assert!(!h.state().1.iter().any(|a| matches!(a, Action::OpenAbsorb)));
    h.state_mut().0.work.in_progress = None;
    h.state_mut().0.refs.current = None;
    h.step();
    h.get_by_label("Distribute staged fixes…").click();
    h.step();
    assert!(!h.state().1.iter().any(|a| matches!(a, Action::OpenAbsorb)));
}

#[test]
fn responsive_selected_commit_metadata_and_all_files_header_matrix() {
    for width in [360.0, 520.0, 900.0] {
        for scale in [1.0, 1.25, 1.5] {
            let mut tab = tab();
            let path = tab.work.staged[0].path.clone();
            let hash = tab.refs.head.clone();
            tab.detail = Some(crate::git::CommitDetail {
                hash: hash.clone(), parents: vec!["b".repeat(40), "c".repeat(40)],
                author: tab.identity.name.clone(), email: tab.identity.email.clone(),
                author_date: "2026-11-07 19:27:35 +0100".into(), committer: "Another long committer name".into(),
                commit_date: "2026-11-07 20:00:00 +0100".into(),
                message: "fix: a selected commit with an exceptionally long subject that must wrap\n\nA longer description that remains selectable and editable when the panel changes size.".into(),
                files: vec![crate::git::ChangedFile { path: path.clone(), old_path: None, status: 'M', added: Some(1234), removed: Some(567) }],
                in_head: true, merges_after: false,
            });
            tab.detail_hash = Some(hash.clone());
            tab.all_files.of = Some(hash);
            tab.all_files.files = Some(Ok(vec![path, "unchanged.txt".into()]));
            let mut styled = false;
            let mut h = Harness::builder()
                .with_size([width / scale, 750.0])
                .with_pixels_per_point(scale)
                .build_ui_state(
                    move |ui, (tab, acts): &mut (RepoTab, Vec<Action>)| {
                        if !styled {
                            crate::theme::install_fonts(ui.ctx());
                            crate::theme::apply(
                                ui.ctx(),
                                &crate::theme::ThemeId::default(),
                                &crate::theme::Tweaks::default(),
                            );
                            egui_extras::install_image_loaders(ui.ctx());
                            styled = true;
                            return;
                        }
                        egui::CentralPanel::default().show(ui, |ui| tab.commit_panel(ui, acts));
                    },
                    (tab, Vec::new()),
                );
            for _ in 0..5 {
                h.step();
            }
            let all = h.get_by_label("View all files").rect();
            let summary = h.get_by_label("1 file changed  +1234  −567").rect();
            assert!(summary.right() <= all.left());
            assert!((summary.center().y - all.center().y).abs() < 1.0);
            h.get_by_label("View all files").click();
            h.step();
            assert!(
                h.state()
                    .1
                    .iter()
                    .any(|a| matches!(a, Action::ViewAllFiles(true)))
            );
            evidence(&mut h, &format!("selected-commit-{width}-{scale}"));
            h.state_mut().0.all_files.on = true;
            h.step();
            evidence(&mut h, &format!("all-files-{width}-{scale}"));
            assert!(h.query_by_label_contains("unchanged.txt").is_some());
        }
    }
}

#[test]
fn responsive_file_scrollers_are_independent_and_do_not_widen_editor() {
    let paths = ["src/a-very-long-folder/another-long-folder/a-very-long-file-name.rs"];
    let mut styled = false;
    let mut message = "typed draft".to_owned();
    let mut h = Harness::builder().with_size([300.0, 400.0]).build_ui_state(
        move |ui, state: &mut (egui::Rect, f32, f32)| {
            if !styled {
                crate::theme::install_fonts(ui.ctx());
                crate::theme::apply(
                    ui.ctx(),
                    &crate::theme::ThemeId::default(),
                    &crate::theme::Tweaks::default(),
                );
                styled = true;
                return;
            }
            egui::CentralPanel::default().show(ui, |ui| {
                let first = horizontal_files(ui, "first", &paths, |ui| {
                    ui.label(paths[0]);
                });
                state.0 = first.inner_rect;
                state.1 = first.state.offset.x;
                let second = horizontal_files(ui, "second", &paths, |ui| {
                    ui.label(paths[0]);
                });
                state.2 = second.state.offset.x;
                let edit = ui.add(
                    egui::TextEdit::singleline(&mut message)
                        .hint_text("Draft")
                        .desired_width(f32::INFINITY),
                );
                assert!(edit.rect.right() <= 300.0);
            });
        },
        (egui::Rect::NOTHING, 0.0, 0.0),
    );
    h.step();
    let pos = h.state().0.left_top() + egui::vec2(80.0, 8.0);
    h.event(egui::Event::PointerMoved(pos));
    h.step();
    h.event(egui::Event::MouseWheel {
        unit: egui::MouseWheelUnit::Point,
        delta: egui::vec2(-180.0, 0.0),
        modifiers: egui::Modifiers::NONE,
        phase: egui::TouchPhase::Move,
    });
    for _ in 0..4 {
        h.step();
    }
    assert!(
        h.state().1 > 0.0,
        "first list did not scroll: {:?}",
        h.state()
    );
    assert_eq!(
        h.state().2,
        0.0,
        "second list shared the first list's offset"
    );
}
