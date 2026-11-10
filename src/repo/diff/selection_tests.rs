//! Real pointer events against the diff's code and Git-action hit zones.
use super::*;
use crate::repo::pr_review::PrAct;
use egui::{Event, Modifiers, PointerButton, Pos2};
use egui_kittest::Harness;

struct State {
    view: DiffView,
    split: bool,
    partial: bool,
    installed: bool,
    acts: Vec<Action>,
    review_local: Option<bool>,
}

fn harness(split: bool, wrap: bool, partial: bool, new_file: bool) -> Harness<'static, State> {
    let text = if new_file {
        "@@ -0,0 +1,2 @@\n+new_αlpha beta\n+second value\n".to_owned()
    } else {
        format!(
            "@@ -1,4 +1,4 @@\n context value\n-old_αlpha beta\n+new_αlpha beta\n-old_second value\n+new_second value\n {} tail\n",
            "long_token ".repeat(50)
        )
    };
    let lines = super::super::parse(&text);
    let data = DiffData {
        split: super::super::parse::pair_lines(&lines).1,
        spans: vec![None; lines.len()],
        changed: vec![None; lines.len()],
        longest: lines
            .iter()
            .map(|l| l.text.chars().count())
            .max()
            .unwrap_or(0),
        lines,
        new_file,
        ..Default::default()
    };
    let mut view = DiffView::new(DiffKey::Commit {
        hash: "recorded".into(),
        path: "file.txt".into(),
    });
    view.data = Some(Ok(data));
    let mut h = Harness::builder()
        .with_size([740.0, 380.0])
        .with_step_dt(1.0 / 60.0)
        .with_max_steps(30)
        .build_ui_state(
            move |ui, state: &mut State| {
                if !state.installed {
                    crate::theme::install_fonts(ui.ctx());
                    egui_extras::install_image_loaders(ui.ctx());
                    let mut settings = crate::settings::Settings::default();
                    settings.diff_wrap = wrap;
                    settings.key_bindings.insert(
                        crate::keymap::Command::SelectionHistory.id().into(),
                        vec![crate::keymap::Stroke::new(egui::Key::F8, Modifiers::NONE)],
                    );
                    settings.put_for_test(ui.ctx());
                    state.installed = true;
                    return;
                }
                crate::keymap::active(ui.ctx(), vec![crate::keymap::Scope::Selection]);
                let Some(Ok(data)) = &state.view.data else {
                    return;
                };
                let review = state
                    .review_local
                    .map(|local| crate::repo::pr_review::DiffReview {
                        marks: Default::default(),
                        target: None,
                        path: "file.txt".into(),
                        local,
                    });
                let mut rows = Rows::new(
                    ui,
                    &state.view,
                    data,
                    review.as_ref(),
                    state.partial,
                    data.shows_split(state.split),
                );
                let sel = rows.show(ui, "file.txt", &mut state.acts);
                let (clicked, started) = (rows.clicked, rows.text_started);
                drop(rows);
                state.view.text_sel = sel;
                if started {
                    state.view.chosen.clear();
                    state.view.anchor = None;
                }
                if let Some(i) = clicked {
                    state.view.pick_line(i, ui.input(|i| i.modifiers.shift));
                }
            },
            State {
                view,
                split,
                partial,
                installed: false,
                acts: Vec::new(),
                review_local: None,
            },
        );
    h.run_steps(3);
    h
}

fn position(h: &Harness<'_, State>, prefix: &str, char_index: usize) -> Pos2 {
    fn find(shape: &egui::Shape, prefix: &str, c: usize) -> Option<Pos2> {
        match shape {
            egui::Shape::Text(t) if t.galley.text().starts_with(prefix) => {
                let rect = t.galley.pos_from_cursor(egui::text::CCursor::new(c));
                Some(t.pos + rect.left_center().to_vec2() + egui::vec2(0.1, 0.0))
            }
            egui::Shape::Vec(shapes) => shapes.iter().find_map(|s| find(s, prefix, c)),
            _ => None,
        }
    }
    h.output()
        .shapes
        .iter()
        .find_map(|s| find(&s.shape, prefix, char_index))
        .unwrap_or_else(|| unreachable!("line {prefix:?} is not painted"))
}

fn button(h: &mut Harness<'_, State>, pos: Pos2, pressed: bool, modifiers: Modifiers) {
    h.event(Event::ModifiersChanged(modifiers));
    h.event(Event::PointerButton {
        pos,
        button: PointerButton::Primary,
        pressed,
        modifiers,
    });
    h.step();
}

fn click(h: &mut Harness<'_, State>, pos: Pos2, modifiers: Modifiers) {
    h.event(Event::PointerMoved(pos));
    h.step();
    button(h, pos, true, modifiers);
    button(h, pos, false, modifiers);
}

fn selected(h: &Harness<'_, State>) -> String {
    let Some(Ok(data)) = &h.state().view.data else {
        return String::new();
    };
    h.state()
        .view
        .text_sel
        .map(|s| s.text(&data.lines, &data.uncut))
        .unwrap_or_default()
}

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
