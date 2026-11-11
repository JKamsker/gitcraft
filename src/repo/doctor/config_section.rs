//! Which configuration wins: each setting of the allowlist with the value in effect and where
//! it comes from; opened, every value git read for it, the one that wins on top, removing one
//! from its file, setting it at a chosen scope, and what it is in the other worktrees.

#[path = "config_section/values.rs"]
mod values;
use egui::{RichText, Ui};
use values::*;

use super::page::{card, code, dim, faint, reading};
use super::{ConfigState, Confirm, DoctorAct};
use crate::git::doctor::config::{self, Origin, SETTINGS, Scope, Setting};
use crate::repo::{Action, widgets};
use crate::theme::Tokens;

/// What the rows need to know about the tab.
pub(super) struct Env {
    /// Worktrees the setting can be compared across.
    pub worktrees: usize,
    /// The repository is in WSL or over SSH, where no gitcraft profile applies.
    pub remote: bool,
    /// An operation is running.
    pub locked: bool,
    /// `extensions.worktreeConfig` is on.
    worktree_config: bool,
}

impl Env {
    pub fn new(worktrees: usize, remote: bool, locked: bool) -> Self {
        Self {
            worktrees,
            remote,
            locked,
            worktree_config: false,
        }
    }
}

pub(super) fn ui(ui: &mut Ui, state: &mut ConfigState, mut env: Env, acts: &mut Vec<Action>) {
    faint(ui, crate::i18n::text("repo-workflows-doctor-ui-1f9897b7"));
    let Some(data) = reading(ui, &state.origins).cloned() else {
        return;
    };
    env.worktree_config = data.worktree_config;
    let all = data.origins;
    ui.add_space(10.0);
    ui.horizontal(|ui| {
        ui.add(
            egui::TextEdit::singleline(&mut state.filter)
                .hint_text(crate::i18n::text("repo-workflows-doctor-ui-4e47a723"))
                .desired_width(220.0),
        );
        ui.checkbox(
            &mut state.show_unset,
            crate::i18n::text("repo-workflows-doctor-ui-089db75c"),
        );
    });
    let filter = state.filter.trim().to_lowercase();
    let mut group = "";
    for setting in SETTINGS {
        let values = config::values_of(&all, setting.key);
        if (values.is_empty() && !state.show_unset)
            || (!filter.is_empty()
                && !setting.key.to_lowercase().contains(&filter)
                && !setting.about_label().to_lowercase().contains(&filter))
        {
            continue;
        }
        if setting.group != group {
            group = setting.group;
            group_heading(ui, setting.group_label());
        }
        setting_row(ui, state, setting, &values, &env, acts);
    }
    if group.is_empty() {
        ui.add_space(12.0);
        faint(ui, crate::i18n::text("repo-workflows-doctor-ui-2bfe98f3"));
    }
}

/// The heading above each group of settings.
fn group_heading(ui: &mut Ui, group: &str) {
    let t = Tokens::get(ui.ctx());
    ui.add_space(14.0);
    ui.label(
        RichText::new(group.to_uppercase())
            .font(crate::theme::semibold(10.5))
            .color(t.text_faint),
    );
    ui.add_space(2.0);
}

