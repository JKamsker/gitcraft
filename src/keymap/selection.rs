//! Copy in painted line views. Text edits and the terminal keep their native copy protocol.
use egui::{Context, Event, Key, Modifiers};

use super::{Command, Stroke, config, take};

pub(crate) fn copy_requested(ctx: &Context) -> bool {
    if crate::a11y::keyboard_taken(ctx) {
        return false;
    }
    let configured = take(ctx, Command::SelectionCopy);
    let keys = config::effective(
        &config::usable(&crate::settings::Settings::get(ctx).key_bindings),
        Command::SelectionCopy,
    );
    // Winit can deliver Cmd/Ctrl+C as Copy without a Key event. Keep that default
    // path, but do not let it bypass an explicit rebind/unbind. A native Edit→Copy
    // action without modifiers remains a semantic copy request.
    let native = super::resolver::permitted(ctx, Command::SelectionCopy)
        && ctx.input(|i| {
            i.events.iter().any(|e| matches!(e, Event::Copy))
                && (i.modifiers.is_none() || keys == [Stroke::new(Key::C, Modifiers::COMMAND)])
        });
    configured || native
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keymap::{self, Scope};

    #[test]
    fn native_copy_obeys_rebinding_and_text_ownership() {
        let ctx = Context::default();
        let mut settings = crate::settings::Settings::default();
        let run = |mut events: Vec<Event>, modifiers, text_owned| {
            events.insert(0, Event::ModifiersChanged(modifiers));
            let mut copied = false;
            ctx.run_ui(
                egui::RawInput {
                    events,
                    ..Default::default()
                },
                |ui| {
                    keymap::active(
                        &ctx,
                        vec![Scope::Window, Scope::Repository, Scope::Selection],
                    );
                    if text_owned {
                        ui.text_edit_singleline(&mut String::new()).request_focus();
                    }
                    copied = copy_requested(&ctx);
                },
            )
            .drop_without_applying_deltas();
            copied
        };
        assert!(run(vec![Event::Copy], Modifiers::COMMAND, false));
        settings.key_bindings.insert(
            Command::SelectionCopy.id().into(),
            vec![Stroke::new(Key::F8, Modifiers::NONE)],
        );
        settings.put_for_test(&ctx);
        assert!(!run(vec![Event::Copy], Modifiers::COMMAND, false));
        assert!(
            run(vec![Event::Copy], Modifiers::NONE, false),
            "native Edit→Copy is semantic"
        );
        let event = Event::Key {
            key: Key::F8,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        };
        assert!(run(vec![event.clone()], Modifiers::NONE, false));
        assert!(!run(vec![event], Modifiers::NONE, true));
        ctx.memory_mut(|m| {
            if let Some(id) = m.focused() {
                m.surrender_focus(id);
            }
        });
        ctx.run_ui(
            egui::RawInput {
                events: vec![Event::Copy],
                ..Default::default()
            },
            |_| {
                keymap::restrict(&ctx, Some(Vec::new()));
                assert!(
                    !copy_requested(&ctx),
                    "native Copy must obey the modal command restriction"
                );
            },
        )
        .drop_without_applying_deltas();
    }
}
