//! Transactional keyboard editor: capture actual key strokes, validate, then Save or Cancel.
use super::{Command, Overrides, Stroke, config};
use egui::{Event, Id, Key, Ui};

#[derive(Clone)]
struct Editing {
    command: Command,
    sequence: Vec<Stroke>,
    focus_capture: bool,
    capture_id: Option<Id>,
    last_seen: u64,
}

pub(crate) fn capturing(ctx: &egui::Context) -> bool {
    let key = Id::new(("key-binding-editor", ctx.viewport_id()));
    ctx.data(|d| d.get_temp::<Editing>(key))
        .is_some_and(|edit| ctx.cumulative_pass_nr() <= edit.last_seen.saturating_add(1))
}

pub(crate) fn page(overrides: &mut Overrides, ui: &mut Ui) {
    ui.label(crate::i18n::text("interaction-control-select-change-press-up-to-four-keys-in-the-capture-control-then-tab-to-save"));
    if let Err(error) = config::validate(overrides) {
        ui.colored_label(
            crate::theme::Tokens::get(ui.ctx()).danger,
            crate::i18n::message("interaction-key-invalid-saved", &[("detail", error.into())]),
        );
    }
    let edit_id = Id::new(("key-binding-editor", ui.ctx().viewport_id()));
    let mut editing: Option<Editing> = ui.ctx().data(|d| d.get_temp(edit_id));
    if let Some(edit) = &mut editing {
        super::cancel(ui.ctx(), super::Scope::Window);
        super::cancel(ui.ctx(), super::Scope::Repository);
        // Only the capture control records strokes. Save, Clear and Cancel keep their
        // normal keyboard activation, and Tab/Shift+Tab can reach them.
        let recording = edit.focus_capture
            || edit
                .capture_id
                .is_some_and(|id| ui.memory(|m| m.has_focus(id)));
        let events = ui.input(|i| i.events.clone());
        let mut cancelled = false;
        for event in events {
            if let Event::Key {
                key,
                modifiers,
                pressed: true,
                repeat: false,
                ..
            } = event
            {
                if key == Key::Escape {
                    cancelled = true;
                } else if recording && !navigation_tab(key, modifiers) && edit.sequence.len() < 4 {
                    edit.sequence.push(Stroke::new(key, modifiers));
                }
            }
        }
        ui.input_mut(|i| {
            i.events.retain(|event| match event {
                Event::Key {
                    key: Key::Escape, ..
                } => false,
                Event::Key { key, modifiers, .. } if recording => navigation_tab(*key, *modifiers),
                Event::Text(_) | Event::Ime(_) if recording => false,
                _ => true,
            })
        });
        let mut candidate = overrides.clone();
        candidate.insert(edit.command.id().into(), edit.sequence.clone());
        let validation = config::validate(&candidate);
        ui.group(|ui| {
            ui.label(edit.command.title());
            let sequence = edit
                .sequence
                .iter()
                .map(Stroke::label)
                .collect::<Vec<_>>()
                .join(" → ");
            let capture = ui.button(crate::i18n::message(
                "interaction-key-capture",
                &[(
                    "keys",
                    if sequence.is_empty() {
                        crate::i18n::text("interaction-control-no-keys")
                    } else {
                        &sequence
                    }
                    .into(),
                )],
            ));
            edit.capture_id = Some(capture.id);
            if edit.focus_capture || capture.clicked() {
                capture.request_focus();
                edit.focus_capture = false;
            }
            if let Err(error) = &validation {
                ui.label(error);
            }
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(
                        validation.is_ok(),
                        egui::Button::new(crate::i18n::text("interaction-control-save-binding")),
                    )
                    .clicked()
                {
                    *overrides = candidate;
                    cancelled = true;
                }
                if ui
                    .button(crate::i18n::text("interaction-control-clear-captured-keys"))
                    .clicked()
                {
                    edit.sequence.clear();
                }
                if ui
                    .button(crate::i18n::text("interaction-control-cancel"))
                    .clicked()
                {
                    cancelled = true;
                }
            });
            ui.horizontal(|ui| {
                for (label, modifiers) in [
                    (
                        crate::i18n::text("interaction-control-add-tab-key"),
                        egui::Modifiers::NONE,
                    ),
                    (
                        crate::i18n::text("interaction-control-add-shift-tab-key"),
                        egui::Modifiers::SHIFT,
                    ),
                ] {
                    if ui
                        .add_enabled(edit.sequence.len() < 4, egui::Button::new(label))
                        .clicked()
                    {
                        edit.sequence.push(Stroke::new(Key::Tab, modifiers));
                    }
                }
            });
        });
        if cancelled {
            if let Some(id) = edit.capture_id {
                ui.memory_mut(|m| m.surrender_focus(id));
            }
            editing = None;
        }
    }
    if ui
        .button(crate::i18n::text("interaction-control-reset-all-shortcuts"))
        .clicked()
    {
        overrides.clear();
        editing = None;
    }
    egui::Grid::new("editable-bindings")
        .striped(true)
        .num_columns(5)
        .show(ui, |ui| {
            for command in config::commands() {
                ui.push_id(command.id(), |ui| {
                    ui.label(command.title());
                    let keys = config::effective(&config::usable(overrides), command)
                        .iter()
                        .map(Stroke::label)
                        .collect::<Vec<_>>()
                        .join(" → ");
                    ui.monospace(if keys.is_empty() {
                        crate::i18n::text("interaction-control-unbound")
                    } else {
                        &keys
                    });
                    if ui
                        .button(crate::i18n::text("interaction-control-change"))
                        .clicked()
                    {
                        editing = Some(Editing {
                            command,
                            sequence: Vec::new(),
                            focus_capture: true,
                            capture_id: None,
                            last_seen: ui.ctx().cumulative_pass_nr(),
                        });
                    }
                    if ui
                        .button(crate::i18n::text("interaction-control-unbind"))
                        .clicked()
                    {
                        overrides.insert(command.id().into(), Vec::new());
                    }
                    if ui
                        .button(crate::i18n::text("interaction-control-reset"))
                        .clicked()
                    {
                        overrides.remove(command.id());
                    }
                    ui.end_row();
                });
            }
        });
    let pass = ui.ctx().cumulative_pass_nr();
    ui.ctx().data_mut(|d| {
        if let Some(mut edit) = editing {
            edit.last_seen = pass;
            d.insert_temp(edit_id, edit);
        } else {
            d.remove::<Editing>(edit_id);
        }
    });
}

