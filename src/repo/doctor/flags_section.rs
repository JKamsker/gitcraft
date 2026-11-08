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
        [one] => (format!("Clear the flag of {}", one.path), one.path.clone()),
        _ => (
            format!("Clear the flags of {} files", files.len()),
            format!("{} files", files.len()),
        ),
    };
    Action::Doctor(DoctorAct::Confirm(
        Confirm {
            title: "Clear the flags?".to_owned(),
            body: format!(
                "Git will look at {what} again: local edits show under Changes, where Discard \
             would throw them away. Which files were flagged isn't kept, so this can't be \
             undone here."
            ),
            button: "Clear".to_owned(),
            label,
            ops: flags::clear_ops(files),
            presentation: None,
        }
        .with_presentation(super::ConfirmKind::Clear {
            paths: files.iter().map(|f| f.path.clone()).collect(),
        }),
    ))
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

#[cfg(test)]
mod fluent_r1_confirmation_tests {
    use super::*;
    use egui_kittest::{Harness, kittest::Queryable};
    #[test]
    fn fluent_partition_r1_cached_doctor_confirmation_repaints_without_changing_label_or_argv() {
        let files: Vec<_> = (0..1234)
            .map(|i| Flagged {
                path: format!("raw/{i}.txt"),
                flag: Flag::AssumeUnchanged,
            })
            .collect();
        crate::i18n::set_language("de");
        let Action::Doctor(DoctorAct::Confirm(confirm)) = clear(&files) else {
            panic!("actual clear must confirm")
        };
        assert_eq!(confirm.label, "Clear the flags of 1234 files");
        let ops = confirm.ops.clone();
        let raw = (
            confirm.title.clone(),
            confirm.body.clone(),
            confirm.button.clone(),
        );
        let mut h = Harness::new_ui_state(
            |ui, state: &mut (super::super::Confirm, Vec<Action>)| {
                let (t, b, button) = state.0.displayed();
                super::super::page::ask(
                    ui,
                    &t,
                    &b,
                    &button,
                    crate::i18n::text("repo-workflows-doctor-confirm-modal-19766ed6"),
                    |yes| Action::Doctor(DoctorAct::Answer(yes)),
                    &mut state.1,
                );
            },
            (confirm, Vec::new()),
        );
        for lang in ["de", "en-US", "de"] {
            crate::i18n::set_language(lang);
            h.run();
            let (title, body, button) = h.state().0.displayed();
            h.get_by_label(&title);
            h.get_by_label(&body);
            h.get_by_label(&button);
            assert_eq!(h.state().0.ops, ops);
            assert_eq!(h.state().0.label, "Clear the flags of 1234 files");
            assert_eq!(
                (
                    h.state().0.title.clone(),
                    h.state().0.body.clone(),
                    h.state().0.button.clone()
                ),
                raw
            );
            assert!(h.state().1.is_empty());
            if lang == "de" {
                assert!(body.contains("1.234"));
            }
        }
        h.get_by_label(&h.state().0.displayed().2).click();
        h.run();
        assert!(matches!(
            h.state().1.as_slice(),
            [Action::Doctor(DoctorAct::Answer(true))]
        ));
        crate::i18n::set_language("en-US");
    }
}
