//! Real pointer events against the diff's code and Git-action hit zones.
use super::*;
use crate::repo::pr_review::PrAct;
use egui::{Event, Modifiers, PointerButton, Pos2};
use egui_kittest::Harness;

#[path = "selection_tests/fixture.rs"]
mod fixture;
use fixture::{button, click, harness, position, selected};

/// Verify caret, word, line, drag, and copy behavior across layouts and staging modes.
#[test]
fn code_click_drag_word_line_and_copy_matrix() {
    for split in [false, true] {
        for wrap in [false, true] {
            for partial in [false, true] {
                let mut h = harness(split, wrap, partial, false);
                for prefix in ["old_αlpha", "new_αlpha", "context"] {
                    let pos = position(&h, prefix, 5);
                    click(&mut h, pos, Modifiers::NONE);
                    assert!(h.state().view.text_sel.is_some_and(|s| s.is_empty()));
                    assert!(h.state().view.chosen.is_empty());
                    click(&mut h, pos, Modifiers::NONE);
                    assert_eq!(selected(&h), prefix);
                    assert!(h.state().view.chosen.is_empty());
                    click(&mut h, pos, Modifiers::NONE);
                    assert_eq!(
                        selected(&h),
                        format!(
                            "{prefix} {}",
                            if prefix == "context" { "value" } else { "beta" }
                        )
                    );
                    // A fresh drag must replace the previous line selection, from its press.
                    let from = position(&h, prefix, 0);
                    let to = position(&h, prefix, 3);
                    h.event(Event::PointerMoved(from));
                    h.step();
                    button(&mut h, from, true, Modifiers::NONE);
                    assert_eq!(selected(&h), "");
                    h.event(Event::PointerMoved(to));
                    h.step();
                    button(&mut h, to, false, Modifiers::NONE);
                    assert_eq!(selected(&h), prefix.chars().take(3).collect::<String>());
                    h.event(Event::Copy);
                    h.step();
                    assert!(h.output().platform_output.commands.iter().any(|c| matches!(
                        c, egui::OutputCommand::CopyText(t) if t == &selected(&h)
                    )));
                    assert!(h.state().view.chosen.is_empty());
                    h.run_steps(40); // Finish the previous multi-click sequence.
                }
            }
        }
    }
}

/// Verify newly added files support word selection and replacement by a fresh drag.
#[test]
fn new_file_uses_the_same_word_and_replacement_selection() {
    for split in [false, true] {
        let mut h = harness(split, false, false, true);
        let word = position(&h, "new_αlpha", 5);
        click(&mut h, word, Modifiers::NONE);
        click(&mut h, word, Modifiers::NONE);
        assert_eq!(selected(&h), "new_αlpha");
        let next = position(&h, "second", 2);
        click(&mut h, next, Modifiers::NONE);
        assert_eq!(selected(&h), "");
        click(&mut h, next, Modifiers::NONE);
        assert_eq!(selected(&h), "second");
    }
}

/// Verify Git marker toggles and Shift ranges do not interfere with code selections.
#[test]
fn marker_toggle_and_shift_range_are_separate_from_text() {
    for split in [false, true] {
        let mut h = harness(split, false, true, false);
        let code = position(&h, "old_αlpha", 0);
        let marker = code - egui::vec2(super::super::MARK_W / 2.0, 0.0);
        click(&mut h, marker, Modifiers::NONE);
        assert_eq!(h.state().view.chosen, [2].into());
        assert!(h.state().view.text_sel.is_none());
        click(&mut h, marker, Modifiers::NONE);
        assert!(h.state().view.chosen.is_empty());
        click(&mut h, marker, Modifiers::NONE);
        let last = position(&h, "new_second", 0) - egui::vec2(super::super::MARK_W / 2.0, 0.0);
        click(&mut h, last, Modifiers::SHIFT);
        assert_eq!(h.state().view.chosen, [2, 3, 4, 5].into());
        click(&mut h, code, Modifiers::NONE);
        assert!(h.state().view.chosen.is_empty());
        assert_eq!(selected(&h), "");
    }
}

/// Verify logical character selection survives wrapping and horizontal scrolling.
#[test]
fn wrapped_and_horizontally_scrolled_text_uses_galley_character_positions() {
    for split in [false, true] {
        for wrap in [false, true] {
            let mut h = harness(split, wrap, true, false);
            if !wrap {
                let pos = position(&h, "long_token", 5);
                h.event(Event::PointerMoved(pos));
                h.step();
                h.event(Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    delta: egui::vec2(-160.0, 0.0),
                    modifiers: Modifiers::NONE,
                    phase: egui::TouchPhase::Move,
                });
                h.run_steps(12);
            }
            let from = position(&h, "long_token", 29);
            click(&mut h, from, Modifiers::NONE);
            click(&mut h, from, Modifiers::NONE);
            assert_eq!(selected(&h), "long_token");
            // Cross a visual wrap without introducing a newline into the copied text.
            if wrap {
                let to = position(&h, "long_token", 125);
                assert!(to.y > from.y);
                h.event(Event::PointerMoved(from));
                h.step();
                button(&mut h, from, true, Modifiers::NONE);
                h.event(Event::PointerMoved(to));
                h.step();
                button(&mut h, to, false, Modifiers::NONE);
                let expected: String = "long_token ".repeat(50).chars().skip(29).take(96).collect();
                assert_eq!(selected(&h), expected);
            }
            assert!(h.state().view.chosen.is_empty());
        }
    }
}

