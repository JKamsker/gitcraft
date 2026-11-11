use super::*;
use egui_kittest::{Harness, kittest::Queryable};

fn harness(width: f32, height: f32, scale: f32) -> Harness<'static, (RepoTab, Vec<Action>)> {
    let ctx = egui::Context::default();
    let mut tab = RepoTab::new_deferred("/nonexistent/responsive-followup".into(), &ctx);
    tab.refs.head = "a".repeat(40);
    tab.refs.current = Some("main".into());
    tab.commits.push(std::sync::Arc::new(crate::git::Commit {
        hash: crate::repo::WIP_HASH.into(),
        ..Default::default()
    }));
    tab.selected = Some(0);
    tab.commit_summary = "keep this short-height draft".into();
    tab.commit_body = "and its description".into();
    tab.identity.name = "Test author".into();
    tab.identity.email = "author@example.invalid".into();
    tab.work.staged.push(crate::git::StatusFile {
        path: "src/file.rs".into(),
        old_path: None,
        area: crate::git::Area::Staged,
        code: 'M',
    });
    tab.focus_commit_summary = true;
    let mut styled = false;
    Harness::builder()
        .with_size([width / scale, height / scale])
        .with_pixels_per_point(scale)
        .build_ui_state(
            move |ui, (tab, acts): &mut (RepoTab, Vec<Action>)| {
                if !styled {
                    crate::theme::install_fonts(ui.ctx());
                    crate::theme::apply(
                        ui.ctx(),
                        &crate::theme::ThemeId::default(),
                        &Default::default(),
                    );
                    egui_extras::install_image_loaders(ui.ctx());
                    let mut settings = Settings::get(ui.ctx());
                    settings.show_branches = false;
                    settings.show_detail = true;
                    settings.set(ui.ctx());
                    styled = true;
                    return;
                }
                // Reserve room for app chrome and a wrapped repository toolbar, including the
                // extra height that the parent's integrated toolbar uses in narrow windows.
                egui::Panel::top("followup-toolbar")
                    .exact_size(70.0)
                    .show(ui, |ui| {
                        ui.label("Repository toolbar");
                    });
                tab.side_panels(ui, acts);
                egui::CentralPanel::default().show(ui, |ui| {
                    ui.label("History");
                });
            },
            (tab, Vec::new()),
        )
}

#[test]
fn responsive_followup_full_height_composer_needs_no_scrolling_after_resize()
-> Result<(), Box<dyn std::error::Error>> {
    for scale in [1.0, 1.25, 1.5] {
        let mut h = harness(1400.0, 860.0, scale);
        for _ in 0..8 {
            h.step();
        }
        // A persisted small panel size must not cap the composition area forever.
        let mut panel =
            egui::PanelState::load(&h.ctx, egui::Id::new("commit_box")).ok_or("composer panel")?;
        panel.outer_rect.set_height(90.0);
        h.ctx
            .data_mut(|d| d.insert_persisted(egui::Id::new("commit_box"), panel));
        for _ in 0..6 {
            h.step();
        }
        for resized in [false, true] {
            if resized {
                h.set_size(egui::vec2(800.0 / scale, 400.0 / scale));
                for _ in 0..6 {
                    h.step();
                }
                if h.query_by_label("Show the detail panel").is_some() {
                    h.get_by_label("Show the detail panel").click();
                    for _ in 0..6 {
                        h.step();
                    }
                }
                h.get_by_label("Commit 1 file").scroll_to_me();
                for _ in 0..6 {
                    h.step();
                }
                h.set_size(egui::vec2(1400.0 / scale, 860.0 / scale));
                for _ in 0..8 {
                    h.step();
                }
            }
            let panel = egui::PanelState::load(&h.ctx, egui::Id::new("commit_box"))
                .ok_or("composer panel")?
                .outer_rect;
            let commit = h.get_by_label("Commit 1 file").rect();
            assert!(
                panel.shrink(2.0).contains_rect(commit),
                "commit clipped at {scale}, resized={resized}: {commit:?}, panel {panel:?}"
            );
            for field in h.query_all_by_role(egui::accesskit::Role::TextInput) {
                assert!(
                    panel.shrink(2.0).contains_rect(field.rect()),
                    "message field clipped at {scale}, resized={resized}: {:?}, panel {panel:?}",
                    field.rect()
                );
            }
            let toolbar = h.get_by_label("Message options").rect();
            assert!(
                panel.shrink(2.0).contains_rect(toolbar),
                "toolbar scrolled out at full height"
            );
            // Check real pointer reachability, not just an offscreen accessibility node.
            h.get_by_label("Commit 1 file").click();
            h.step();
            assert!(h.state().1.iter().any(|a| matches!(a, Action::Commit)));
            h.state_mut().1.clear();
            crate::repo::detail::responsive_tests::evidence(
                &mut h,
                &format!("followup-full-composer-1400-860-{scale}-{resized}"),
            );
        }
    }
    Ok(())
}

