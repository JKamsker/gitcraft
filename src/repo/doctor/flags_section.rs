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
    faint(
        ui,
        "Files changed on disk that never show under Changes are often flagged in git's \
         index. Both flags hide a file's changes; they mean different things, and git \
         supports neither as a way to ignore changes to tracked files.",
    );
    card(ui, "The two flags", None, |ui| {
        dim(
            ui,
            "assume-unchanged: a promise to git that the file won't change, so it doesn't \
             check it, for speed on slow disks. Git may still overwrite the file (a pull, a \
             checkout) and drop the flag; it is not for keeping local edits.",
        );
        ui.add_space(6.0);
        dim(
            ui,
            "skip-worktree: git keeps the working-tree version and doesn't compare it with \
             the index. Sparse-checkout uses it; people also use it to keep local edits to a \
             tracked file, which git warns isn't supported: a pull, checkout or merge that \
             changes the file may refuse to run until the flag is cleared. For local \
             settings, an untracked override file is safer.",
        );
    });
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
        0 => "No flagged files".to_owned(),
        1 => "1 flagged file".to_owned(),
        n => format!("{n} flagged files"),
    };
    card(ui, &title, None, |ui| {
        if manual.is_empty() {
            faint(
                ui,
                "No tracked file is marked assume-unchanged or skip-worktree.",
            );
            return;
        }
        ui.horizontal(|ui| {
            let some = !ticked.is_empty();
            let text = if some {
                format!("Clear the {} ticked", ticked.len())
            } else {
                "Clear ticked".to_owned()
            };
            let clear_ticked = ui
                .add_enabled(!locked && some, egui::Button::new(text))
                .on_disabled_hover_text(if locked {
                    "Wait for the running operation"
                } else {
                    "Tick the files to clear first"
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
                .add_enabled(!locked, egui::Button::new("Clear all"))
                .on_hover_text("Git looks at every one of these files again")
                .on_disabled_hover_text("Wait for the running operation");
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
                    .on_disabled_hover_text(
                        "Its name isn't valid UTF-8: clear it with git update-index in a terminal",
                    );
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
    Action::Doctor(DoctorAct::Confirm(Confirm {
        title: "Clear the flags?".to_owned(),
        body: format!(
            "Git will look at {what} again: local edits show under Changes, where Discard \
             would throw them away. Which files were flagged isn't kept, so this can't be \
             undone here."
        ),
        button: "Clear".to_owned(),
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
            "Sparse-checkout is on in cone mode with only the files at the top checked out."
                .to_owned()
        }
        Sparse::Cone(dirs) => format!(
            "Sparse-checkout is on in cone mode; checked out: {}.",
            dirs.join(", ")
        ),
        Sparse::Patterns => "Sparse-checkout is on with patterns: every skip-worktree file \
                             counts as left out by them, so none of them is cleared here."
            .to_owned(),
    };
    let title = format!("Left out by sparse-checkout ({})", left_out.len());
    card(ui, &title, Some(&note), |ui| {
        faint(
            ui,
            "Git marks these skip-worktree too, but they are sparse-checkout's: change what is \
             checked out with git sparse-checkout (set, add, disable), not by clearing flags.",
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
            RichText::new(format!("and {} more", total - SHOWN))
                .italics()
                .color(Tokens::get(ui.ctx()).text_faint),
        );
    }
}
