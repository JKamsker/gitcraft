//! Which configuration wins: each setting of the allowlist with the value in effect and where
//! it comes from; opened, every value git read for it, the one that wins on top, removing one
//! from its file, setting it at a chosen scope, and what it is in the other worktrees.

use egui::{RichText, Ui};

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
    faint(
        ui,
        "Git reads the system file, then your global one, then this repository's and this \
         worktree's, then values given on the command line; for a setting with one value, the \
         last value read wins. With a gitcraft profile, the global file is the profile's: it \
         reads ~/.gitconfig first, then its own values on top (git run outside gitcraft \
         doesn't see it). Values from include or includeIf files show under their own file. \
         Only these settings are shown: nothing credential-related.",
    );
    let Some(data) = reading(ui, &state.origins).cloned() else {
        return;
    };
    env.worktree_config = data.worktree_config;
    let all = data.origins;
    ui.add_space(10.0);
    ui.horizontal(|ui| {
        ui.add(
            egui::TextEdit::singleline(&mut state.filter)
                .hint_text("Filter settings")
                .desired_width(220.0),
        );
        ui.checkbox(&mut state.show_unset, "Show settings nobody set");
    });
    let filter = state.filter.trim().to_lowercase();
    let mut group = "";
    for setting in SETTINGS {
        let values = config::values_of(&all, setting.key);
        if (values.is_empty() && !state.show_unset)
            || (!filter.is_empty()
                && !setting.key.to_lowercase().contains(&filter)
                && !setting.about.to_lowercase().contains(&filter))
        {
            continue;
        }
        if setting.group != group {
            group = setting.group;
            group_heading(ui, group);
        }
        setting_row(ui, state, setting, &values, &env, acts);
    }
    if group.is_empty() {
        ui.add_space(12.0);
        faint(ui, "No setting here matches.");
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
                            RichText::new(format!("overrides {}", values.len() - 1))
                                .size(11.5)
                                .color(t.warning),
                        );
                    }
                }
                None => {
                    ui.label(RichText::new("not set: git's default").color(t.text_faint));
                }
            }
        },
    );
    let resp = resp
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .on_hover_text(setting.about);
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

/// `value` cut to fit a row: its first line, at most [`ROW_VALUE`] characters.
fn shortened(value: &str) -> String {
    let first_line = value.lines().next().unwrap_or_default();
    if first_line.chars().count() > ROW_VALUE || first_line.len() < value.len() {
        let cut: String = first_line.chars().take(ROW_VALUE).collect();
        format!("{cut}…")
    } else {
        value.to_owned()
    }
}

/// Where a value comes from, as the row's chip says it.
fn origin_label(o: &Origin) -> String {
    if o.file.as_deref().is_some_and(is_profile_file) {
        return "gitcraft profile".to_owned();
    }
    o.scope.label().to_owned()
}

/// Whether git named the active profile's config file (see `settings::profiles`): written
/// over by gitcraft, so never changed here.
fn is_profile_file(file: &str) -> bool {
    let Some(profile) = crate::git::profile_config() else {
        return false;
    };
    let norm = |s: &str| {
        let s = s.replace('\\', "/");
        if cfg!(windows) { s.to_lowercase() } else { s }
    };
    norm(file) == norm(&profile.to_string_lossy())
}

fn details(
    ui: &mut Ui,
    state: &mut ConfigState,
    setting: &'static Setting,
    values: &[&Origin],
    env: &Env,
    acts: &mut Vec<Action>,
) {
    card(ui, setting.key, Some(setting.about), |ui| {
        if values.is_empty() {
            faint(ui, "No file sets it: git uses its built-in default.");
        }
        // The one in effect first, then those it overrides.
        for (n, origin) in values.iter().rev().enumerate() {
            value_row(ui, origin, n == 0, env.locked, acts);
        }
        ui.add_space(10.0);
        ui.separator();
        set_row(ui, state, setting, env, acts);
        if env.worktrees > 1 {
            ui.add_space(8.0);
            other_worktrees(ui, state, setting.key, acts);
        }
    });
}

/// One value git read: the value, its scope and file, whether it is in effect, and removing it.
fn value_row(ui: &mut Ui, origin: &Origin, wins: bool, locked: bool, acts: &mut Vec<Action>) {
    let t = Tokens::get(ui.ctx());
    ui.horizontal_wrapped(|ui| {
        if wins {
            widgets::chip(ui, "In effect", t.success);
        } else {
            widgets::chip(ui, "Overridden", t.text_faint);
        }
        let value = code(origin.shown_value());
        ui.label(if wins {
            value.color(t.text)
        } else {
            value.color(t.text_faint).strikethrough()
        });
        ui.label(RichText::new(origin_label(origin)).color(t.text_dim));
        if let Some(file) = &origin.file {
            ui.label(code(file).color(t.text_faint));
        }
        let profile = origin.file.as_deref().is_some_and(is_profile_file);
        let why = match () {
            _ if origin.scope == Scope::Command => {
                Some("Given on the command line or in GIT_CONFIG_PARAMETERS: not in a file")
            }
            _ if profile => {
                Some("gitcraft writes this file from Settings → Profile: change it there")
            }
            _ => None,
        };
        let args = config::unset_args(origin);
        let why = why.or(args.as_ref().err().map(String::as_str));
        let remove = ui
            .add_enabled(why.is_none() && !locked, egui::Button::new("Remove"))
            .on_hover_text(format!(
                "Remove every {} from this file; the next value down (or git's default) then \
                 applies",
                origin.key
            ))
            .on_disabled_hover_text(why.unwrap_or("Wait for the running operation"));
        if remove.clicked()
            && let Ok(op) = &args
        {
            acts.push(remove_confirm(origin, op.clone()));
        }
    });
}

