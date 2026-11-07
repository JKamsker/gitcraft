//! Transactional keyboard editor: capture actual key strokes, validate, then Save or Cancel.
use super::{Command, Overrides, Stroke, config};
use egui::{Event, Id, Key, Ui};

#[derive(Clone)]
struct Editing {
    command: Command,
    sequence: Vec<Stroke>,
}

pub(crate) fn capturing(ctx: &egui::Context) -> bool {
    ctx.data(|d| d.get_temp::<Option<Editing>>(Id::new(("key-binding-editor", ctx.viewport_id()))))
        .flatten()
        .is_some()
}

pub(crate) fn page(overrides: &mut Overrides, ui: &mut Ui) {
    ui.label("Select Change, press up to four keys, then Save. Empty bindings disable an action. Escape cancels capture.");
    if let Err(error) = config::validate(overrides) {
        ui.colored_label(
            crate::theme::Tokens::get(ui.ctx()).danger,
            format!("Saved bindings are invalid; defaults are active: {error}"),
        );
    }
    let edit_id = Id::new(("key-binding-editor", ui.ctx().viewport_id()));
    let mut editing: Option<Editing> = ui.ctx().data(|d| d.get_temp(edit_id));
    if let Some(edit) = &mut editing {
        super::cancel(ui.ctx(), super::Scope::Window);
        super::cancel(ui.ctx(), super::Scope::Repository);
        // Capture owns keyboard input, even though there is no text field.
        ui.memory_mut(|m| m.request_focus(edit_id));
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
                } else if edit.sequence.len() < 4 {
                    edit.sequence.push(Stroke::new(key, modifiers));
                }
            }
        }
        ui.input_mut(|i| {
            i.events
                .retain(|e| !matches!(e, Event::Key { .. } | Event::Text(_)))
        });
        let mut candidate = overrides.clone();
        candidate.insert(edit.command.id().into(), edit.sequence.clone());
        let validation = config::validate(&candidate);
        ui.group(|ui| {
            ui.label(edit.command.title());
            ui.monospace(
                edit.sequence
                    .iter()
                    .map(Stroke::label)
                    .collect::<Vec<_>>()
                    .join(" → "),
            );
            if let Err(error) = &validation {
                ui.label(error);
            }
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(validation.is_ok(), egui::Button::new("Save binding"))
                    .clicked()
                {
                    *overrides = candidate;
                    cancelled = true;
                }
                if ui.button("Clear captured keys").clicked() {
                    edit.sequence.clear();
                }
                if ui.button("Cancel").clicked() {
                    cancelled = true;
                }
            });
        });
        if cancelled {
            ui.memory_mut(|m| m.surrender_focus(edit_id));
            editing = None;
        }
    }
    if ui.button("Reset all shortcuts").clicked() {
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
                    ui.monospace(if keys.is_empty() { "Unbound" } else { &keys });
                    if ui.button("Change").clicked() {
                        editing = Some(Editing {
                            command,
                            sequence: Vec::new(),
                        });
                        ui.memory_mut(|m| m.request_focus(edit_id));
                    }
                    if ui.button("Unbind").clicked() {
                        overrides.insert(command.id().into(), Vec::new());
                    }
                    if ui.button("Reset").clicked() {
                        overrides.remove(command.id());
                    }
                    ui.end_row();
                });
            }
        });
    ui.ctx().data_mut(|d| d.insert_temp(edit_id, editing));
}
