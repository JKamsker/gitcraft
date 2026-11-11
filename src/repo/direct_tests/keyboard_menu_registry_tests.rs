use super::super::file_view::FileTab;
use super::*;
use crate::keymap::Stroke;
#[test]
fn optional_file_commands_capture_actual_reconciled_path_and_revision() {
    let ctx = egui::Context::default();
    let mut tab = RepoTab::new_deferred("/recorded/file-commands".into(), &ctx);
    tab.loading = false;
    tab.keyboard_scope();
    let scope = Id::new("recorded-file-list");
    let target = DiffKey::Range {
        from: "old".into(),
        to: Some("recorded-revision".into()),
        path: "日本語.txt".into(),
    };
    tab.keyboard.borrow_mut().pane = Some(Pane::Detail);
    tab.keyboard.borrow_mut().cursor = Some((scope, target.clone()));
    let mut settings = Settings::default();
    let commands = [
        (Command::FileHistory, Key::F8, Some(FileTab::History)),
        (Command::FileBlame, Key::F9, Some(FileTab::Blame)),
        (Command::FileContents, Key::F10, Some(FileTab::File)),
        (Command::FileCopyPath, Key::F11, None),
    ];
    for (command, key, _) in commands {
        settings.key_bindings.insert(
            command.id().into(),
            vec![Stroke::new(key, egui::Modifiers::NONE)],
        );
    }
    settings.put_for_test(&ctx);
    for (_, key, expected) in commands {
        let mut acts = Vec::new();
        let out = ctx.run_ui(
            egui::RawInput {
                events: vec![egui::Event::Key {
                    key,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                }],
                ..Default::default()
            },
            |ui| {
                keymap::active(&ctx, tab.key_scopes(&ctx));
                tab.keyboard_files(
                    ui,
                    (scope, None),
                    std::slice::from_ref(&target),
                    |key| key.path(),
                    Clone::clone,
                    &mut acts,
                );
            },
        );
        if let Some(expected) = expected {
            assert!(
                matches!(acts.as_slice(), [Action::FileView { path, rev, tab }] if path == "日本語.txt" && rev.as_deref() == Some("recorded-revision") && *tab == expected)
            );
        } else {
            assert!(acts.is_empty());
            assert!(
                out.platform_output
                    .commands
                    .iter()
                    .any(|c| matches!(c, egui::OutputCommand::CopyText(t) if t == "日本語.txt"))
            );
        }
        out.drop_without_applying_deltas();
    }
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
        |ui| {
            keymap::active(&ctx, tab.key_scopes(&ctx));
            let mut acts = Vec::new();
            tab.keyboard_files(
                ui,
                (scope, None),
                &[] as &[DiffKey],
                |key| key.path(),
                Clone::clone,
                &mut acts,
            );
            assert!(
                acts.is_empty(),
                "a vanished selection must not open another file"
            );
        },
    )
    .drop_without_applying_deltas();
}