/// The question asked before removing `origin` from its file.
fn remove_confirm(origin: &Origin, op: Vec<String>) -> Action {
    let file = origin.file.clone().unwrap_or_default();
    let reach = match origin.scope {
        Scope::Global => " This applies to all your repositories.",
        Scope::System => " This applies to every user of this machine.",
        _ => "",
    };
    Action::Doctor(DoctorAct::Confirm(Confirm {
        title: format!("Remove {}?", origin.key),
        body: format!(
            "Remove {} = {} from {file}?{reach} The value isn't kept: to get it back, set it \
             again.",
            origin.key,
            origin.shown_value()
        ),
        button: "Remove".to_owned(),
        label: format!(
            "Remove {} ({}) from {file}",
            origin.key,
            shortened(origin.shown_value())
        ),
        ops: vec![op],
    }))
}

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
        ui.label("Set it for");
        egui::ComboBox::from_id_salt(("doctor-scope", setting.key))
            .selected_text(scope_choice(scope))
            .show_ui(ui, |ui| {
                for s in Scope::WRITABLE {
                    let off = s == Scope::Worktree && !env.worktree_config;
                    ui.add_enabled_ui(!off, |ui| {
                        ui.selectable_value(&mut state.scope, Some(s), scope_choice(s))
                            .on_disabled_hover_text(
                                "Needs extensions.worktreeConfig = true in this repository: \
                                 without it git refuses when there are other worktrees, and \
                                 with only one writes .git/config",
                            );
                    });
                }
            });
        ui.label("to");
        if setting.choices.is_empty() {
            ui.add(egui::TextEdit::singleline(&mut state.value).desired_width(220.0));
        } else {
            egui::ComboBox::from_id_salt(("doctor-value", setting.key))
                .selected_text(if state.value.is_empty() {
                    "choose"
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
            .add_enabled(enabled, egui::Button::new("Set"))
            .on_disabled_hover_text(match &args {
                Err(e) => e.as_str(),
                Ok(_) if locked => "Wait for the running operation",
                Ok(_) if setting.choices.is_empty() => "Type a value first",
                Ok(_) => "Choose a value first",
            });
        if set.clicked()
            && let Ok(op) = args
        {
            let label = format!(
                "Set {} to {} for {}",
                setting.key,
                shortened(&value),
                scope_choice(scope).to_lowercase()
            );
            acts.push(Action::Doctor(match scope {
                // Beyond this repository: asked first.
                Scope::Global | Scope::System => DoctorAct::Confirm(Confirm {
                    title: format!(
                        "Set {} for {}?",
                        setting.key,
                        scope_choice(scope).to_lowercase()
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
                }),
                _ => DoctorAct::Run {
                    label,
                    ops: vec![op],
                },
            }));
        }
    });
    let note = match scope {
        Scope::Global if env.remote => {
            "Your global git config on that machine (usually ~/.gitconfig), for all your \
             repositories there."
        }
        Scope::Global => {
            "Your global git config (usually ~/.gitconfig), for all your repositories. A \
             gitcraft profile that sets the same thing still wins over it."
        }
        Scope::System => {
            "Git's system file, for every user of this machine; usually needs administrator \
             rights."
        }
        Scope::Worktree => "Only this worktree (its config.worktree file).",
        _ => {
            "Only this repository (.git/config). An include later in that file that sets it \
             still wins."
        }
    };
    faint(ui, note);
}

fn scope_choice(scope: Scope) -> &'static str {
    match scope {
        Scope::Local => "This repository",
        Scope::Worktree => "This worktree",
        Scope::Global => "All my repositories",
        Scope::System => "Everyone on this machine",
        Scope::Command => "Command line",
    }
}

/// What the setting is in each worktree, once asked for.
fn other_worktrees(ui: &mut Ui, state: &ConfigState, key: &'static str, acts: &mut Vec<Action>) {
    let t = Tokens::get(ui.ctx());
    let shown = state.worktrees.as_ref().filter(|(k, _)| *k == key);
    let Some((_, read)) = shown else {
        if ui.button("Compare with the other worktrees").clicked() {
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
                Ok(None) => dim(ui, "not set"),
                Err(e) => {
                    ui.label(RichText::new(e).color(t.danger));
                }
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn long_and_multiline_values_are_cut_in_rows() {
        assert_eq!(shortened("true"), "true");
        assert_eq!(shortened("a\nb"), "a…");
        let long = "é".repeat(60);
        assert_eq!(shortened(&long).chars().count(), ROW_VALUE + 1);
        assert_eq!(shortened(""), "");
    }
}
