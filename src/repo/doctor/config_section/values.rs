//! Configuration origin disclosure and value removal confirmation.

use super::*;

/// `value` cut to fit a row: its first line, at most [`ROW_VALUE`] characters.
pub(super) fn shortened(value: &str) -> String {
    let first_line = value.lines().next().unwrap_or_default();
    if first_line.chars().count() > ROW_VALUE || first_line.len() < value.len() {
        let cut: String = first_line.chars().take(ROW_VALUE).collect();
        format!("{cut}…")
    } else {
        value.to_owned()
    }
}

/// Where a value comes from, as the row's chip says it.
pub(super) fn origin_label(o: &Origin) -> String {
    if o.file.as_deref().is_some_and(is_profile_file) {
        return crate::i18n::text("repo-workflows-doctor-origin-label-4cf3860a").to_owned();
    }
    o.scope.label().to_owned()
}

/// Whether git named the active profile's config file (see `settings::profiles`): written
/// over by gitcraft, so never changed here.
pub(super) fn is_profile_file(file: &str) -> bool {
    let Some(profile) = crate::git::profile_config() else {
        return false;
    };
    let norm = |s: &str| {
        let s = s.replace('\\', "/");
        if cfg!(windows) { s.to_lowercase() } else { s }
    };
    norm(file) == norm(&profile.to_string_lossy())
}

pub(super) fn details(
    ui: &mut Ui,
    state: &mut ConfigState,
    setting: &'static Setting,
    values: &[&Origin],
    env: &Env,
    acts: &mut Vec<Action>,
) {
    card(ui, setting.key, Some(setting.about_label()), |ui| {
        if values.is_empty() {
            faint(
                ui,
                crate::i18n::text("repo-workflows-doctor-details-ba8d6068"),
            );
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
pub(super) fn value_row(
    ui: &mut Ui,
    origin: &Origin,
    wins: bool,
    locked: bool,
    acts: &mut Vec<Action>,
) {
    let t = Tokens::get(ui.ctx());
    ui.horizontal_wrapped(|ui| {
        if wins {
            widgets::chip(
                ui,
                crate::i18n::text("repo-workflows-doctor-value-row-4b67153d"),
                t.success,
            );
        } else {
            widgets::chip(
                ui,
                crate::i18n::text("repo-workflows-doctor-value-row-62a03f09"),
                t.text_faint,
            );
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
            _ if origin.scope == Scope::Command => Some(crate::i18n::text(
                "repo-workflows-doctor-value-row-4d8ff0fe",
            )),
            _ if profile => Some(crate::i18n::text(
                "repo-workflows-doctor-value-row-fd2cc5af",
            )),
            _ => None,
        };
        let args = config::unset_args(origin);
        let why = why.or(args.as_ref().err().map(String::as_str));
        let remove = ui
            .add_enabled(
                why.is_none() && !locked,
                egui::Button::new(crate::i18n::text(
                    "repo-workflows-doctor-value-row-c3812fc4",
                )),
            )
            .on_hover_text(crate::i18n::message(
                "repo-workflows-doctor-value-row-29f009c0",
                &[("origin-key-1", (origin.key).clone().into())],
            ))
            .on_disabled_hover_text(why.unwrap_or(crate::i18n::text(
                "repo-workflows-doctor-value-row-b5c82673",
            )));
        if remove.clicked()
            && let Ok(op) = &args
        {
            acts.push(remove_confirm(origin, op.clone()));
        }
    });
}

/// The question asked before removing `origin` from its file.
pub(super) fn remove_confirm(origin: &Origin, op: Vec<String>) -> Action {
    let file = origin.file.clone().unwrap_or_default();

    Action::Doctor(DoctorAct::Confirm(
        Confirm {
            title: format!("Remove {}?", origin.key),
            body: format!(
                "Remove {} = {} from {file}?{} The value isn't kept: to get it back, set it \
             again.",
                origin.key,
                origin.shown_value(),
                match origin.scope {
                    Scope::Global => " This applies to all your repositories.",
                    Scope::System => " This applies to every user of this machine.",
                    _ => "",
                }
            ),
            button: "Remove".to_owned(),
            label: format!(
                "Remove {} ({}) from {file}",
                origin.key,
                shortened(origin.shown_value())
            ),
            ops: vec![op],
            presentation: None,
        }
        .with_presentation(super::super::ConfirmKind::Remove {
            key: origin.key.clone(),
            value: origin.shown_value().into(),
            file: file.clone(),
            scope: origin.scope,
        }),
    ))
}
