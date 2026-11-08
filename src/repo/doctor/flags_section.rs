//! Hidden changes: tracked files git was told not to look at (`assume-unchanged`,
//! `skip-worktree`), what each flag means, clearing those set by mistake, and the files
//! sparse-checkout leaves out, kept apart since they are not flags anyone set by hand.

use egui::{RichText, Ui};

use super::page::{card, code, dim, faint, reading};
use super::{Confirm, DoctorAct, FlagsState};
use crate::git::doctor::flags::{self, Flag, Flagged, Sparse};
use crate::repo::{Action, widgets};
use crate::theme::Tokens;

/// Most rows a list shows; the rest are counted.
const SHOWN: usize = 500;

pub(super) fn ui(ui: &mut Ui, state: &mut FlagsState, locked: bool, acts: &mut Vec<Action>) {
    faint(ui, crate::i18n::text("repo-workflows-doctor-ui-195ee0f7"));
    card(
        ui,
        crate::i18n::text("repo-workflows-doctor-ui-56146558"),
        None,
        |ui| {
            dim(ui, crate::i18n::text("repo-workflows-doctor-ui-282e3905"));
            ui.add_space(6.0);
            dim(ui, crate::i18n::text("repo-workflows-doctor-ui-99079609"));
        },
    );
    let FlagsState { list, ticked } = state;
    let Some(data) = reading(ui, list) else {
        return;
    };
    manual_card(ui, ticked, &data.manual, locked, acts);
    sparse_card(ui, &data.sparse, &data.left_out);
}

/// The flags someone set: ticked to be cleared, all or some.
fn manual_card(
    ui: &mut Ui,
    ticked: &mut std::collections::BTreeSet<String>,
    manual: &[Flagged],
    locked: bool,
    acts: &mut Vec<Action>,
) {
    let t = Tokens::get(ui.ctx());
    let title = match manual.len() {
        0 => crate::i18n::text("repo-workflows-doctor-manual-card-261a3c3d").to_owned(),
        1 => crate::i18n::text("repo-workflows-doctor-manual-card-f01d6b0e").to_owned(),
        n => crate::i18n::message(
            "repo-workflows-doctor-manual-card-70727cd6",
            &[("n", (n).clone().into())],
        ),
    };
    card(ui, &title, None, |ui| {
        if manual.is_empty() {
            faint(
                ui,
                crate::i18n::text("repo-workflows-doctor-manual-card-eb376b6a"),
            );
            return;
        }
        ui.horizontal(|ui| {
            let some = !ticked.is_empty();
            let text = if some {
                crate::i18n::message(
                    "repo-workflows-doctor-manual-card-14faa67e",
                    &[("ticked-len-1", (ticked.len()).clone().into())],
                )
            } else {
                crate::i18n::text("repo-workflows-doctor-manual-card-6c8cdf68").to_owned()
            };
            let clear_ticked = ui
                .add_enabled(!locked && some, egui::Button::new(text))
                .on_disabled_hover_text(if locked {
                    crate::i18n::text("repo-workflows-doctor-manual-card-b5c82673")
                } else {
                    crate::i18n::text("repo-workflows-doctor-manual-card-98cd282b")
                });
            if clear_ticked.clicked() {
                let chosen: Vec<Flagged> = manual
                    .iter()
                    .filter(|f| ticked.contains(&f.path))
                    .cloned()
                    .collect();
                acts.push(clear(&chosen));
            }
            let clear_all = ui
                .add_enabled(
                    !locked,
                    egui::Button::new(crate::i18n::text(
                        "repo-workflows-doctor-manual-card-29a390f9",
                    )),
                )
                .on_hover_text(crate::i18n::text(
                    "repo-workflows-doctor-manual-card-febfc014",
                ))
                .on_disabled_hover_text(crate::i18n::text(
                    "repo-workflows-doctor-manual-card-b5c82673",
                ));
            if clear_all.clicked() {
                acts.push(clear(manual));
            }
        });
        ui.add_space(6.0);
        for file in manual.iter().take(SHOWN) {
            ui.horizontal(|ui| {
                let readable = flags::clearable(&file.path);
                let mut on = ticked.contains(&file.path);
                let check = ui
                    .add_enabled(readable, egui::Checkbox::without_text(&mut on))
                    .on_disabled_hover_text(crate::i18n::text(
                        "repo-workflows-doctor-manual-card-bf185132",
                    ));
                if check.changed() {
                    if on {
                        ticked.insert(file.path.clone());
                    } else {
                        ticked.remove(&file.path);
                    }
                }
                let color = match file.flag {
                    Flag::SkipWorktree => t.accent,
                    _ => t.warning,
                };
                widgets::chip(ui, file.flag.label(), color);
                ui.label(code(&file.path).color(t.text));
            });
        }
        more(ui, manual.len());
    });
}

