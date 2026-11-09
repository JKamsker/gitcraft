//! Inert input sequencing: real text edits and buttons, with recording-only actions.
use super::singleline_enter;
use egui::{Event, Key, Modifiers, PointerButton, Pos2, Rect};
use egui_kittest::Harness;

#[derive(Default)]
struct Form {
    text: String,
    field: Option<egui::Id>,
    browse_rect: Option<Rect>,
    submissions: usize,
    browses: usize,
    lost_focus_on_enter: bool,
    had_focus_on_enter: bool,
}

fn form(text: &str) -> Harness<'static, Form> {
    let mut h = Harness::builder().with_size([600.0, 300.0]).build_ui_state(
        |ui, form: &mut Form| {
            ui.set_width(460.0);
            ui.horizontal(|ui| {
                let field = ui.add(
                    egui::TextEdit::singleline(&mut form.text)
                        .desired_width(ui.available_width() - 90.0),
                );
                form.field = Some(field.id);
                if ui.input(|i| i.key_pressed(Key::Enter)) {
                    form.lost_focus_on_enter = field.lost_focus();
                    form.had_focus_on_enter = ui.memory(|m| m.had_focus_last_frame(field.id));
                }
                if singleline_enter(&field) {
                    form.submissions += 1;
                }
                let browse = ui.button("Browse");
                form.browse_rect = Some(browse.rect);
                if browse.clicked() {
                    form.browses += 1;
                }
            });
        },
        Form {
            text: text.into(),
            ..Default::default()
        },
    );
    h.ctx
        .options_mut(|o| o.max_passes = std::num::NonZeroUsize::MIN);
    h.run_steps(3);
    let field = h.state().field.expect("field rendered");
    h.ctx.memory_mut(|m| m.request_focus(field));
    h.step();
    assert!(h.ctx.memory(|m| m.has_focus(field)));
    h
}

fn enter() -> Event {
    Event::Key {
        key: Key::Enter,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: Modifiers::NONE,
    }
}

fn pointer(pos: Pos2, pressed: bool) -> Event {
    Event::PointerButton {
        pos,
        button: PointerButton::Primary,
        pressed,
        modifiers: Modifiers::NONE,
    }
}

#[test]
fn browse_pointer_activation_and_enter_never_submit_the_text_field() {
    for enter_first in [false, true] {
        let mut h = form("/inert");
        let pos = h.state().browse_rect.expect("Browse rendered").center();
        let mut events = vec![
            Event::PointerMoved(pos),
            pointer(pos, true),
            pointer(pos, false),
        ];
        if enter_first {
            events.insert(0, enter());
        } else {
            events.push(enter());
        }
        h.input_mut().events.extend(events);
        h.step();
        assert!(h.state().lost_focus_on_enter);
        assert_eq!(h.state().browses, 1);
        assert_eq!(h.state().submissions, 0);
    }
    // Complete a click in the Enter frame, or begin it there and release later.
    for enter_on_release in [false, true] {
        let mut h = form("/inert");
        let pos = h.state().browse_rect.expect("Browse rendered").center();
        h.input_mut()
            .events
            .extend([Event::PointerMoved(pos), pointer(pos, true)]);
        if !enter_on_release {
            h.input_mut().events.push(enter());
        }
        h.step();
        h.input_mut().events.push(pointer(pos, false));
        if enter_on_release {
            h.input_mut().events.push(enter());
        }
        h.step();
        assert_eq!(h.state().browses, 1);
        assert_eq!(h.state().submissions, 0);
    }
}

#[test]
fn stale_focus_loss_after_blank_click_does_not_submit() {
    let mut h = form("/inert");
    let blank = Pos2::new(550.0, 250.0);
    h.input_mut().events.extend([
        Event::PointerMoved(blank),
        pointer(blank, true),
        pointer(blank, false),
    ]);
    h.step();
    h.input_mut().events.push(enter());
    h.step();
    assert!(h.state().lost_focus_on_enter);
    assert!(!h.state().had_focus_on_enter);
    assert_eq!(h.state().submissions, 0);
    assert_eq!(h.state().browses, 0);
}

#[test]
fn ordinary_enter_still_submits_after_focus_surrender() {
    for text in ["", "/inert"] {
        let mut h = form(text);
        h.key_press(Key::Enter);
        h.step();
        assert!(h.state().lost_focus_on_enter);
        assert!(h.state().had_focus_on_enter);
        assert_eq!(h.state().submissions, 1);
        assert_eq!(h.state().browses, 0);
        assert!(h.ctx.memory(|m| m.focused().is_none()));
    }
}