fn navigation_tab(key: Key, modifiers: egui::Modifiers) -> bool {
    key == Key::Tab && !modifiers.command && !modifiers.ctrl && !modifiers.alt
}

#[cfg(test)]
#[allow(clippy::expect_used)] // Failed test fixtures must fail the gate, not use fallback data.
mod tests {
    use super::*;
    use egui::Modifiers;
    use egui_kittest::{
        Harness,
        kittest::{NodeT, Queryable},
    };

    fn harness() -> Harness<'static, Overrides> {
        Harness::new_ui_state(|ui, overrides| page(overrides, ui), Overrides::default())
    }

    fn change(h: &mut Harness<'_, Overrides>) {
        h.get_all_by_label("Change")
            .next()
            .expect("first action")
            .click();
        h.run();
        assert!(capturing(&h.ctx));
    }

    fn change_by_keyboard(h: &mut Harness<'_, Overrides>) {
        h.key_press(Key::Tab);
        h.run();
        assert!(h.get_by_label("Reset all shortcuts").is_focused());
        h.key_press(Key::Tab);
        h.run();
        assert!(
            h.get_all_by_label("Change")
                .next()
                .expect("first action")
                .is_focused()
        );
        h.key_press(Key::Enter);
        h.run();
        assert!(capturing(&h.ctx));
    }

    #[test]
    fn change_capture_survives_frames_and_saves_the_actual_key() {
        let mut h = harness();
        h.run();
        change(&mut h);
        h.key_press(Key::F8);
        h.run();
        h.run_steps(3);
        h.get_by_label("Capture shortcut: F8");
        h.get_by_label("Save binding").click();
        h.run();
        assert_eq!(
            h.state().get(Command::NewTab.id()),
            Some(&vec![Stroke::new(Key::F8, Modifiers::NONE)])
        );
        assert!(h.query_by_label("Save binding").is_none());
        assert!(!capturing(&h.ctx));
        // Exercise persisted editor output through the production resolver and label,
        // rather than supplying a separate programmatic override fixture.
        let persisted = serde_json::to_string(h.state()).expect("serialize edited binding");
        let ctx = egui::Context::default();
        let settings = crate::settings::Settings {
            key_bindings: serde_json::from_str(&persisted).expect("restore edited binding"),
            ..Default::default()
        };
        settings.put_for_test(&ctx);
        assert_eq!(super::super::label(&ctx, Command::NewTab), "F8");
        ctx.run_ui(
            egui::RawInput {
                events: vec![Event::Key {
                    key: Key::F8,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: Modifiers::NONE,
                }],
                ..Default::default()
            },
            |_| {
                super::super::active(&ctx, vec![super::super::Scope::Window]);
                assert_eq!(
                    super::super::pressed(&ctx, super::super::Scope::Window),
                    vec![Command::NewTab]
                );
            },
        )
        .drop_without_applying_deltas();
    }

    #[test]
    fn tab_to_save_and_enter_do_not_become_part_of_the_binding() {
        let mut h = harness();
        h.run();
        change_by_keyboard(&mut h);
        h.key_press(Key::F8);
        h.run();
        h.key_press(Key::Tab);
        h.run();
        assert!(h.get_by_label("Save binding").is_focused());
        h.key_press(Key::Enter);
        h.run();
        assert_eq!(
            h.state().get(Command::NewTab.id()),
            Some(&vec![Stroke::new(Key::F8, Modifiers::NONE)])
        );
    }

    #[test]
    fn tab_to_cancel_and_space_leave_saved_bindings_unchanged() {
        let mut h = harness();
        h.run();
        change_by_keyboard(&mut h);
        h.key_press(Key::F8);
        h.run();
        for _ in 0..3 {
            h.key_press(Key::Tab);
            h.run();
        }
        assert!(h.get_by_label("Cancel").is_focused());
        h.key_press(Key::Space);
        h.run();
        assert!(h.state().is_empty());
        assert!(h.query_by_label("Save binding").is_none());
    }

    #[test]
    fn escape_cancels_from_an_action_button_and_tab_can_be_added_explicitly() {
        let mut h = harness();
        h.run();
        change(&mut h);
        h.key_press(Key::Tab);
        h.run();
        assert!(h.get_by_label("Save binding").is_focused());
        h.key_press(Key::Escape);
        h.run();
        assert!(h.state().is_empty());
        assert!(h.query_by_label("Save binding").is_none());
        change(&mut h);
        h.get_by_label("Add Tab key").click();
        h.run();
        h.get_by_label("Capture shortcut: Tab");
        h.get_by_label("Save binding").click();
        h.run();
        assert_eq!(
            h.state().get(Command::NewTab.id()),
            Some(&vec![Stroke::new(Key::Tab, Modifiers::NONE)])
        );
    }

    #[test]
    fn invalid_candidate_cannot_save_and_clear_is_keyboard_accessible() {
        let mut h = harness();
        h.run();
        change(&mut h);
        h.key_press_modifiers(Modifiers::COMMAND, Key::W);
        h.run();
        assert!(
            h.get_by_label("Save binding")
                .accesskit_node()
                .is_disabled()
        );
        h.key_press(Key::Tab);
        h.run();
        assert!(h.get_by_label("Clear captured keys").is_focused());
        h.key_press(Key::Enter);
        h.run();
        h.get_by_label("Capture shortcut: No keys");
        h.get_by_label("Save binding").click();
        h.run();
        assert_eq!(h.state().get(Command::NewTab.id()), Some(&vec![]));
    }

    #[test]
    fn capture_bounds_chords_ignores_repeats_and_is_window_scoped() {
        let mut h = harness();
        h.run();
        change(&mut h);
        for key in [Key::F8, Key::F9, Key::F10, Key::F11, Key::F12] {
            h.key_press(key);
            h.run();
        }
        h.event(Event::Key {
            key: Key::F8,
            physical_key: None,
            pressed: true,
            repeat: true,
            modifiers: Modifiers::NONE,
        });
        h.run();
        h.get_by_label("Capture shortcut: F8 → F9 → F10 → F11");
        let other = Id::new((
            "key-binding-editor",
            egui::ViewportId::from_hash_of("other-window"),
        ));
        assert!(h.ctx.data(|d| d.get_temp::<Editing>(other)).is_none());
        h.get_by_label("Save binding").click();
        h.run();
        assert_eq!(
            h.state().get(Command::NewTab.id()),
            Some(
                &[Key::F8, Key::F9, Key::F10, Key::F11]
                    .map(|key| Stroke::new(key, Modifiers::NONE))
                    .to_vec()
            )
        );
        assert!(!capturing(&h.ctx));
    }

    #[test]
    fn an_editor_that_is_no_longer_drawn_releases_shortcut_ownership() {
        let mut h = harness();
        h.run();
        change(&mut h);
        for _ in 0..2 {
            h.ctx
                .run_ui(egui::RawInput::default(), |_| {})
                .drop_without_applying_deltas();
        }
        assert!(!capturing(&h.ctx));
    }
}

