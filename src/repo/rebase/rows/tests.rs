use super::super::Entry;
use super::*;
use crate::git::RebaseCommit;
use egui::Modifiers;

fn editor(hashes: &[&str]) -> RebaseEditor {
    let commits = hashes
        .iter()
        .map(|h| RebaseCommit {
            hash: (*h).to_owned(),
            summary: format!("summary {h}"),
            message: format!("summary {h}"),
            ..RebaseCommit::default()
        })
        .collect();
    RebaseEditor::new("x".into(), "x".into(), commits)
}

fn order(ed: &RebaseEditor) -> Vec<&str> {
    ed.entries.iter().map(|e| e.commit.hash.as_str()).collect()
}

fn steps(ed: &RebaseEditor) -> Vec<Step> {
    ed.entries.iter().map(|e: &Entry| e.step).collect()
}

#[test]
fn letters_set_the_selected_rows_action() {
    let mut ed = editor(&["a", "b", "c"]);
    // The newest row starts selected.
    assert_eq!(ed.selected, Some(2));
    ed.selected = None;
    // Nothing selected: letters do nothing.
    assert!(!ed.press(Key::D, false));
    assert_eq!(steps(&ed), [Step::Pick; 3]);
    ed.selected = Some(1);
    for (key, step) in [
        (Key::D, Step::Drop),
        (Key::S, Step::Squash),
        (Key::F, Step::Fixup),
        (Key::E, Step::Edit),
        (Key::P, Step::Pick),
    ] {
        assert!(!ed.press(key, false));
        assert_eq!(ed.entries[1].step, step);
    }
    // Rewording says so once, for the message to take the keyboard.
    assert!(ed.press(Key::R, false));
    assert!(!ed.press(Key::R, false));
    assert_eq!(ed.entries[1].step, Step::Reword);
    // Other keys, or letters with Alt, change nothing.
    assert!(!ed.press(Key::X, false));
    assert!(!ed.press(Key::D, true));
    assert_eq!(steps(&ed), [Step::Pick, Step::Reword, Step::Pick]);
}

#[test]
fn arrows_select_and_alt_arrows_move() {
    let mut ed = editor(&["a", "b", "c"]);
    ed.selected = None;
    // The newest (top) row first.
    ed.press(Key::ArrowDown, false);
    assert_eq!(ed.selected, Some(2));
    ed.press(Key::ArrowDown, false);
    ed.press(Key::ArrowDown, false);
    ed.press(Key::ArrowDown, false);
    assert_eq!(ed.selected, Some(0));
    ed.press(Key::ArrowUp, false);
    assert_eq!(ed.selected, Some(1));
    // Alt+↑ moves b up (later), the selection with it; at the top it stays.
    ed.press(Key::ArrowUp, true);
    assert_eq!(order(&ed), ["a", "c", "b"]);
    assert_eq!(ed.selected, Some(2));
    ed.press(Key::ArrowUp, true);
    assert_eq!(order(&ed), ["a", "c", "b"]);
    ed.press(Key::ArrowDown, true);
    ed.press(Key::ArrowDown, true);
    ed.press(Key::ArrowDown, true);
    assert_eq!(order(&ed), ["b", "a", "c"]);
    assert_eq!(ed.selected, Some(0));
}

#[test]
fn nothing_changes_while_the_rebase_is_being_started() {
    let mut ed = editor(&["a", "b"]);
    ed.selected = Some(0);
    ed.starting = true;
    ed.press(Key::D, false);
    ed.press(Key::ArrowUp, true);
    assert_eq!(order(&ed), ["a", "b"]);
    assert_eq!(steps(&ed), [Step::Pick; 2]);
    assert!(!editor(&[]).press(Key::ArrowUp, false));
}

