//! Asks before changing a file's metadata: the executable bit of its index entry, or the
//! letter case of its name. What the file is in the index is read in the background when the
//! dialog opens; the edit, its commands and its undo are worked out from that.

use std::sync::atomic::{AtomicU64, Ordering};

use egui::{Key, RichText, Ui};

use crate::git::file_metadata::{self as meta, Plan, Snapshot};
use crate::git::{Area, StatusFile};
use crate::repo::{Action, DiffKey, Refresh, RepoTab, Undo};
use crate::theme::Tokens;

use super::{Dialog, Outcome, cancel_button, field, preview, title, widgets};

/// Tells one reading from the next: a dialog only takes the answer to its own.
static ASKED: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Edit {
    Executable,
    LetterCase,
}

enum State {
    Unread,
    Loading(u64),
    /// The file, and for the executable bit the patch file its commands apply (or why not).
    Ready(Snapshot, Option<Result<String, String>>),
    Failed(String),
}

/// The dialog's file, edit and what was read of the file.
pub struct FileMetaForm {
    path: String,
    edit: Edit,
    /// The new name typed (a case-only rename).
    name: String,
    focus: bool,
    state: State,
}

impl FileMetaForm {
    pub(crate) fn new(path: &str, edit: Edit) -> Self {
        let name = path.rsplit('/').next().unwrap_or(path).to_owned();
        FileMetaForm {
            path: path.to_owned(),
            edit,
            name,
            focus: true,
            state: State::Unread,
        }
    }

    fn plan(&self) -> Option<Result<Plan, String>> {
        let State::Ready(snapshot, patch) = &self.state else {
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

    pub(super) fn show(&mut self, ui: &mut Ui, idle: bool) -> Outcome {
        let t = Tokens::get(ui.ctx());
        let (heading, button) = match self.edit {
            Edit::Executable => ("file-metadata-executable-title", "file-metadata-apply"),
            Edit::LetterCase => ("file-metadata-case-title", "file-metadata-rename"),
        };
        title(ui, crate::i18n::text(heading), &self.path, &t);
        let mut enter = ui.input(|i| i.key_pressed(Key::Enter));
        if self.edit == Edit::LetterCase {
            let typed = field(ui, &mut self.name, "", &mut self.focus);
            enter = crate::a11y::singleline_enter(&typed);
            ui.add_space(6.0);
        }
        match &self.state {
            State::Unread | State::Loading(_) => {
                ui.horizontal(|ui| {
                    ui.add(crate::spinner::Spinner::new().size(14.0));
                    ui.label(RichText::new(crate::i18n::text("repo-core-1435")).color(t.text_dim));
                });
            }
            State::Failed(e) => {
                ui.add(egui::Label::new(RichText::new(e).color(t.danger)).wrap());
            }
            State::Ready(..) => {}
        }
        let plan = self.plan();
        match &plan {
            Some(Ok(plan)) => self.describe(ui, plan),
            Some(Err(why)) => {
                ui.add(egui::Label::new(RichText::new(why).color(t.warning)).wrap());
            }
            None => {}
        }
        ui.add_space(12.0);
        let mut ready = plan.and_then(Result::ok);
        let valid = idle && ready.is_some();
        let mut outcome = Outcome::Open;
        ui.horizontal(|ui| {
            let primary = widgets::primary_button(ui, crate::i18n::text(button), valid, 90.0);
            if valid
                && (primary.clicked() || enter)
                && let Some(plan) = ready.take()
            {
                outcome = Outcome::Done(Box::new(run_action(plan)));
            }
            if cancel_button(ui) {
                outcome = Outcome::Cancel;
            }
        });
        outcome
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
                        (
                            "executable",
                            meta::is_executable(&plan.to).to_string().into(),
                        ),
                    ],
                ),
                crate::i18n::text("file-metadata-executable-kept"),
            ),
            Edit::LetterCase => (
                format!("{} → {}", plan.from, plan.to),
                crate::i18n::text("file-metadata-case-kept"),
            ),
        };
        ui.label(RichText::new(change).font(crate::theme::medium(12.5)));
        ui.add_space(4.0);
        ui.add(egui::Label::new(RichText::new(kept).color(t.text_dim)).wrap());
        preview::commands(ui, &plan.label, &plan.ops, None);
        let undo = undo_of(plan);
        preview::undo_note(ui, &undo, &preview::WhatItDoes::default());
    }
}

