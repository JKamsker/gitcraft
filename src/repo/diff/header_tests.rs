//! Responsive header regressions, with real egui hit targets and optional PNG evidence.
use super::*;
use egui_kittest::{Harness, kittest::Queryable};

const PATH: &str = "src/repo/a-very-long-folder/another-long-folder/graph_view/readability-with-a-very-long-file-name.rs";

fn harness(width: f32, scale: f32, split: bool) -> Harness<'static, (RepoTab, Vec<Action>)> {
    let ctx = egui::Context::default();
    let mut tab = RepoTab::new_deferred("/nonexistent/responsive-diff".into(), &ctx);
    let mut view = DiffView::new(DiffKey::Commit {
        hash: "a".repeat(40),
        path: PATH.into(),
    });
    view.data = Some(Ok(DiffData {
        revertable: true,
        ..Default::default()
    }));
    tab.diff = Some(view);
    let mut styled = false;
    Harness::builder()
        .with_size([width / scale, 280.0])
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
                egui::CentralPanel::default().show(ui, |ui| {
                    let Some(view) = tab.diff.as_ref() else {
                        return;
                    };
                    tab.diff_header(
                        ui,
                        view,
                        (
                            PATH,
                            "Commit aaaaaaaaaaaa · origin/feature/a-long-branch-name",
                        ),
                        false,
                        (split, false, false, tab.diff_as_file, false),
                        (Some(false), &[], None, false),
                        acts,
                    );
                });
            },
            (tab, Vec::new()),
        )
}

fn evidence(h: &mut Harness<'_, (RepoTab, Vec<Action>)>, name: &str) {
    crate::repo::detail::responsive_tests::evidence(h, name);
}

#[test]
fn responsive_diff_header_matrix_keeps_title_and_fixed_switch_separate()
-> Result<(), Box<dyn std::error::Error>> {
    for width in [360.0, 800.0, 1280.0] {
        for scale in [1.0, 1.25, 1.5] {
            for split in [false, true] {
                let mut h = harness(width, scale, split);
                for _ in 0..4 {
                    h.step();
                }
                let file = h.get_by_label("File").rect();
                let diff = h.get_by_label("Diff").rect();
                let title = h.get_by_label(PATH).rect();
                assert!(
                    title.right() <= diff.left(),
                    "path crosses Diff at {width}/{scale}"
                );
                assert!(diff.right() <= file.left());
                assert!(file.right() <= width / scale);
                let fixed = (file, diff);
                h.state_mut().0.diff_as_file = true;
                h.state_mut()
                    .0
                    .diff
                    .as_mut()
                    .ok_or("responsive diff fixture is missing its view")?
                    .chosen
                    .extend([1, 2, 3]);
                h.step();
                assert_eq!(
                    fixed,
                    (h.get_by_label("File").rect(), h.get_by_label("Diff").rect())
                );
                evidence(
                    &mut h,
                    &format!(
                        "diff-{width}-{scale}-{}-file",
                        if split { "split" } else { "unified" }
                    ),
                );
                h.state_mut().0.diff_as_file = false;
                h.step();
                evidence(
                    &mut h,
                    &format!(
                        "diff-{width}-{scale}-{}",
                        if split { "split" } else { "unified" }
                    ),
                );
            }
        }
    }
    Ok(())
}

#[test]
fn responsive_diff_revert_keeps_selected_lines_and_busy_guard()
-> Result<(), Box<dyn std::error::Error>> {
    let mut h = harness(800.0, 1.0, false);
    h.step();
    h.get_by_label("Revert file").click();
    h.step();
    assert!(matches!(h.state().1.last(), Some(Action::RevertFile { path, .. }) if path == PATH));
    h.state_mut().1.clear();
    h.state_mut()
        .0
        .diff
        .as_mut()
        .ok_or("responsive diff fixture is missing its view")?
        .chosen
        .extend([2, 7]);
    h.step();
    h.get_by_label("Revert 2 selected lines").click();
    h.step();
    assert!(
        matches!(h.state().1.last(), Some(Action::PickLines { pick: Pick::Revert, lines }) if lines.iter().copied().collect::<Vec<_>>() == [2,7])
    );
    h.state_mut().1.clear();
    h.state_mut().0.busy_op = Some("Busy".into());
    h.step();
    h.get_by_label("Revert 2 selected lines").click();
    h.step();
    assert!(h.state().1.is_empty());
    Ok(())
}