#[cfg(test)]
mod localization_tests {
    use super::*;
    use egui_kittest::{Harness, kittest::Queryable};

    #[test]
    fn german_editor_saves_real_binding_with_stable_action_ids_and_numbered_tab_keys()
    -> Result<(), String> {
        let ctx = egui::Context::default();
        let before = crate::keymap::label(&ctx, Command::GoToTab(1));
        crate::i18n::set_language("de");
        assert_eq!(Command::GoToTab(1).id(), "tab_1");
        assert_eq!(crate::keymap::label(&ctx, Command::GoToTab(1)), before);
        assert_eq!(Command::NewTab.title(), "Neuer Tab");
        let mut h =
            Harness::new_ui_state(|ui, overrides| page(overrides, ui), Overrides::default());
        h.get_all_by_label("Ändern")
            .next()
            .ok_or("missing Change control")?
            .click();
        h.run();
        h.key_press(Key::F8);
        h.run();
        h.get_by_label("Tastenkürzel aufnehmen: F8");
        h.get_by_label("Belegung speichern").click();
        h.run();
        assert_eq!(
            h.state().get("new_tab"),
            Some(&vec![Stroke::new(Key::F8, egui::Modifiers::NONE)])
        );
        let json = serde_json::to_string(h.state()).map_err(|e| e.to_string())?;
        assert!(json.contains("new_tab"));
        assert!(!json.contains("Neuer Tab"));
        Ok(())
    }
}