#[test]
fn responsive_followup_short_reopen_scrolls_composer_actions_into_view() {
    for height in [400.0, 500.0, 650.0] {
        for scale in [1.0, 1.25, 1.5] {
            let mut h = harness(800.0, height, scale);
            for _ in 0..6 {
                h.step();
            }
            if h.query_by_label("Show the detail panel").is_some() {
                h.get_by_label("Show the detail panel").click();
                for _ in 0..6 {
                    h.step();
                }
            }
            assert!(h.query_by_label("Hide the detail panel").is_some());
            h.get_by_label("Commit 1 file").scroll_to_me();
            for _ in 0..6 {
                h.step();
            }
            crate::repo::detail::responsive_tests::evidence(
                &mut h,
                &format!("followup-short-composer-{height}-{scale}"),
            );
            let r = h.get_by_label("Commit 1 file").rect();
            assert!(
                r.top() >= 70.0 && r.bottom() < height / scale - 20.0,
                "commit action outside detail viewport at {height}/{scale}: {r:?}"
            );
            h.get_by_label("Commit 1 file").click();
            h.step();
            assert!(
                h.state().1.iter().any(|a| matches!(a, Action::Commit)),
                "commit action could not be clicked at {height}/{scale}"
            );
            h.get_by_label("Skip hooks").scroll_to_me();
            for _ in 0..6 {
                h.step();
            }
            let r = h.get_by_label("Skip hooks").rect();
            assert!(r.top() >= 70.0 && r.bottom() < height / scale - 20.0);
            h.get_by_label("Skip hooks").click();
            h.step();
            for _ in 0..3 {
                h.step();
            }
            crate::repo::detail::responsive_tests::evidence(
                &mut h,
                &format!("followup-short-toolbar-{height}-{scale}"),
            );
            assert!(h.state().0.skip_hooks.commit);
            assert_eq!(h.state().0.commit_summary, "keep this short-height draft");
            assert_eq!(h.state().0.commit_body, "and its description");
        }
    }
}

#[test]
fn responsive_followup_forced_reopen_divider_collapses_and_restores_state()
-> Result<(), Box<dyn std::error::Error>> {
    let mut h = harness(800.0, 650.0, 1.5);
    for _ in 0..6 {
        h.step();
    }
    let mut settings = Settings::get(&h.ctx);
    settings.show_branches = true;
    settings.set(&h.ctx);
    h.set_size(egui::vec2(1100.0, 700.0));
    for _ in 0..6 {
        h.step();
    }
    let original = egui::PanelState::load(&h.ctx, egui::Id::new("repo_detail"))
        .ok_or("detail state")?
        .size()
        .x;
    h.state_mut().0.amend = true;
    h.state_mut().0.focus_commit_summary = true;
    h.step();
    h.set_size(egui::vec2(800.0 / 1.5, 650.0 / 1.5));
    for _ in 0..6 {
        h.step();
    }
    // This width fits until branches/chrome take room; shorten to force automatic collapse.
    h.set_size(egui::vec2(800.0 / 1.5, 500.0 / 1.5));
    for _ in 0..6 {
        h.step();
    }
    h.get_by_label("Show the detail panel").click();
    for _ in 0..6 {
        h.step();
    }
    h.set_size(egui::vec2(800.0 / 1.5, 650.0 / 1.5));
    for _ in 0..6 {
        h.step();
    }
    assert!(h.query_by_label("Hide the detail panel").is_some());
    h.state_mut().0.focus_commit_summary = true;
    h.step();
    assert!(
        h.query_all_by_role(egui::accesskit::Role::TextInput)
            .any(|n| n.is_focused())
    );
    // Drag the actual divider, matching the review's x≈270 -> 370 gesture.
    let rect = egui::PanelState::load(&h.ctx, egui::Id::new("repo_detail"))
        .ok_or("expanded state")?
        .outer_rect;
    assert!(rect.width() >= DETAIL_COLLAPSE_W);
    let start = egui::pos2(rect.left(), rect.top() + 80.0);
    let end = egui::pos2(rect.right() - 163.0, start.y);
    h.event(egui::Event::PointerMoved(start));
    h.step();
    h.event(egui::Event::PointerButton {
        pos: start,
        button: egui::PointerButton::Primary,
        pressed: true,
        modifiers: egui::Modifiers::NONE,
    });
    h.step();
    h.event(egui::Event::PointerMoved(end));
    for _ in 0..3 {
        h.step();
    }
    h.event(egui::Event::PointerButton {
        pos: end,
        button: egui::PointerButton::Primary,
        pressed: false,
        modifiers: egui::Modifiers::NONE,
    });
    for _ in 0..6 {
        h.step();
    }
    crate::repo::detail::responsive_tests::evidence(&mut h, "followup-forced-divider-collapsed");
    assert!(
        h.query_by_label("Show the detail panel").is_some(),
        "narrowing a forced-open panel left it expanded and blank"
    );
    assert!(!Settings::get(&h.ctx).show_detail);
    assert_eq!(h.state().0.commit_summary, "keep this short-height draft");
    assert!(h.state().0.amend);
    h.set_size(egui::vec2(1100.0, 700.0));
    for _ in 0..6 {
        h.step();
    }
    let mut settings = Settings::get(&h.ctx);
    settings.show_detail = true;
    settings.set(&h.ctx);
    for _ in 0..6 {
        h.step();
    }
    let restored = egui::PanelState::load(&h.ctx, egui::Id::new("repo_detail"))
        .ok_or("restored state")?
        .size()
        .x;
    assert!(
        (original - restored).abs() < 1.0,
        "preferred width changed: {original} -> {restored}"
    );
    assert!(
        h.query_all_by_role(egui::accesskit::Role::TextInput)
            .any(|n| n.is_focused()),
        "editor focus lost across collapse/reopen"
    );
    Ok(())
}