#[test]
fn dropping_on_a_row_puts_it_above_or_below() {
    // Oldest first: a b c d, shown d c b a.
    assert_eq!(drop_index(0, 3, true), Some(3)); // a above d: newest
    assert_eq!(drop_index(0, 3, false), Some(2)); // a below d
    assert_eq!(drop_index(3, 0, false), Some(0)); // d below a: oldest
    assert_eq!(drop_index(3, 0, true), Some(1)); // d above a
    assert_eq!(drop_index(1, 2, false), None); // b below c: where it is
    assert_eq!(drop_index(2, 1, true), None); // c above b: where it is
    assert_eq!(drop_index(1, 1, true), None);
    assert_eq!(drop_index(1, 1, false), None);
    assert_eq!(drop_index(1, 2, true), Some(2));
}

#[test]
fn a_dragged_row_lands_where_dropped_and_stays_selected() {
    let mut ed = editor(&["a", "b", "c", "d"]);
    ed.selected = Some(2);
    let to = drop_index(0, 3, true).unwrap_or_default();
    ed.move_entry(0, to);
    assert_eq!(order(&ed), ["b", "c", "d", "a"]);
    // c moved down with the others.
    assert_eq!(ed.selected, Some(1));
    ed.selected = Some(3);
    ed.move_entry(3, 0);
    assert_eq!(order(&ed), ["a", "b", "c", "d"]);
    assert_eq!(ed.selected, Some(0));
    // Out of range: nothing.
    ed.move_entry(0, 9);
    assert_eq!(order(&ed), ["a", "b", "c", "d"]);
}

#[test]
fn the_row_under_the_pointer() {
    let rect = |top: f32| Rect::from_min_max(egui::pos2(0.0, top), egui::pos2(10.0, top + 20.0));
    // Drawn newest first: entry 2 at the top.
    let rows = [(2, rect(0.0)), (1, rect(20.0)), (0, rect(40.0))];
    assert_eq!(row_at(&rows, 5.0), Some((2, true)));
    assert_eq!(row_at(&rows, 15.0), Some((2, false)));
    assert_eq!(row_at(&rows, 31.0), Some((1, false)));
    assert_eq!(row_at(&rows, -50.0), Some((2, true)));
    assert_eq!(row_at(&rows, 500.0), Some((0, false)));
    assert_eq!(row_at(&[], 5.0), None);
}

/// Draws the list one frame with `events`, as the editor does.
fn frame(ctx: &egui::Context, ed: &mut RebaseEditor, events: Vec<egui::Event>) {
    crate::theme::install_fonts(ctx);
    let input = egui::RawInput {
        screen_rect: Some(Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(800.0, 600.0),
        )),
        events,
        ..Default::default()
    };
    let mut out = ctx.run_ui(input, |ui| {
        egui::CentralPanel::default().show(ui, |ui| {
            let focus = ed.keys(ui, true);
            ed.list(ui, focus);
        });
    });
    out.textures_delta.clear();
}

fn key(key: Key, modifiers: Modifiers) -> egui::Event {
    egui::Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers,
    }
}

#[test]
fn configured_commands_reach_actual_rebase_keys_and_step_hints() {
    use crate::keymap::{Command, Stroke};
    let ctx = egui::Context::default();
    let mut settings = crate::settings::Settings::default();
    settings.key_bindings.insert(
        Command::RebaseDrop.id().into(),
        vec![Stroke::new(Key::F8, Modifiers::NONE)],
    );
    settings.put_for_test(&ctx);
    let mut ed = editor(&["a", "b"]);
    frame(&ctx, &mut ed, vec![]);
    frame(&ctx, &mut ed, vec![key(Key::F8, Modifiers::NONE)]);
    assert_eq!(ed.entries[1].step, Step::Drop);
    assert!(super::super::keys_hint(&ctx).contains("Drop: F8"));
    ed.entries[1].step = Step::Pick;
    frame(&ctx, &mut ed, vec![key(Key::D, Modifiers::NONE)]);
    assert_eq!(
        ed.entries[1].step,
        Step::Pick,
        "replaced default must not continue executing"
    );
}
#[test]
fn keys_pressed_over_the_list_act_on_it() {
    let ctx = egui::Context::default();
    let mut ed = editor(&["a", "b", "c"]);
    ed.selected = None;
    frame(&ctx, &mut ed, vec![]);
    frame(
        &ctx,
        &mut ed,
        vec![
            key(Key::ArrowDown, Modifiers::NONE),
            key(Key::F, Modifiers::NONE),
            key(Key::ArrowDown, Modifiers::ALT),
        ],
    );
    assert_eq!(order(&ed), ["a", "c", "b"]);
    assert_eq!(ed.entries[1].step, Step::Fixup);
    // With Ctrl held a letter is someone else's shortcut.
    frame(&ctx, &mut ed, vec![key(Key::D, Modifiers::COMMAND)]);
    assert_eq!(ed.entries[1].step, Step::Fixup);
}

