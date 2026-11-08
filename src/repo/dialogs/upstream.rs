//! The dialog that sets the remote branch a local one tracks, publishing it when that branch
//! doesn't exist yet.

use egui::{RichText, Ui};

use crate::git::Branch;
use crate::repo::{Action, Refresh, Undo, sidebar::remote_short};
use crate::theme::Tokens;

use super::{Outcome, buttons, dash_spaces, field, title, usable};

/// `branch` tracks `remote`/`name`: set when that remote branch exists, else pushed there first.
/// `undo` brings back what it tracked before.
#[allow(clippy::too_many_arguments)]
pub(super) fn set_upstream(
    ui: &mut Ui,
    enter: bool,
    branch: &str,
    remote: &mut String,
    name: &mut String,
    undo: &[Vec<String>],
    remote_names: &[&str],
    remote_branches: &[(String, Vec<Branch>)],
    focus: &mut bool,
) -> Outcome {
    let t = Tokens::get(ui.ctx());
    title(
        ui,
        crate::i18n::text("repo-core-1522"),
        &crate::i18n::message(
            "repo-core-1523",
            &[("branch", format!("{}", branch).into())],
        ),
        &t,
    );
    if remote_names.len() > 1 {
        egui::ComboBox::from_id_salt("upstream-remote")
            .width(ui.available_width())
            .selected_text(remote.as_str())
            .show_ui(ui, |ui| {
                for r in remote_names {
                    if ui.selectable_label(remote == r, *r).clicked() {
                        *remote = (*r).to_owned();
                    }
                }
            });
        ui.add_space(4.0);
    }
    let existing: Vec<&str> = remote_branches
        .iter()
        .filter(|(r, _)| r == remote)
        .flat_map(|(_, list)| list)
        .filter_map(|b| remote_short(&b.name, remote))
        .collect();
    field(ui, name, crate::i18n::text("repo-core-1525"), focus);
    dash_spaces(name);
    if !existing.is_empty() {
        ui.add_space(4.0);
        egui::ComboBox::from_id_salt("upstream-existing")
            .width(ui.available_width())
            .selected_text(crate::i18n::text("repo-core-1527"))
            .show_ui(ui, |ui| {
                for b in &existing {
                    if ui.selectable_label(name.trim() == *b, *b).clicked() {
                        *name = (*b).to_owned();
                    }
                }
            });
    }
    let target = name.trim().to_owned();
    let exists = existing.contains(&target.as_str());
    let valid = usable(&target) && usable(remote);
    ui.add_space(6.0);
    if valid {
        let note = if exists {
            crate::i18n::message(
                "repo-core-1528",
                &[
                    ("remote", format!("{}", remote).into()),
                    ("target", format!("{}", target).into()),
                ],
            )
        } else {
            crate::i18n::message(
                "repo-core-1529",
                &[
                    ("remote", format!("{}", remote).into()),
                    ("target", format!("{}", target).into()),
                    ("branch", format!("{}", branch).into()),
                ],
            )
        };
        ui.label(RichText::new(note).color(t.text_dim));
    }
    ui.add_space(10.0);
    let button = if exists {
        crate::i18n::text("repo-core-1522")
    } else {
        crate::i18n::text("repo-core-0032")
    };
    buttons(ui, button, valid, enter, || {
        let upstream = format!("{remote}/{target}");
        if exists {
            // The full name: a local branch or tag also called `{remote}/{target}` makes the
            // short one ambiguous, which git refuses.
            let set = vec![
                "branch".to_owned(),
                format!("--set-upstream-to=refs/remotes/{upstream}"),
                branch.to_owned(),
            ];
            Action::UndoableOps {
                label: crate::i18n::message(
                    "repo-core-1532",
                    &[("upstream", format!("{}", upstream).into())],
                ),
                ops: vec![set.clone()],
                refresh: Refresh::All,
                undo: Undo::Custom {
                    undo: undo.to_vec(),
                    redo: vec![set],
                },
            }
        } else {
            Action::Ops {
                label: crate::i18n::message(
                    "repo-core-1533",
                    &[
                        ("branch", format!("{}", branch).into()),
                        ("upstream", format!("{}", upstream).into()),
                    ],
                ),
                ops: vec![vec![
                    "push".to_owned(),
                    "-u".to_owned(),
                    remote.clone(),
                    format!("refs/heads/{branch}:refs/heads/{target}"),
                ]],
                refresh: Refresh::All,
            }
        }
    })
}
