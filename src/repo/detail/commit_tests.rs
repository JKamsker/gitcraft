use super::*;

/// What has the keyboard when a key is pressed with the reword editor open.
#[derive(Clone, Copy, PartialEq)]
enum Focus {
    Nothing,
    /// The terminal, drawn after the editor, which keeps Escape for the shell.
    Terminal,
    /// A plain text field drawn before the editor (the search box).
    FieldBefore,
    /// A plain text field drawn after it (the commit box).
    FieldAfter,
    /// The editor's own summary.
    Summary,
}

/// What the editor answers to Escape (or Ctrl+Enter when `save`) pressed while `focus` has
/// the keyboard.
fn press(focus: Focus, save: bool) -> Option<bool> {
    let ctx = egui::Context::default();
    crate::theme::install_fonts(&ctx);
    let mut reword = Reword {
        hash: "a".repeat(40),
        subject: "Fix the login".to_owned(),
        body: "A long explanation being typed".to_owned(),
        focus: focus == Focus::Summary,
        fill: false,
    };
    let terminal_id = egui::Id::new("terminal");
    let (key, modifiers) = if save {
        (Key::Enter, egui::Modifiers::COMMAND)
    } else {
        (Key::Escape, egui::Modifiers::NONE)
    };
    let event = egui::Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers,
    };
    let mut other = "something typed".to_owned();
    let mut out = None;
    for (pass, events) in [
        Vec::new(),
        Vec::new(),
        vec![egui::Event::ModifiersChanged(modifiers), event],
    ]
    .into_iter()
    .enumerate()
    {
        let input = egui::RawInput {
            events,
            ..Default::default()
        };
        ctx.run_ui(input, |ui| {
            let mut field = |ui: &mut Ui| {
                let r = ui.text_edit_singleline(&mut other);
                if pass == 0 {
                    r.request_focus();
                }
            };
            if focus == Focus::FieldBefore {
                field(ui);
            }
            out = reword_editor(ui, &mut reword);
            if focus == Focus::FieldAfter {
                field(ui);
            }
            let (_, rect) = ui.allocate_space(vec2(200.0, 100.0));
            let terminal = ui.interact(rect, terminal_id, Sense::click());
            if focus == Focus::Terminal {
                terminal.request_focus();
                let keep = egui::EventFilter {
                    escape: true,
                    ..Default::default()
                };
                ui.memory_mut(|m| m.set_focus_lock_filter(terminal_id, keep));
            }
        })
        .drop_without_applying_deltas();
    }
    out
}

/// Escape in the terminal (vim, the shell) or in another field is theirs: the message being
/// reworded isn't thrown away. With nothing else holding the keyboard it cancels.
#[test]
fn escape_elsewhere_does_not_cancel_a_reword() {
    assert_eq!(press(Focus::Terminal, false), None);
    // egui takes the focus from a text field for an Escape before the editor is drawn.
    assert_eq!(press(Focus::FieldBefore, false), None);
    assert_eq!(press(Focus::FieldAfter, false), None);
    assert_eq!(press(Focus::Nothing, false), Some(false));
    assert_eq!(press(Focus::Summary, false), Some(false));
}

/// Nor does Ctrl+Enter there save it.
#[test]
fn ctrl_enter_elsewhere_does_not_save_a_reword() {
    assert_eq!(press(Focus::Terminal, true), None);
    assert_eq!(press(Focus::FieldBefore, true), None);
    assert_eq!(press(Focus::FieldAfter, true), None);
    assert_eq!(press(Focus::Nothing, true), Some(true));
    assert_eq!(press(Focus::Summary, true), Some(true));
}