fn undo_of(plan: &Plan) -> Undo {
    Undo::Custom {
        undo: plan.undo.clone(),
        redo: plan.ops.clone(),
    }
}

/// The operation the dialog confirms: undoable with the plan's inverse commands.
fn run_action(plan: Plan) -> Action {
    let undo = undo_of(&plan);
    Action::UndoableOps {
        label: plan.label,
        ops: plan.ops,
        refresh: Refresh::Status,
        undo,
    }
}

impl RepoTab {
    /// Reads what the dialog's edit needs, on a background thread.
    pub(super) fn read_file_metadata(&self, form: &mut FileMetaForm, ctx: &egui::Context) {
        let asked = ASKED.fetch_add(1, Ordering::Relaxed) + 1;
        form.state = State::Loading(asked);
        let (repo, path) = (self.path.clone(), form.path.clone());
        let edit = form.edit;
        self.then(
            ctx,
            move || {
                let found = meta::snapshot(&repo, &path, edit == Edit::LetterCase)?;
                let patch =
                    (edit == Edit::Executable).then(|| meta::save_executable_patch(&repo, &found));
                Ok::<_, String>((found, patch))
            },
            move |tab, result, _| {
                let Some(Dialog::FileMetadata(form)) = &mut tab.dialog else {
                    return;
                };
                if matches!(form.state, State::Loading(n) if n == asked) {
                    form.state = match result {
                        Ok((found, patch)) => State::Ready(found, patch),
                        Err(e) => State::Failed(e),
                    };
                }
            },
        );
    }

    /// The executable bit (a staged file) and a case-only rename (any tracked file) in the
    /// menu of a working-tree file `key`.
    pub(in crate::repo) fn file_metadata_menu_items(
        &self,
        ui: &mut Ui,
        key: &DiffKey,
        acts: &mut Vec<Action>,
    ) {
        let DiffKey::Work(file) = key else {
            return;
        };
        let edits = offered(file);
        if edits.is_empty() {
            return;
        }
        ui.separator();
        let idle = self.busy_op.is_none();
        for edit in edits {
            let (label, tip, icon) = match edit {
                Edit::Executable => (
                    "file-metadata-executable-menu",
                    "file-metadata-executable-menu-tip",
                    "square-terminal",
                ),
                Edit::LetterCase => (
                    "file-metadata-case-menu",
                    "file-metadata-case-menu-tip",
                    "arrow-down-a-z",
                ),
            };
            let r = crate::menu_item::MenuItem::new(crate::i18n::text(label))
                .icon(icon)
                .enabled(idle)
                .show(ui)
                .on_hover_text(crate::i18n::text(tip));
            if r.clicked() {
                let form = FileMetaForm::new(&file.path, edit);
                acts.push(Action::Dialog(Dialog::FileMetadata(Box::new(form))));
            }
        }
    }
}

/// The edits a working-tree file's menu offers: only for a file in the index and the working
/// tree, not conflicted; the executable bit only on the staged side, as it is an index edit.
/// An unstaged `A` is an intent-to-add entry (`git add -N`): nothing real is in the index yet.
pub(crate) fn offered(file: &StatusFile) -> Vec<Edit> {
    match (file.area, file.code) {
        (_, 'D' | '?' | 'U') | (Area::Unstaged, 'A') => Vec::new(),
        (Area::Staged, _) => vec![Edit::Executable, Edit::LetterCase],
        (Area::Unstaged, _) => vec![Edit::LetterCase],
        (Area::Untracked | Area::Conflicted, _) => Vec::new(),
    }
}

#[cfg(test)]
mod tests;
