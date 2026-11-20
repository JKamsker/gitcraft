//! Asks before changing a working-tree file's metadata: the executable bit of its index
//! entry, or the letter case of its name. What the file is in the index is read in the
//! background when the dialog opens and read again when the edit is confirmed: the edit runs
//! only when it is still the one shown, through the tab's undoable operations.

use std::path::PathBuf;
use std::sync::mpsc::Receiver;

use egui::{Key, RichText, Ui};

use crate::git::file_metadata::{Plan, Snapshot};
use crate::repo::dialogs::Outcome;
use crate::repo::widgets;
use crate::theme::{self, Tokens};

mod flow;
mod menu;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Edit {
    Executable,
    LetterCase,
}

/// The file read, and for a confirmed executable-bit edit the patch file its commands apply
/// (or why it couldn't be saved).
type Reread = Result<(Snapshot, Option<Result<String, String>>), String>;

/// A read in flight: the first one, or the one confirming the edit.
struct Pending {
    rx: Receiver<Reread>,
    confirming: bool,
}

/// The dialog's file, edit and what was read of the file.
pub(crate) struct FileMetaForm {
    repo: PathBuf,
    path: String,
    edit: Edit,
    /// The new name typed (a case-only rename).
    name: String,
    focus: bool,
    read: Option<Result<Snapshot, String>>,
    pending: Option<Pending>,
    /// A confirming read found the file changed since it was shown.
    changed: bool,
}

impl FileMetaForm {
    pub(in crate::repo) fn show(&mut self, ui: &mut Ui, idle: bool) -> Outcome {
        if let Some(action) = self.settle() {
            return Outcome::Done(Box::new(action));
        }
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
        let plan = self.preview();
        match &plan {
            Some(Ok(plan)) => self.describe(ui, plan),
            Some(Err(why)) => warn(ui, why, t.warning),
            None => {}
        }
        ui.add_space(12.0);
        let ready = idle && self.pending.is_none() && matches!(plan, Some(Ok(_)));
        let label = crate::i18n::text(button);
        let (confirm, cancel) = buttons(ui, label, ready, enter);
        if confirm {
            self.start(true);
        }
        if cancel {
            Outcome::Cancel
        } else {
            Outcome::Open
        }
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

    /// Reading, why the file couldn't be read, and what changed since it was shown.
    fn state(&self, ui: &mut Ui, t: &Tokens) {
        if self.pending.is_some() {
            ui.horizontal(|ui| {
                ui.add(crate::spinner::Spinner::new().size(14.0));
                ui.label(RichText::new(crate::i18n::text("repo-core-1435")).color(t.text_dim));
            });
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(50));
        }
        if let Some(Err(e)) = &self.read {
            warn(ui, e, t.danger);
        }
        if self.changed {
            warn(ui, crate::i18n::text("file-metadata-changed"), t.warning);
        }
        let tracks_modes = matches!(&self.read, Some(Ok(s)) if s.file_mode);
        if self.edit == Edit::Executable && tracks_modes {
            warn(
                ui,
                crate::i18n::text("file-metadata-file-mode-warning"),
                t.warning,
            );
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

fn warn(ui: &mut Ui, text: &str, color: egui::Color32) {
    ui.add(egui::Label::new(RichText::new(text).color(color)).wrap());
}

/// The primary button, enabled when `ready` (also Enter), and Cancel: (confirmed, cancelled).
fn buttons(ui: &mut Ui, label: &str, ready: bool, enter: bool) -> (bool, bool) {
    ui.horizontal(|ui| {
        let primary = widgets::primary_button(ui, label, ready, 90.0);
        let cancel =
            egui::Button::new(crate::i18n::text("repo-core-0443")).min_size(egui::vec2(80.0, 30.0));
        let cancelled = ui.add(cancel).clicked();
        (ready && (primary.clicked() || enter), cancelled)
    })
    .inner
}

#[cfg(test)]
mod tests;
