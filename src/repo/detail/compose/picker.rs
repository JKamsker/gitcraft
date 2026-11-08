//! The type / scope picker: writes a Conventional Commits prefix (`feat(ui)!: `) or a Gitmoji
//! in front of the summary, in place of the one it had.

use egui::{RichText, Ui};

use super::prefix::{self, Conventional, GITMOJI, TYPES};
use super::store::{PrefixStyle, Store};
use crate::icons;
use crate::repo::RepoTab;
use crate::theme::Tokens;

/// The scope and "breaking" typed in the picker, kept while it is open.
#[derive(Clone, Debug, Default)]
pub(super) struct PickerState {
    scope: String,
    breaking: bool,
    /// The summary these were read from, so that they follow what was typed meanwhile.
    read_from: Option<String>,
}

impl RepoTab {
    pub(super) fn picker_button(&mut self, ui: &mut Ui) {
        let style = Store::style(ui.ctx());
        if style == PrefixStyle::Off {
            return;
        }
        let tip = match style {
            PrefixStyle::Gitmoji => "Gitmoji",
            _ => crate::i18n::text("repo-core-0981"),
        };
        let b = icons::small_button(ui, "tag", tip);
        let used_scopes = self.compose.used.as_ref().map(|(_, s)| s.clone());
        let summary = &mut self.commit_summary;
        let state = &mut self.compose.picker;
        egui::Popup::menu(&b)
            .width(300.0)
            .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
            .show(|ui| match style {
                PrefixStyle::Gitmoji => gitmoji(ui, summary),
                _ => conventional(ui, summary, state, used_scopes.as_deref().unwrap_or(&[])),
            });
    }
}

/// The types, a scope field (with the scopes used before) and "breaking change".
fn conventional(ui: &mut Ui, summary: &mut String, state: &mut PickerState, used: &[String]) {
    let t = Tokens::get(ui.ctx());
    let current = prefix::parse_conventional(summary).map(|(c, _)| c);
    if state.read_from.as_deref() != Some(summary.as_str()) {
        let c = current.clone().unwrap_or_default();
        state.scope = c.scope;
        state.breaking = c.breaking;
        state.read_from = Some(summary.clone());
    }
    let kind = current.as_ref().map(|c| c.kind.clone()).unwrap_or_default();
    ui.label(RichText::new(crate::i18n::text("repo-core-0982")).color(t.text_faint).small());
    egui::Grid::new("conventional-types")
        .num_columns(2)
        .spacing([8.0, 2.0])
        .show(ui, |ui| {
            for (name, what) in TYPES {
                if ui.selectable_label(kind == *name, *name).clicked() {
                    write(summary, name, state);
                }
                ui.label(RichText::new(crate::i18n::text(what)).color(t.text_dim).small());
                ui.end_row();
            }
        });
    ui.separator();
    ui.horizontal(|ui| {
        ui.label(crate::i18n::text("repo-core-0984"));
        let field = ui.add(
            egui::TextEdit::singleline(&mut state.scope)
                .hint_text(crate::i18n::text("repo-core-0985"))
                .desired_width(150.0),
        );
        if field.changed() && !kind.is_empty() {
            write(summary, &kind, state);
        }
    });
    if !used.is_empty() {
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new(crate::i18n::text("repo-core-0986")).color(t.text_faint).small());
            for scope in used.iter().take(12) {
                if ui.small_button(scope).clicked() {
                    state.scope = scope.clone();
                    if !kind.is_empty() {
                        write(summary, &kind, state);
                    }
                }
            }
        });
    }
    let breaking = ui
        .checkbox(&mut state.breaking, crate::i18n::text("repo-core-0987"))
        .on_hover_text(crate::i18n::text("repo-core-0988"));
    if breaking.changed() && !kind.is_empty() {
        write(summary, &kind, state);
    }
    if !kind.is_empty() && ui.button(crate::i18n::text("repo-core-0989")).clicked() {
        write(summary, "", state);
    }
}

/// Writes the prefix `kind` with the picker's scope and "breaking" in front of `summary`.
fn write(summary: &mut String, kind: &str, state: &mut PickerState) {
    let prefix = Conventional {
        kind: kind.to_owned(),
        scope: state.scope.clone(),
        breaking: state.breaking,
    };
    *summary = prefix::with_conventional(summary, &prefix);
    state.read_from = Some(summary.clone());
}

/// The Gitmoji, each with what it is for.
fn gitmoji(ui: &mut Ui, summary: &mut String) {
    let t = Tokens::get(ui.ctx());
    let current = prefix::parse_gitmoji(summary).map(|(e, _)| e);
    egui::ScrollArea::vertical()
        .max_height(320.0)
        .show(ui, |ui| {
            egui::Grid::new("gitmoji")
                .num_columns(2)
                .spacing([8.0, 2.0])
                .show(ui, |ui| {
                    for (emoji, what) in GITMOJI {
                        if ui
                            .selectable_label(
                                current == Some(*emoji),
                                RichText::new(*emoji).size(16.0),
                            )
                            .clicked()
                        {
                            *summary = prefix::with_gitmoji(summary, emoji);
                            ui.close();
                        }
                        ui.label(RichText::new(crate::i18n::text(what)).color(t.text_dim).small());
                        ui.end_row();
                    }
                });
        });
}