/// One setting: its key, the value in effect and where it is from; opened, its details.
fn setting_row(
    ui: &mut Ui,
    state: &mut ConfigState,
    setting: &'static Setting,
    values: &[&Origin],
    env: &Env,
    acts: &mut Vec<Action>,
) {
    let t = Tokens::get(ui.ctx());
    let open = state.open == Some(setting.key);
    let winner = values.last();
    // The whole row takes the click; what is drawn on it senses none.
    let (rect, resp) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 26.0), egui::Sense::click());
    crate::a11y::expanded(
        &resp,
        || {
            format!(
                "{}: {}. {}",
                setting.key,
                winner.map(|o| o.shown_value()).unwrap_or(crate::i18n::text(
                    "repo-workflows-doctor-setting-row-8ffd7cc3"
                )),
                setting.about_label()
            )
        },
        open,
    );
    if resp.hovered() {
        ui.painter().rect_filled(rect, t.radius, t.hover);
    }
    ui.scope_builder(
        egui::UiBuilder::new()
            .max_rect(rect)
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
        |ui| {
            ui.style_mut().interaction.selectable_labels = false;
            ui.add_space(4.0);
            let arrow = if open {
                "chevron-down"
            } else {
                "chevron-right"
            };
            crate::icons::inline(ui, arrow, 14.0, t.text_faint);
            ui.label(code(setting.key).color(t.text));
            match winner {
                Some(o) => {
                    ui.label(code(&shortened(o.shown_value())).color(t.accent));
                    widgets::chip(ui, &origin_label(o), t.text_dim);
                    if values.len() > 1 {
                        ui.label(
                            RichText::new(crate::i18n::message(
                                "repo-workflows-doctor-setting-row-8f8c7445",
                                &[("values-len-1-1", (values.len() - 1).into())],
                            ))
                            .size(11.5)
                            .color(t.warning),
                        );
                    }
                }
                None => {
                    ui.label(
                        RichText::new(crate::i18n::text(
                            "repo-workflows-doctor-setting-row-2224c772",
                        ))
                        .color(t.text_faint),
                    );
                }
            }
        },
    );
    let resp = resp
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .on_hover_text(setting.about_label());
    if resp.clicked() {
        state.open = if open { None } else { Some(setting.key) };
        state.scope = None;
        state.value = winner
            .map(|o| o.shown_value().to_owned())
            .unwrap_or_default();
    }
    if open {
        details(ui, state, setting, values, env, acts);
    }
}

/// Longest value a row shows whole; the opened setting shows it all.
const ROW_VALUE: usize = 36;

/// Setting a new value at a chosen scope.
fn set_row(
    ui: &mut Ui,
    state: &mut ConfigState,
    setting: &'static Setting,
    env: &Env,
    acts: &mut Vec<Action>,
) {
    let locked = env.locked;
    let scope = *state.scope.get_or_insert(Scope::Local);
    ui.horizontal_wrapped(|ui| {
        ui.label(crate::i18n::text("repo-workflows-doctor-set-row-85b9cd3d"));
        egui::ComboBox::from_id_salt(("doctor-scope", setting.key))
            .selected_text(scope_choice(scope))
            .show_ui(ui, |ui| {
                for s in Scope::WRITABLE {
                    let off = s == Scope::Worktree && !env.worktree_config;
                    ui.add_enabled_ui(!off, |ui| {
                        ui.selectable_value(&mut state.scope, Some(s), scope_choice(s))
                            .on_disabled_hover_text(crate::i18n::text(
                                "repo-workflows-doctor-set-row-6ec46474",
                            ));
                    });
                }
            });
        ui.label(crate::i18n::text("repo-workflows-doctor-set-row-663ea1bf"));
        if setting.choices.is_empty() {
            ui.add(egui::TextEdit::singleline(&mut state.value).desired_width(220.0));
        } else {
            egui::ComboBox::from_id_salt(("doctor-value", setting.key))
                .selected_text(if state.value.is_empty() {
                    crate::i18n::text("repo-workflows-r1-choose")
                } else {
                    state.value.as_str()
                })
                .show_ui(ui, |ui| {
                    for choice in setting.choices {
                        ui.selectable_value(&mut state.value, (*choice).to_owned(), *choice);
                    }
                });
        }
        let value = state.value.trim().to_owned();
        let args = config::set_args(scope, setting.key, &value);
        let enabled = !locked && !value.is_empty() && args.is_ok();
        let set = ui
            .add_enabled(
                enabled,
                egui::Button::new(crate::i18n::text("repo-workflows-doctor-set-row-b6f6f3ad")),
            )
            .on_disabled_hover_text(match &args {
                Err(e) => e.as_str(),
                Ok(_) if locked => crate::i18n::text("repo-workflows-doctor-set-row-b5c82673"),
                Ok(_) if setting.choices.is_empty() => {
                    crate::i18n::text("repo-workflows-doctor-set-row-46f01635")
                }
                Ok(_) => crate::i18n::text("repo-workflows-doctor-set-row-30a28bf1"),
            });
        if set.clicked()
            && let Ok(op) = args
        {
            let label = format!(
                "Set {} to {} for {}",
                setting.key,
                shortened(&value),
                scope_identity(scope).to_lowercase()
            );
            acts.push(Action::Doctor(match scope {
                // Beyond this repository: asked first.
                Scope::Global | Scope::System => DoctorAct::Confirm(
                    Confirm {
                        title: format!(
                            "Set {} for {}?",
                            setting.key,
                            scope_identity(scope).to_lowercase()
                        ),
                        body: format!(
                            "{} = {value} goes into {}. The value it replaces there isn't kept.",
                            setting.key,
                            if scope == Scope::Global {
                                "your global git config, for all your repositories"
                            } else {
                                "git's system file, for every user of this machine"
                            }
                        ),
                        button: "Set".to_owned(),
                        label,
                        ops: vec![op],
                        presentation: None,
                    }
                    .with_presentation(super::ConfirmKind::Set {
                        key: setting.key.into(),
                        value: value.clone(),
                        scope,
                    }),
                ),
                _ => DoctorAct::Run {
                    label,
                    ops: vec![op],
                },
            }));
        }
    });
    let note = match scope {
        Scope::Global if env.remote => crate::i18n::text("repo-workflows-doctor-set-row-2b7c9578"),
        Scope::Global => crate::i18n::text("repo-workflows-doctor-set-row-ad30af45"),
        Scope::System => crate::i18n::text("repo-workflows-doctor-set-row-3da50e6f"),
        Scope::Worktree => crate::i18n::text("repo-workflows-doctor-set-row-cadb5751"),
        _ => crate::i18n::text("repo-workflows-doctor-set-row-e0d5ce0a"),
    };
    faint(ui, note);
}