/// The question asked before clearing `files`' flags.
fn clear(files: &[Flagged]) -> Action {
    let (label, what) = match files {
        [one] => (
            crate::i18n::message(
                "repo-workflows-doctor-clear-7f76d2cf",
                &[("one-path-1", (one.path).clone().into())],
            ),
            one.path.clone(),
        ),
        _ => (
            crate::i18n::message(
                "repo-workflows-doctor-clear-e318945b",
                &[("files-len-1", (files.len()).clone().into())],
            ),
            crate::i18n::message(
                "repo-workflows-doctor-clear-ac70d25c",
                &[("files-len-1", (files.len()).clone().into())],
            ),
        ),
    };
    Action::Doctor(DoctorAct::Confirm(Confirm {
        title: crate::i18n::text("repo-workflows-doctor-clear-909d7fba").to_owned(),
        body: crate::i18n::message(
            "repo-workflows-doctor-clear-ae587a37",
            &[("what", (what).clone().into())],
        ),
        button: crate::i18n::text("repo-workflows-doctor-clear-83b12c22").to_owned(),
        label,
        ops: flags::clear_ops(files),
    }))
}

/// What sparse-checkout leaves out.
fn sparse_card(ui: &mut Ui, sparse: &Sparse, left_out: &[Flagged]) {
    let t = Tokens::get(ui.ctx());
    let note = match sparse {
        Sparse::Off => return,
        Sparse::Cone(dirs) if dirs.is_empty() => {
            crate::i18n::text("repo-workflows-doctor-sparse-card-9cff4083").to_owned()
        }
        Sparse::Cone(dirs) => crate::i18n::message(
            "repo-workflows-doctor-sparse-card-ce4ee135",
            &[("dirs-join-1", (dirs.join(", ")).clone().into())],
        ),
        Sparse::Patterns => {
            crate::i18n::text("repo-workflows-doctor-sparse-card-f364e530").to_owned()
        }
    };
    let title = crate::i18n::message(
        "repo-workflows-doctor-sparse-card-04bc71e9",
        &[("left-out-len-1", (left_out.len()).clone().into())],
    );
    card(ui, &title, Some(&note), |ui| {
        faint(
            ui,
            crate::i18n::text("repo-workflows-doctor-sparse-card-7bb29a8c"),
        );
        ui.add_space(6.0);
        for file in left_out.iter().take(SHOWN) {
            ui.label(code(&file.path).color(t.text_dim));
        }
        more(ui, left_out.len());
    });
}

/// How many more there are than the list shows.
fn more(ui: &mut Ui, total: usize) {
    if total > SHOWN {
        ui.add_space(4.0);
        ui.label(
            RichText::new(crate::i18n::message(
                "repo-workflows-doctor-more-683768ab",
                &[("total-shown-1", (total - SHOWN).clone().into())],
            ))
            .italics()
            .color(Tokens::get(ui.ctx()).text_faint),
        );
    }
}
