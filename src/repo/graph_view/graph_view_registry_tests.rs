use super::*;
#[test]
fn optional_graph_copy_commands_use_actual_selected_commit() {
    use crate::keymap::{Command, Stroke};
    let ctx = egui::Context::default();
    let mut settings = Settings::default();
    let commands = [
        (Command::GraphCopyHash, Key::F8, "recorded-hash"),
        (Command::GraphCopyMessage, Key::F9, "recorded message"),
        (Command::GraphCopyBranch, Key::F10, "main"),
    ];
    for (command, key, _) in commands {
        settings.key_bindings.insert(
            command.id().into(),
            vec![Stroke::new(key, egui::Modifiers::NONE)],
        );
    }
    settings.put_for_test(&ctx);
    let mut tab = RepoTab::new_deferred("/recorded/graph-copy".into(), &ctx);
    tab.refs.local.push(git::Branch {
        name: "main".into(),
        target: "recorded-hash".into(),
        is_head: true,
        upstream: String::new(),
        track: String::new(),
    });
    tab.commits = vec![std::sync::Arc::new(git::Commit {
        hash: "recorded-hash".into(),
        summary: "recorded message".into(),
        refs: vec!["main".into()],
        ..Default::default()
    })];
    tab.selected = Some(0);
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
            |ui| tab.arrow_keys(ui, None, &mut acts),
        );
        assert!(acts.is_empty());
        assert!(
            out.platform_output
                .commands
                .iter()
                .any(|c| matches!(c, egui::OutputCommand::CopyText(t) if t == expected))
        );
        out.drop_without_applying_deltas();
    }
}

#[test]
fn configured_graph_movement_reaches_actual_selection_adapter() {
    use crate::keymap::{Command, Stroke};
    let ctx = egui::Context::default();
    let mut settings = Settings {
        graph_lane_walk: false,
        ..Default::default()
    };
    settings.key_bindings.insert(
        Command::GraphNext.id().into(),
        vec![Stroke::new(Key::F8, egui::Modifiers::NONE)],
    );
    settings.put_for_test(&ctx);
    let mut tab = RepoTab::new_deferred("/recorded/graph-registry".into(), &ctx);
    tab.commits = (0..3)
        .map(|i| {
            std::sync::Arc::new(git::Commit {
                hash: i.to_string(),
                ..Default::default()
            })
        })
        .collect();
    tab.selected = Some(0);
    for (key, expected) in [(Key::F8, true), (Key::ArrowDown, false)] {
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
            |ui| tab.arrow_keys(ui, None, &mut acts),
        )
        .drop_without_applying_deltas();
        assert_eq!(matches!(acts.as_slice(), [Action::Select(1)]), expected);
    }
}