/// Verify clicking unused code space clears the prior selection.
#[test]
fn clicking_blank_code_area_clears_the_previous_selection() {
    let mut h = harness(false, false, true, false);
    let word = position(&h, "new_αlpha", 5);
    click(&mut h, word, Modifiers::NONE);
    click(&mut h, word, Modifiers::NONE);
    assert_eq!(selected(&h), "new_αlpha");
    click(&mut h, egui::pos2(600.0, 350.0), Modifiers::NONE);
    assert!(h.state().view.text_sel.is_none());
    assert!(h.state().view.chosen.is_empty());
}

/// Verify a code context menu copies the selected word without selecting Git lines.
#[test]
fn right_click_copy_preserves_a_word_selection_without_picking_git_lines() {
    use egui_kittest::kittest::Queryable;
    let mut h = harness(false, false, true, false);
    let word = position(&h, "new_αlpha", 5);
    click(&mut h, word, Modifiers::NONE);
    click(&mut h, word, Modifiers::NONE);
    for pressed in [true, false] {
        h.event(Event::PointerButton {
            pos: word,
            button: PointerButton::Secondary,
            pressed,
            modifiers: Modifiers::NONE,
        });
        h.step();
    }
    h.run();
    assert_eq!(selected(&h), "new_αlpha");
    assert!(h.state().view.chosen.is_empty());
    h.get_by_label_contains("Copy").click();
    h.step();
    assert!(h.output().platform_output.commands.iter().any(|c| matches!(
        c, egui::OutputCommand::CopyText(t) if t == "new_αlpha"
    )));
}

/// Verify closing a context menu resets the next primary click to a caret.
#[test]
fn dismissing_a_context_menu_starts_a_fresh_primary_selection() {
    let mut h = harness(false, false, true, false);
    let word = position(&h, "new_αlpha", 5);
    click(&mut h, word, Modifiers::NONE);
    click(&mut h, word, Modifiers::NONE);
    for pressed in [true, false] {
        h.event(Event::PointerButton {
            pos: word,
            button: PointerButton::Secondary,
            pressed,
            modifiers: Modifiers::NONE,
        });
        h.step();
    }
    h.key_press(egui::Key::Escape);
    h.step();
    click(&mut h, word, Modifiers::NONE);
    assert_eq!(selected(&h), "");
    assert!(h.state().view.chosen.is_empty());
}

/// Verify a code context menu replaces the prior Git selection with text.
#[test]
fn code_context_menu_replaces_an_existing_git_line_selection() {
    let mut h = harness(false, false, true, false);
    let marker = position(&h, "old_αlpha", 0) - egui::vec2(super::super::MARK_W / 2.0, 0.0);
    click(&mut h, marker, Modifiers::NONE);
    assert_eq!(h.state().view.chosen, [2].into());
    let code = position(&h, "new_αlpha", 5);
    h.event(Event::PointerMoved(code));
    h.step();
    for pressed in [true, false] {
        h.event(Event::PointerButton {
            pos: code,
            button: PointerButton::Secondary,
            pressed,
            modifiers: Modifiers::NONE,
        });
        h.step();
    }
    assert_eq!(selected(&h), "new_αlpha beta");
    assert!(h.state().view.chosen.is_empty());
}

/// Verify Select All clears Git picks and selects the displayed text.
#[test]
fn select_all_replaces_git_line_selection() {
    let mut h = harness(false, false, true, false);
    let marker = position(&h, "old_αlpha", 0) - egui::vec2(super::super::MARK_W / 2.0, 0.0);
    click(&mut h, marker, Modifiers::NONE);
    assert!(!h.state().view.chosen.is_empty());
    h.key_press_modifiers(Modifiers::COMMAND, egui::Key::A);
    h.step();
    assert!(!selected(&h).is_empty());
    assert!(h.state().view.chosen.is_empty());
}

/// Verify history uses the caret coordinates and revision for each diff side.
#[test]
fn history_shortcut_uses_the_caret_line_on_each_side() {
    for (prefix, rev) in [
        ("old_αlpha", "recorded^"),
        ("new_αlpha", "recorded"),
        ("context", "recorded^"),
    ] {
        let mut h = harness(true, false, true, false);
        let pos = position(&h, prefix, 3);
        click(&mut h, pos, Modifiers::NONE);
        assert_eq!(selected(&h), "");
        h.key_press(egui::Key::F8);
        h.step();
        let line = if prefix == "context" { 1 } else { 2 };
        assert!(
            h.state().acts.iter().any(|a| matches!(a,
                Action::LineHistory { at: crate::git::LinesAt::Rev(at), range, .. }
                    if at == rev && *range == (line, line)
            )),
            "caret history mismatch for {prefix}: {:?}",
            h.state().view.text_sel
        );
    }
}

