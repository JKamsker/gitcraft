//! Real pointer events against the diff's code and Git-action hit zones.
use super::*;
use egui::{Event, Modifiers, PointerButton, Pos2};
use egui_kittest::Harness;

struct State {
    view: DiffView,
    split: bool,
    partial: bool,
    installed: bool,
    acts: Vec<Action>,
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
    let mut h = Harness::builder().with_size([740.0, 380.0]).build_ui_state(
        move |ui, state: &mut State| {
            if !state.installed {
                crate::theme::install_fonts(ui.ctx());
                egui_extras::install_image_loaders(ui.ctx());
                let mut settings = crate::settings::Settings::default();
                settings.diff_wrap = wrap;
                settings.put_for_test(ui.ctx());
                state.installed = true;
                return;
            }
            crate::keymap::active(ui.ctx(), vec![crate::keymap::Scope::Selection]);
            let Some(Ok(data)) = &state.view.data else {
                return;
            };
            let mut rows = Rows::new(
                ui,
                &state.view,
                data,
                None,
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
