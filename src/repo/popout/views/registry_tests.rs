use super::*;
use crate::keymap::{self, Command, Scope, Stroke};

#[test]
fn child_prepares_selection_context_and_find_field_keeps_escape() {
    let ctx = egui::Context::default();
    let mut tab = RepoTab::new_deferred("/recorded/panel-selection".into(), &ctx);
    tab.diff = Some(crate::repo::DiffView::new(crate::repo::DiffKey::Commit {
        hash: "recorded".into(),
        path: "file".into(),
    }));
    let mut settings = crate::settings::Settings::default();
    settings.key_bindings.insert(
        Command::SelectionCopy.id().into(),
        vec![Stroke::new(Key::F8, egui::Modifiers::NONE)],
    );
    settings.put_for_test(&ctx);
    ctx.run_ui(
        egui::RawInput {
            events: vec![egui::Event::Key {
                key: Key::F8,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
            ..Default::default()
        },
        |_| {
            let mut acts = Vec::new();
            tab.popout_keys(&ctx, Panel::Diff, &mut acts);
            assert!(
                keymap::take(&ctx, Command::SelectionCopy),
                "view gets its configured command after child context setup"
            );
            assert!(acts.is_empty());
        },
    )
    .drop_without_applying_deltas();
    tab.find.show();
    ctx.run_ui(
        egui::RawInput {
            events: vec![egui::Event::Key {
                key: Key::Escape,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
            ..Default::default()
        },
        |ui| {
            // The production find field ID; a text primitive owns Escape before the diff.
            ui.add(
                egui::TextEdit::singleline(&mut String::new())
                    .id(egui::Id::new("find_in_view_field")),
            )
            .request_focus();
            let mut acts = Vec::new();
            tab.popout_keys(&ctx, Panel::Diff, &mut acts);
            assert!(
                acts.is_empty(),
                "Escape in the find field must not close the entire diff"
            );
        },
    )
    .drop_without_applying_deltas();
}

#[test]
fn actual_child_dispatch_uses_overrides_and_cancels_unrelated_commands() {
    let ctx = egui::Context::default();
    let mut tab = RepoTab::new_deferred("/recorded/panel-keys".into(), &ctx);
    tab.diff = Some(crate::repo::DiffView::new(crate::repo::DiffKey::Commit {
        hash: "recorded".into(),
        path: "file".into(),
    }));
    let mut settings = crate::settings::Settings::default();
    settings.key_bindings.insert(
        Command::Undo.id().into(),
        vec![Stroke::new(Key::F8, egui::Modifiers::NONE)],
    );
    settings.key_bindings.insert(
        Command::NextChange.id().into(),
        vec![Stroke::new(Key::F9, egui::Modifiers::NONE)],
    );
    settings.put_for_test(&ctx);
    for (key, expected) in [
        (Key::F8, Some(Command::Undo)),
        (Key::F9, Some(Command::NextChange)),
        (Key::F5, Some(Command::Reload)),
        (Key::F7, None),
    ] {
        let mut acts = Vec::new();
        ctx.run_ui(
            egui::RawInput {
                events: vec![
                    egui::Event::Key {
                        key,
                        physical_key: Some(key),
                        pressed: true,
                        repeat: false,
                        modifiers: egui::Modifiers::NONE,
                    },
                    egui::Event::Key {
                        key,
                        physical_key: Some(key),
                        pressed: false,
                        repeat: false,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
                ..Default::default()
            },
            |_| tab.popout_keys(&ctx, Panel::Diff, &mut acts),
        )
        .drop_without_applying_deltas();
        match expected {
            Some(Command::Undo) => assert!(matches!(acts.as_slice(), [Action::Undo])),
            Some(Command::Reload) => assert!(matches!(acts.as_slice(), [Action::Reload])),
            Some(command) => {
                assert!(matches!(acts.as_slice(), [Action::Key(found)] if *found == command))
            }
            None => assert!(acts.is_empty()),
        }
    }
    // A panel cannot consume a reconfigured window or remote-operation shortcut.
    assert!(!keymap::queued(&ctx, Command::Fetch));
    keymap::cancel(&ctx, Scope::Window);
}