/// Verify comment-number clicks and Git marker clicks dispatch independent actions.
#[test]
fn comment_numbers_and_git_markers_keep_separate_actions() {
    for split in [false, true] {
        for local in [false, true] {
            let mut h = harness(split, false, true, false);
            h.state_mut().review_local = Some(local);
            h.run_steps(2);
            let code = position(&h, "old_αlpha", 0);
            let number = code - egui::vec2(super::super::MARK_W + super::super::NUM_W / 2.0, 0.0);
            click(
                &mut h,
                number,
                if local {
                    Modifiers::ALT
                } else {
                    Modifiers::NONE
                },
            );
            assert!(h.state().acts.iter().any(|a| match a {
                Action::Agent(crate::repo::agents::AgentAct::OpenNote(t)) if local =>
                    t.line == 2 && t.side == DiffSide::Left,
                Action::Pr(PrAct::OpenLine(t)) if !local => t.line == 2 && t.side == DiffSide::Left,
                _ => false,
            }));
            assert!(h.state().view.chosen.is_empty());
            assert!(h.state().view.text_sel.is_none());
            let marker = code - egui::vec2(super::super::MARK_W / 2.0, 0.0);
            click(&mut h, marker, Modifiers::NONE);
            assert_eq!(h.state().view.chosen, [2].into());
            assert_eq!(h.state().acts.len(), 1);
        }
    }
}

/// Verify scrolling during a drag preserves its original side and anchor.
#[test]
fn drag_keeps_its_original_side_and_anchor_when_scrolled_out_of_view() {
    let mut h = harness(true, false, true, false);
    if let Some(Ok(data)) = &mut h.state_mut().view.data {
        for n in 5..100 {
            data.lines.push(super::super::DiffLine {
                kind: Kind::Context,
                old: Some(n),
                new: Some(n),
                text: format!("vertical_{n}"),
                raw: format!(" vertical_{n}"),
            });
        }
        data.split = super::super::parse::pair_lines(&data.lines).1;
        data.spans.resize(data.lines.len(), None);
        data.changed.resize(data.lines.len(), None);
    }
    h.run_steps(2);
    let from = position(&h, "new_αlpha", 3);
    h.event(Event::PointerMoved(from));
    h.step();
    button(&mut h, from, true, Modifiers::NONE);
    h.event(Event::PointerMoved(egui::pos2(from.x, 300.0)));
    h.step();
    h.event(Event::MouseWheel {
        unit: egui::MouseWheelUnit::Point,
        delta: egui::vec2(0.0, -400.0),
        modifiers: Modifiers::NONE,
        phase: egui::TouchPhase::Move,
    });
    h.run_steps(30);
    let to = egui::pos2(from.x, position(&h, "vertical_26", 5).y);
    h.event(Event::PointerMoved(to));
    h.step();
    button(&mut h, to, false, Modifiers::NONE);
    let sel = h
        .state()
        .view
        .text_sel
        .unwrap_or_else(|| unreachable!("lost selection"));
    assert_eq!(sel.side, super::super::select::Side::New);
    assert_eq!(sel.anchor, (3, 3));
    assert!(sel.cursor.0 > sel.anchor.0);
    assert!(selected(&h).starts_with("_αlpha beta\nnew_second value\n"));
    assert!(!selected(&h).contains("old_"));
    assert!(h.state().view.chosen.is_empty());
}

/// The painted +/− target stays clickable while the code scrolls underneath it.
#[test]
fn horizontally_scrolled_git_markers_stay_visible_and_pick_expected_lines() {
    for split in [false, true] {
        let mut h = harness(split, false, true, false);
        let marker = position(&h, "old_αlpha", 0) - egui::vec2(super::super::MARK_W / 2.0, 0.0);
        let before = position(&h, "long_token", 29);
        h.event(Event::PointerMoved(before));
        h.step();
        h.event(Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            delta: egui::vec2(-160.0, 0.0),
            modifiers: Modifiers::NONE,
            phase: egui::TouchPhase::Move,
        });
        h.run_steps(12);
        assert!(position(&h, "long_token", 29).x < before.x - 100.0);
        // Check painting as well as the original, still-visible hit target.
        let painted_marker = position(&h, "−", 0);
        assert!((painted_marker.x - marker.x).abs() < 6.0);
        click(&mut h, marker, Modifiers::NONE);
        assert_eq!(h.state().view.chosen, [2].into());
        assert!(h.state().view.text_sel.is_none());
        click(&mut h, marker, Modifiers::NONE);
        assert!(h.state().view.chosen.is_empty());
    }
}
