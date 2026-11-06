//! Hidden changes: tracked files git was told not to look at (`assume-unchanged`,
//! `skip-worktree`), what each flag means, clearing those set by mistake, and the files
//! sparse-checkout leaves out, kept apart since they are not flags anyone set by hand.

use egui::{RichText, Ui};

use super::page::{card, code, dim, faint, reading};
use super::{DoctorAct, FlagsState};
use crate::git::doctor::flags::{self, Flag, Flagged, Sparse};
use crate::repo::{Action, widgets};
use crate::theme::Tokens;

/// Most rows a list shows; the rest are counted.
const SHOWN: usize = 500;

pub(super) fn ui(ui: &mut Ui, state: &mut FlagsState, locked: bool, acts: &mut Vec<Action>) {
    faint(
        ui,
        "Files changed on disk that never show under Changes are often flagged in git's \
         index. Both flags hide a file's changes; they mean different things.",
    );
    card(ui, "The two flags", None, |ui| {
        dim(
            ui,
            "assume-unchanged: a promise to git that the file won't change, so it doesn't \
             check it, for speed on slow disks. Git may still overwrite it (a pull, a \
             checkout) and drop the promise; it is not for keeping local edits.",
        );
        ui.add_space(6.0);
        dim(
            ui,
            "skip-worktree: keep this working-tree version, whatever the index says. Meant \
             for files you change locally on purpose (a config with your settings); a pull \
             that changes the file stops instead of overwriting it.",
        );
    });
    let Some((list, sparse)) = reading(ui, &state.list).cloned() else {
        return;
    };
    let (manual, left_out): (Vec<Flagged>, Vec<Flagged>) =
        list.into_iter().partition(|f| f.flag != Flag::Sparse);
    manual_card(ui, state, &manual, locked, acts);
    sparse_card(ui, &sparse, &left_out);
}

/// The flags someone set: ticked to be cleared, all or some.
fn manual_card(
    ui: &mut Ui,
    state: &mut FlagsState,
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
            let ticked: Vec<Flagged> = manual
                .iter()
                .filter(|f| state.ticked.contains(&f.path))
                .cloned()
                .collect();
            let clear_ticked = ui
                .add_enabled(
                    !locked && !ticked.is_empty(),
                    egui::Button::new(format!("Clear the {} ticked", ticked.len())),
                )
                .on_disabled_hover_text("Tick the files to clear first");
            if clear_ticked.clicked() {
                acts.push(clear(&ticked));
            }
            let clear_all = ui
                .add_enabled(!locked, egui::Button::new("Clear all"))
                .on_hover_text("Git looks at every one of these files again");
            if clear_all.clicked() {
                acts.push(clear(manual));
            }
        });
        ui.add_space(6.0);
        for file in manual.iter().take(SHOWN) {
            ui.horizontal(|ui| {
                let mut on = state.ticked.contains(&file.path);
                if ui.checkbox(&mut on, "").changed() {
                    if on {
                        state.ticked.insert(file.path.clone());
                    } else {
                        state.ticked.remove(&file.path);
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

/// The `Action` that clears `files`' flags.
fn clear(files: &[Flagged]) -> Action {
    let label = match files {
        [one] => format!("Clear the flag of {}", one.path),
        _ => format!("Clear the flags of {} files", files.len()),
    };
    Action::Doctor(DoctorAct::Run {
        label,
        ops: flags::clear_ops(files),
    })
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
        Sparse::Patterns => {
            "Sparse-checkout is on with patterns; files it leaves out are missing from the \
             working tree."
                .to_owned()
        }
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