#[test]
fn responsive_diff_overflow_keeps_algorithm_choices_and_keyboard_access()
-> Result<(), Box<dyn std::error::Error>> {
    let mut h = harness(360.0, 1.5, true);
    h.step();
    h.get_by_label("Diff actions and view options").click();
    h.step();
    let algorithm = h.get_by_label_contains("Diff algorithm: Default");
    algorithm.focus();
    h.step();
    h.key_press(egui::Key::Enter);
    h.step();
    for label in ["Default", "Myers", "Minimal", "Patience", "Histogram"] {
        assert!(h.query_by_label(label).is_some(), "missing {label}");
    }
    h.get_by_label("Histogram").click();
    h.step();
    assert!(h.state().1.iter().any(|a| matches!(
        a,
        Action::SetDiffAlgorithm(Some(crate::git::DiffAlgorithm::Histogram))
    )));
    evidence(&mut h, "diff-overflow-algorithm");
    Ok(())
}

#[test]
fn responsive_diff_body_matrix_preserves_horizontal_scroll_and_line_selection()
-> Result<(), Box<dyn std::error::Error>> {
    for width in [360.0, 800.0, 1280.0] {
        for scale in [1.0, 1.25, 1.5] {
            for split in [false, true] {
                let ctx = egui::Context::default();
                let mut tab = RepoTab::new_deferred("/nonexistent/responsive-body".into(), &ctx);
                let key = DiffKey::Commit {
                    hash: "a".repeat(40),
                    path: PATH.into(),
                };
                let long = "a_long_token_that_requires_horizontal_scrolling_".repeat(12);
                let patch = format!(
                    "diff --git a/{PATH} b/{PATH}\nindex 1234567..2345678 100644\n--- a/{PATH}\n+++ b/{PATH}\n@@ -1,3 +1,3 @@\n unchanged\n-old_{long}\n+new_{long}\n after\n@@ -20 +20 @@\n-second_before\n+second_after\n"
                );
                let mut view = DiffView::new(key.clone());
                view.unfolded = true;
                view.data = Some(Ok(super::super::prepare(
                    std::path::Path::new("/nonexistent"),
                    &key,
                    PATH,
                    &patch,
                    true,
                    true,
                    true,
                )));
                view.file = Some(Ok(crate::repo::file_text::FileText::new(
                    format!("unchanged\nnew_{long}\nafter\n").as_bytes(),
                    PATH,
                    true,
                )));
                tab.diff = Some(view);
                let mut styled = false;
                let mut h = Harness::builder()
                    .with_size([width / scale, 480.0])
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
                                let mut settings = crate::settings::Settings::get(ui.ctx());
                                settings.diff_split = split;
                                settings.set(ui.ctx());
                                styled = true;
                                return;
                            }
                            egui::CentralPanel::default().show(ui, |ui| tab.diff_view(ui, acts));
                        },
                        (tab, Vec::new()),
                    );
                for _ in 0..4 {
                    h.step();
                }
                let name = format!(
                    "diff-body-{width}-{scale}-{}",
                    if split { "split" } else { "unified" }
                );
                evidence(&mut h, &name);
                let before =
                    text_position(h.output(), "old_").ok_or("removed line was not painted")?;
                let pos = before + egui::vec2(8.0, super::super::LINE_H * 0.5);
                h.event(egui::Event::PointerMoved(pos));
                h.step();
                for pressed in [true, false] {
                    h.event(egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    });
                    h.step();
                }
                assert!(
                    h.state()
                        .0
                        .diff
                        .as_ref()
                        .ok_or("responsive diff fixture is missing its view")?
                        .chosen
                        .is_empty(),
                    "code click picked a Git line at {width}/{scale}/{split}"
                );
                let second = text_position(h.output(), "second_after")
                    .ok_or("second hunk was not painted")?
                    + egui::vec2(8.0, super::super::LINE_H * 0.5);
                h.event(egui::Event::PointerMoved(second));
                h.step();
                for pressed in [true, false] {
                    h.event(egui::Event::PointerButton {
                        pos: second,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    });
                    h.step();
                }
                assert_eq!(
                    h.state().0.diff.as_ref().and_then(|v| v.keyboard_hunk),
                    Some(5),
                    "code caret must focus the second hunk for keyboard actions"
                );
                let pos = egui::pos2(width / scale * 0.5, 230.0);
                h.event(egui::Event::PointerMoved(pos));
                h.step();
                h.event(egui::Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    delta: egui::vec2(-160.0, 0.0),
                    modifiers: egui::Modifiers::NONE,
                    phase: egui::TouchPhase::Move,
                });
                for _ in 0..4 {
                    h.step();
                }
                let after = text_position(h.output(), "old_").ok_or("removed line disappeared")?;
                assert!(
                    after.x < before.x,
                    "diff did not scroll horizontally at {width}/{scale}/{split}: {before:?} → {after:?}"
                );
                evidence(&mut h, &format!("{name}-scrolled"));
                h.state_mut().0.diff_as_file = true;
                h.step();
                evidence(&mut h, &format!("{name}-file"));
                assert!(h.get_by_label("File").rect().right() <= width / scale);
            }
        }
    }
    Ok(())
}

