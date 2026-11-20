//! Asks before changing a working-tree file's metadata: the executable bit of its index
//! entry, or the letter case of its name. What the file is in the index is read in the
//! background when the dialog opens; the edit, its commands and its undo are worked out from
//! that, and run through the tab's undoable operations.

use std::sync::{Arc, OnceLock};

use egui::{Key, RichText, Ui};

use crate::git::file_metadata::{self as meta, Plan, Snapshot};
use crate::repo::dialogs::Outcome;
use crate::repo::{Action, Refresh, Undo, widgets};
use crate::theme::{self, Tokens};

mod menu;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Edit {
    Executable,
    LetterCase,
}

/// The file, and for the executable bit the patch file its commands apply (or why not).
type Read = Result<(Snapshot, Option<Result<String, String>>), String>;

/// The dialog's file, edit and what was read of the file.
pub(crate) struct FileMetaForm {
    path: String,
    edit: Edit,
    /// The new name typed (a case-only rename).
    name: String,
    focus: bool,
    /// Filled once by the background read; only this dialog holds it, so no other read's
    /// answer can land here.
    read: Arc<OnceLock<Read>>,
}

impl FileMetaForm {
    fn new(path: &str, edit: Edit) -> Self {
        let name = path.rsplit('/').next().unwrap_or(path).to_owned();
        FileMetaForm {
            path: path.to_owned(),
            edit,
            name,
            focus: true,
            read: Arc::default(),
        }
    }

    fn plan(&self) -> Option<Result<Plan, String>> {
        let Ok((snapshot, patch)) = self.read.get()? else {
            return None;
        };
        Some(match self.edit {
            Edit::Executable => patch
                .clone()
                .unwrap_or_else(|| Err(String::new()))
                .and_then(|file| meta::executable_plan(snapshot, &file)),
            Edit::LetterCase => meta::case_rename_plan(snapshot, &self.name),
        })
    }

    pub(in crate::repo) fn show(&mut self, ui: &mut Ui, idle: bool) -> Outcome {
        let t = Tokens::get(ui.ctx());
        let (heading, button) = match self.edit {
            Edit::Executable => ("file-metadata-executable-title", "file-metadata-apply"),
            Edit::LetterCase => ("file-metadata-case-title", "file-metadata-rename"),
        };
        ui.label(RichText::new(crate::i18n::text(heading)).font(theme::semibold(15.0)));
        ui.label(RichText::new(&self.path).color(t.text_dim));
        ui.add_space(10.0);
        let mut enter = ui.input(|i| i.key_pressed(Key::Enter));
        if self.edit == Edit::LetterCase {
            enter = self.name_field(ui);
        }
        self.state(ui, &t);
        let plan = self.plan();
        match &plan {
            Some(Ok(plan)) => self.describe(ui, plan),
            Some(Err(why)) => {
                ui.add(egui::Label::new(RichText::new(why).color(t.warning)).wrap());
            }
            None => {}
        }
        ui.add_space(12.0);
        buttons(
            ui,
            crate::i18n::text(button),
            idle,
            enter,
            plan.and_then(Result::ok),
        )
    }

    /// The new name, which takes the keyboard when the dialog opens. True on Enter.
    fn name_field(&mut self, ui: &mut Ui) -> bool {
        let edit = ui.add(
            egui::TextEdit::singleline(&mut self.name)
                .desired_width(f32::INFINITY)
                .margin(egui::Margin::symmetric(8, 6)),
        );
        if std::mem::take(&mut self.focus) {
            edit.request_focus();
        }
        ui.add_space(6.0);
        crate::a11y::singleline_enter(&edit)
    }

    /// Reading, or why the file couldn't be read.
    fn state(&self, ui: &mut Ui, t: &Tokens) {
        match self.read.get() {
            None => {
                ui.horizontal(|ui| {
                    ui.add(crate::spinner::Spinner::new().size(14.0));
                    ui.label(RichText::new(crate::i18n::text("repo-core-1435")).color(t.text_dim));
                });
            }
            Some(Err(e)) => {
                ui.add(egui::Label::new(RichText::new(e).color(t.danger)).wrap());
            }
            Some(Ok(_)) => {}
        }
    }

    /// Before and after, what stays as it is, the commands and the undo.
    fn describe(&self, ui: &mut Ui, plan: &Plan) {
        let t = Tokens::get(ui.ctx());
        let (change, kept) = match self.edit {
            Edit::Executable => (
                crate::i18n::message(
                    "file-metadata-executable-change",
                    &[
                        ("from", plan.from.clone().into()),
                        ("to", plan.to.clone().into()),
                        ("executable", plan.executable.to_string().into()),
                    ],
                ),
                crate::i18n::text("file-metadata-executable-kept"),
            ),
            Edit::LetterCase => (
                format!("{} → {}", plan.from, plan.to),
                crate::i18n::text("file-metadata-case-kept"),
            ),
        };
        ui.label(RichText::new(change).font(theme::medium(12.5)));
        ui.add_space(4.0);
        ui.add(egui::Label::new(RichText::new(kept).color(t.text_dim)).wrap());
        ui.add_space(8.0);
        egui::CollapsingHeader::new(
            RichText::new(crate::i18n::text("repo-core-1448")).font(theme::medium(12.0)),
        )
        .id_salt(("file-metadata-commands", &plan.label))
        .default_open(false)
        .show(ui, |ui| {
            for op in &plan.ops {
                let line = format!("git {}", op.join(" "));
                ui.add(egui::Label::new(RichText::new(line).font(theme::mono(11.5))).wrap());
            }
        });
        ui.add_space(8.0);
        let key = crate::manual::command();
        let note = crate::i18n::message("repo-core-1452", &[("key", key.to_string().into())]);
        ui.add(egui::Label::new(RichText::new(note).color(t.success)).wrap());
    }
}

/// The primary button, enabled once there is an edit to make and nothing runs, and Cancel.
fn buttons(ui: &mut Ui, label: &str, idle: bool, enter: bool, plan: Option<Plan>) -> Outcome {
    let valid = idle && plan.is_some();
    let mut plan = plan;
    let mut outcome = Outcome::Open;
    ui.horizontal(|ui| {
        let primary = widgets::primary_button(ui, label, valid, 90.0);
        if valid
            && (primary.clicked() || enter)
            && let Some(plan) = plan.take()
        {
            outcome = Outcome::Done(Box::new(run_action(plan)));
        }
        let cancel =
            egui::Button::new(crate::i18n::text("repo-core-0443")).min_size(egui::vec2(80.0, 30.0));
        if ui.add(cancel).clicked() {
            outcome = Outcome::Cancel;
        }
    });
    outcome
}

/// The operation the dialog confirms: undoable with the plan's inverse commands.
fn run_action(plan: Plan) -> Action {
    let undo = Undo::Custom {
        undo: plan.undo,
        redo: plan.ops.clone(),
    };
    Action::UndoableOps {
        label: plan.label,
        ops: plan.ops,
        refresh: Refresh::Status,
        undo,
    }
}

#[cfg(test)]
mod tests;