#[test]
fn letting_go_of_a_dragged_row_above_the_list_makes_it_the_newest() {
    let ctx = egui::Context::default();
    let mut ed = editor(&["a", "b", "c"]);
    frame(&ctx, &mut ed, vec![]);
    egui::DragAndDrop::set_payload(
        &ctx,
        DraggedRow {
            editor: ed.id,
            index: 0,
        },
    );
    let at = egui::pos2(300.0, 1.0);
    frame(
        &ctx,
        &mut ed,
        vec![
            egui::Event::PointerMoved(at),
            egui::Event::PointerButton {
                pos: at,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: Modifiers::NONE,
            },
        ],
    );
    assert_eq!(order(&ed), ["b", "c", "a"]);
    assert_eq!(ed.selected, Some(2));
    // A row of another editor is not taken.
    egui::DragAndDrop::set_payload(
        &ctx,
        DraggedRow {
            editor: ed.id + 1,
            index: 0,
        },
    );
    frame(
        &ctx,
        &mut ed,
        vec![egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: Modifiers::NONE,
        }],
    );
    assert_eq!(order(&ed), ["b", "c", "a"]);
}

#[test]
fn a_row_let_go_outside_the_list_stays_where_it_was() {
    let ctx = egui::Context::default();
    let mut ed = editor(&["a", "b", "c"]);
    frame(&ctx, &mut ed, vec![]);
    egui::DragAndDrop::set_payload(
        &ctx,
        DraggedRow {
            editor: ed.id,
            index: 0,
        },
    );
    let at = egui::pos2(950.0, 1.0);
    frame(
        &ctx,
        &mut ed,
        vec![
            egui::Event::PointerMoved(at),
            egui::Event::PointerButton {
                pos: at,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: Modifiers::NONE,
            },
        ],
    );
    assert_eq!(order(&ed), ["a", "b", "c"]);
    assert!(!egui::DragAndDrop::has_any_payload(&ctx));
}

#[test]
fn rows_out_of_sight_are_only_measured_once_and_the_selection_is_followed() {
    let ctx = egui::Context::default();
    let names: Vec<String> = (0..400).map(|i| format!("c{i}")).collect();
    let mut ed = editor(&names.iter().map(String::as_str).collect::<Vec<_>>());
    frame(&ctx, &mut ed, vec![]);
    frame(&ctx, &mut ed, vec![]);
    assert_eq!(ed.heights.len(), 400);
    // Walking down past the window still moves the selected row, drawn where it is.
    let downs = (0..350)
        .map(|_| key(Key::ArrowDown, Modifiers::NONE))
        .collect();
    frame(&ctx, &mut ed, downs);
    frame(&ctx, &mut ed, vec![key(Key::D, Modifiers::NONE)]);
    assert_eq!(ed.selected, Some(49));
    assert_eq!(ed.entries[49].step, Step::Drop);
}

#[test]
fn every_action_has_its_key() {
    for step in Step::ALL {
        let key = Key::from_name(&step.key().to_string());
        assert_eq!(key.and_then(Step::for_key), Some(step));
    }
}