// Operation labels retain their original raw identity; scope_choice is paint only.
fn scope_identity(scope: Scope) -> &'static str {
    match scope {
        Scope::Local => "This repository",
        Scope::Worktree => "This worktree",
        Scope::Global => "All my repositories",
        Scope::System => "Everyone on this machine",
        Scope::Command => "Command line",
    }
}

pub(super) fn scope_choice(scope: Scope) -> &'static str {
    match scope {
        Scope::Local => crate::i18n::text("repo-workflows-doctor-scope-choice-38429e22"),
        Scope::Worktree => crate::i18n::text("repo-workflows-doctor-scope-choice-0eb26325"),
        Scope::Global => crate::i18n::text("repo-workflows-doctor-scope-choice-59b8031f"),
        Scope::System => crate::i18n::text("repo-workflows-doctor-scope-choice-ad944dcb"),
        Scope::Command => crate::i18n::text("repo-workflows-doctor-scope-choice-c925a118"),
    }
}

/// What the setting is in each worktree, once asked for.
fn other_worktrees(ui: &mut Ui, state: &ConfigState, key: &'static str, acts: &mut Vec<Action>) {
    let t = Tokens::get(ui.ctx());
    let shown = state.worktrees.as_ref().filter(|(k, _)| *k == key);
    let Some((_, read)) = shown else {
        if ui
            .button(crate::i18n::text(
                "repo-workflows-doctor-other-worktrees-965a2b09",
            ))
            .clicked()
        {
            acts.push(Action::Doctor(DoctorAct::CompareWorktrees(key)));
        }
        return;
    };
    let Some(list) = reading(ui, read) else {
        return;
    };
    for (dir, found) in list {
        ui.horizontal_wrapped(|ui| {
            ui.label(code(&crate::remote::display(dir)).color(t.text_dim));
            match found {
                Ok(Some(o)) => {
                    ui.label(code(o.shown_value()).color(t.text));
                    ui.label(RichText::new(origin_label(o)).color(t.text_faint));
                }
                Ok(None) => dim(
                    ui,
                    crate::i18n::text("repo-workflows-doctor-other-worktrees-1aef9399"),
                ),
                Err(e) => {
                    ui.label(RichText::new(e).color(t.danger));
                }
            }
        });
    }
}

#[cfg(test)]
#[path = "config_section/tests.rs"]
mod tests;

#[cfg(test)]
#[path = "config_section/accessibility_tests.rs"]
mod accessibility_tests;

#[cfg(test)]
#[path = "config_section/fluent_r1_confirmation_tests.rs"]
mod fluent_r1_confirmation_tests;
