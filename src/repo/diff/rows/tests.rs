use super::*;

#[test]
fn actual_diff_map_publishes_scroll_range_and_applies_assistive_scroll() {
    use egui_kittest::{
        Harness,
        kittest::{NodeT, Queryable},
    };
    let view = DiffView::new(DiffKey::Commit {
        hash: "recorded".into(),
        path: "file.rs".into(),
    });
    let data = DiffData {
        lines: (1..=100)
            .map(|line| super::super::DiffLine {
                kind: Kind::Context,
                old: Some(line),
                new: Some(line),
                text: "unchanged".into(),
                raw: " unchanged".into(),
            })
            .collect(),
        spans: vec![None; 100],
        changed: vec![None; 100],
        ..Default::default()
    };
    let mut h = Harness::new_ui_state(
        move |ui, state: &mut (bool, f32)| {
            if !std::mem::replace(&mut state.0, true) {
                crate::theme::install_fonts(ui.ctx());
                return;
            }
            let mut rows = Rows::new(ui, &view, &data, None, false, false);
            rows.show(ui, "file.rs", &mut Vec::new());
            state.1 = rows.offset;
            crate::a11y::end_frame(ui.ctx());
        },
        (false, 0.0),
    );
    h.run_steps(2);
    let node = h.get_by_role(egui::accesskit::Role::Slider);
    let data = node.accesskit_node().data();
    assert_eq!(data.numeric_value(), Some(0.0));
    assert_eq!(data.min_numeric_value(), Some(0.0));
    assert!(data.max_numeric_value().is_some_and(|max| max > 100.0));
    assert_eq!(
        data.orientation(),
        Some(egui::accesskit::Orientation::Vertical)
    );
    let (target_node, target_tree) = node.accesskit_node().locate();
    h.event(egui::Event::AccessKitActionRequest(
        egui::accesskit::ActionRequest {
            action: egui::accesskit::Action::SetValue,
            target_node,
            target_tree,
            data: Some(egui::accesskit::ActionData::NumericValue(100.0)),
        },
    ));
    h.run();
    assert_eq!(h.state().1, 100.0);
    h.run_steps(2);
    assert_eq!(
        h.state().1,
        100.0,
        "actual ScrollArea state must retain the requested position"
    );
    h.key_press(egui::Key::Tab);
    h.run();
    assert!(
        h.get_by_role(egui::accesskit::Role::Slider).is_focused(),
        "the text drag hit shield must not take keyboard focus"
    );
}
#[test]
fn selection_registry_runs_actual_diff_rows_and_history_action() {
    use super::super::DiffLine;
    use crate::keymap::{self, Command, Scope, Stroke};
    use egui::{Event, Key, Modifiers};
    let ctx = egui::Context::default();
    crate::theme::install_fonts(&ctx);
    let mut settings = crate::settings::Settings::default();
    for (command, key) in [
        (Command::SelectionAll, Key::F8),
        (Command::SelectionCopy, Key::F9),
        (Command::SelectionHistory, Key::F10),
    ] {
        settings
            .key_bindings
            .insert(command.id().into(), vec![Stroke::new(key, Modifiers::NONE)]);
    }
    settings.put_for_test(&ctx);
    let mut view = DiffView::new(DiffKey::Commit {
        hash: "recorded".into(),
        path: "file.txt".into(),
    });
    let data = DiffData {
        lines: vec![
            DiffLine {
                kind: Kind::Context,
                old: Some(1),
                new: Some(1),
                text: "one".into(),
                raw: " one".into(),
            },
            DiffLine {
                kind: Kind::Add,
                old: None,
                new: Some(2),
                text: "two".into(),
                raw: "+two".into(),
            },
        ],
        spans: vec![None; 2],
        changed: vec![None; 2],
        ..Default::default()
    };
    for key in [None, Some(Key::F8), Some(Key::F9), Some(Key::F10)] {
        let mut acts = Vec::new();
        let out = ctx.run_ui(
            egui::RawInput {
                events: key
                    .into_iter()
                    .flat_map(|key| {
                        [true, false].map(|pressed| Event::Key {
                            key,
                            physical_key: None,
                            pressed,
                            repeat: false,
                            modifiers: Modifiers::NONE,
                        })
                    })
                    .collect(),
                ..Default::default()
            },
            |ui| {
                keymap::active(
                    &ctx,
                    vec![Scope::Window, Scope::Repository, Scope::Selection],
                );
                let mut rows = Rows::new(ui, &view, &data, None, false, false);
                let selection = rows.show(ui, "file.txt", &mut acts);
                drop(rows);
                view.text_sel = selection;
            },
        );
        if key == Some(Key::F8) {
            assert_eq!(
                view.text_sel
                    .map(|s| s.text(&data.lines, &data.uncut))
                    .as_deref(),
                Some("one\ntwo")
            );
        }
        if key == Some(Key::F9) {
            assert!(
                out.platform_output
                    .commands
                    .iter()
                    .any(|c| matches!(c, egui::OutputCommand::CopyText(t) if t == "one\ntwo"))
            );
        }
        if key == Some(Key::F10) {
            assert!(
                matches!(acts.as_slice(), [Action::LineHistory { path, at, range: (1, 2) }] if path == "file.txt" && *at == crate::git::LinesAt::Rev("recorded".into()))
            );
        }
        out.drop_without_applying_deltas();
    }
}

#[test]
fn change_map_keys_scroll_and_clamp_and_leave_modified_keys_alone() {
    use egui::{Key, Modifiers};
    for (key, modifiers, expected) in [
        (Key::ArrowDown, Modifiers::NONE, Some(100.0 + LINE_H)),
        (Key::PageUp, Modifiers::NONE, Some(0.0)),
        (Key::End, Modifiers::NONE, Some(800.0)),
        (Key::Home, Modifiers::NONE, Some(0.0)),
        (Key::ArrowDown, Modifiers::CTRL, None),
    ] {
        let ctx = egui::Context::default();
        ctx.run_ui(
            egui::RawInput {
                events: vec![egui::Event::Key {
                    key,
                    physical_key: Some(key),
                    pressed: true,
                    repeat: false,
                    modifiers,
                }],
                ..Default::default()
            },
            |ui| {
                assert_eq!(map_keyboard(ui, 100.0, 200.0, 1000.0), expected);
            },
        )
        .drop_without_applying_deltas();
    }
}

#[test]
fn diff_names_include_change_kind_both_coordinates_and_content() {
    let line = super::super::DiffLine {
        kind: Kind::Add,
        old: None,
        new: Some(42),
        text: "let 日本語 = 1;".to_owned(),
        raw: "+let 日本語 = 1;".to_owned(),
    };
    assert_eq!(
        line_label(&line),
        "added, old line none, new line 42: let 日本語 = 1;"
    );
}
