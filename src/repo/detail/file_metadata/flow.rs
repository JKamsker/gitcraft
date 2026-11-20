//! Reading the file in the background, the edit shown, and the edit run once a confirming
//! read finds the file as it was shown.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, TryRecvError};

use super::{Edit, FileMetaForm, Pending, Reread};
use crate::git::file_metadata::{self as meta, Plan};
use crate::repo::operations::try_step;
use crate::repo::{Action, Refresh, Undo};

impl FileMetaForm {
    /// A dialog for `edit` of `path` in `repo`, reading the file at once.
    pub(super) fn new(repo: PathBuf, path: &str, edit: Edit) -> Self {
        let name = path.rsplit('/').next().unwrap_or(path).to_owned();
        let mut form = FileMetaForm {
            repo,
            path: path.to_owned(),
            edit,
            name,
            focus: true,
            read: None,
            pending: None,
            changed: false,
        };
        form.start(false);
        form
    }

    /// Reads the file on a background thread: to show it, or to confirm the edit shown.
    pub(super) fn start(&mut self, confirming: bool) {
        let (tx, rx) = mpsc::channel();
        let (repo, path, edit) = (self.repo.clone(), self.path.clone(), self.edit);
        let name = self.name.clone();
        crate::connections::spawn(move || {
            // The dialog may be closed by then: nobody is waiting.
            let _ = tx.send(read(
                &repo,
                &path,
                edit,
                confirming.then_some(name.as_str()),
            ));
        });
        self.pending = Some(Pending { rx, confirming });
    }

    /// The edit as the file was read, shown before it is confirmed: an executable-bit edit's
    /// patch file is only written once it is.
    pub(super) fn preview(&self) -> Option<Result<Plan, String>> {
        let Some(Ok(snapshot)) = &self.read else {
            return None;
        };
        Some(match self.edit {
            Edit::Executable => meta::executable_plan(snapshot, patch_placeholder()),
            Edit::LetterCase => meta::case_rename_plan(snapshot, &self.name),
        })
    }

    /// Takes a finished read. After a confirming one, the edit to run when it is still the
    /// one shown; otherwise the dialog shows what changed, to confirm again.
    pub(super) fn settle(&mut self) -> Option<Action> {
        let pending = self.pending.as_ref()?;
        let confirming = pending.confirming;
        let result = match pending.rx.try_recv() {
            Ok(result) => result,
            Err(TryRecvError::Empty) => return None,
            Err(TryRecvError::Disconnected) => {
                Err(crate::i18n::text("file-metadata-read-failed").to_owned())
            }
        };
        self.pending = None;
        let (snapshot, patch) = match result {
            Ok(found) => found,
            Err(e) => {
                self.read = Some(Err(e));
                return None;
            }
        };
        let shown = self.preview();
        self.read = Some(Ok(snapshot.clone()));
        if !confirming {
            return None;
        }
        let now = self.preview();
        self.changed = now != shown || !matches!(now, Some(Ok(_)));
        if self.changed {
            return None;
        }
        let plan = match self.edit {
            Edit::Executable => patch
                .unwrap_or_else(|| Err(String::new()))
                .and_then(|file| meta::executable_plan(&snapshot, &file)),
            Edit::LetterCase => meta::case_rename_plan(&snapshot, &self.name).map(rolled_back),
        };
        match plan {
            Ok(plan) => Some(run_action(plan)),
            Err(e) => {
                self.read = Some(Err(e));
                None
            }
        }
    }
}

/// Shown in the commands for the patch file written once the edit is confirmed.
fn patch_placeholder() -> &'static str {
    crate::i18n::text("file-metadata-patch-placeholder")
}

/// The file as it is now; when confirming (`name`: the new name typed), also the patch file
/// of an executable-bit edit that can be made.
fn read(repo: &Path, path: &str, edit: Edit, confirming: Option<&str>) -> Reread {
    let snapshot = meta::snapshot(repo, path, edit == Edit::LetterCase)?;
    let patch = (edit == Edit::Executable && confirming.is_some())
        .then(|| meta::save_executable_patch(repo, &snapshot));
    Ok((snapshot, patch))
}

/// A two-step rename as the operation runs it. Should the second move fail (something took
/// the new name in the moment between), the file goes back to its name (`mv -k` does nothing
/// when nothing is left at the temporary path), and the last check fails the operation, so
/// no undo is recorded for a rename that didn't happen. Undo is guarded the same way.
pub(super) fn rolled_back(plan: Plan) -> Plan {
    let Some(via) = plan.via.clone() else {
        return plan;
    };
    let steps = |from: &str, to: &str| {
        let note = crate::i18n::message(
            "file-metadata-rename-rolled-back",
            &[
                ("via", via.clone().into()),
                ("to", to.to_owned().into()),
                ("from", from.to_owned().into()),
            ],
        );
        vec![
            ["mv", "--", from, &via].map(str::to_owned).to_vec(),
            try_step(&["mv", "--", &via, to], &note),
            ["mv", "-k", "--", &via, from].map(str::to_owned).to_vec(),
            [
                "--literal-pathspecs",
                "ls-files",
                "--error-unmatch",
                "--",
                to,
            ]
            .map(str::to_owned)
            .to_vec(),
        ]
    };
    Plan {
        ops: steps(&plan.from, &plan.to),
        undo: steps(&plan.to, &plan.from),
        ..plan
    }
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