fn text_position(output: &egui::FullOutput, prefix: &str) -> Option<egui::Pos2> {
    fn find(shape: &egui::epaint::Shape, prefix: &str) -> Option<egui::Pos2> {
        match shape {
            egui::epaint::Shape::Text(text) if text.galley.job.text.starts_with(prefix) => {
                Some(text.pos)
            }
            egui::epaint::Shape::Vec(shapes) => shapes.iter().find_map(|s| find(s, prefix)),
            _ => None,
        }
    }
    output.shapes.iter().find_map(|s| find(&s.shape, prefix))
}

#[test]
fn responsive_pr_line_comment_keeps_long_title_close_and_draft_separate()
-> Result<(), Box<dyn std::error::Error>> {
    for width in [360.0, 800.0, 1280.0] {
        for scale in [1.0, 1.25, 1.5] {
            let ctx = egui::Context::default();
            let mut tab = RepoTab::new_deferred("/nonexistent/responsive-pr".into(), &ctx);
            tab.open_pr(171, &ctx);
            let from = "a".repeat(40);
            let to = "b".repeat(40);
            let pr = tab
                .pr_view
                .as_mut()
                .ok_or("responsive diff fixture is missing its view")?;
            pr.details = Some(Ok(crate::pull_request::PrDetails {
                number: 171,
                state: "OPEN".into(),
                head_oid: to.clone(),
                ..Default::default()
            }));
            pr.files = Some(Ok(crate::pull_request::PrFiles {
                from: from.clone(),
                to: to.clone(),
                files: Vec::new(),
            }));
            let key = DiffKey::Range {
                from,
                to: Some(to),
                path: PATH.into(),
            };
            let patch = format!(
                "diff --git a/{PATH} b/{PATH}\nindex 1234567..2345678 100644\n--- a/{PATH}\n+++ b/{PATH}\n@@ -1 +1 @@\n-before\n+after\n"
            );
            let mut view = DiffView::new(key.clone());
            view.unfolded = true;
            view.data = Some(Ok(super::super::prepare(
                std::path::Path::new("/nonexistent"),
                &key,
                PATH,
                &patch,
                false,
                true,
                true,
            )));
            tab.diff = Some(view);
            let target = crate::repo::pr_review::LineTarget {
                path: PATH.into(),
                line: 1,
                side: crate::pull_request::DiffSide::Right,
            };
            tab.pr_drafts.insert(
                171,
                crate::repo::pr_review::PrDraft {
                    target: Some(target),
                    line_text: "Review draft stays intact".into(),
                    ..Default::default()
                },
            );
            let mut styled = false;
            let mut h = Harness::builder()
                .with_size([width / scale, 550.0])
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
                        egui::CentralPanel::default().show(ui, |ui| tab.diff_view(ui, acts));
                    },
                    (tab, Vec::new()),
                );
            for _ in 0..4 {
                h.step();
            }
            let title = h.get_by_label(&format!("{PATH}:1")).rect();
            let close = h
                .query_all_by_label("Close")
                .map(|n| n.rect())
                .find(|r| r.center().y > 200.0)
                .ok_or("comment close is missing")?;
            assert!(title.right() <= close.left());
            h.get_by_label("Comment now").click();
            h.step();
            assert!(
                h.state()
                    .1
                    .iter()
                    .any(|a| matches!(a, Action::Pr(crate::repo::pr_review::PrAct::PostLineNow)))
            );
            assert_eq!(
                h.state().0.pr_drafts[&171].line_text,
                "Review draft stays intact"
            );
            evidence(&mut h, &format!("pr-comment-{width}-{scale}"));
        }
    }
    Ok(())
}
