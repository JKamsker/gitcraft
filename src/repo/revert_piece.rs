//! Undoing one file, one hunk or a few lines of a past commit: the piece is reverse-applied to
//! the working tree (`git apply -R`, with a 3-way merge when the file changed since), where it
//! can be looked over before committing.

use std::collections::BTreeSet;
use std::time::Instant;

use super::messages::spawn_or;
use super::{DiffKey, DiffView, Msg, OpKind, Refresh, RepoTab, Undo, diff, ops, short};
use crate::git::{self, Reverted};

impl RepoTab {
    /// Undoes commit `hash`'s changes to `path` in the working tree.
    pub(super) fn revert_file(&mut self, hash: String, path: String, ctx: &egui::Context) {
        let label = format!("Revert {path} as changed in {}", short(&hash));
        let repo = self.path.clone();
        self.start_revert(label, ctx, move || {
            let patch = git::commit_file_patch(&repo, &hash, &path)?;
            git::save_patch_for(&repo, &patch)
        });
    }

    /// Undoes the changed `lines` (indices into the open commit diff's lines) in the working
    /// tree: a hunk's, or those clicked.
    pub(super) fn revert_lines(&mut self, lines: &BTreeSet<usize>, ctx: &egui::Context) {
        if self.refused_while_busy("Revert") {
            return;
        }
        let Some(view) = &mut self.diff else { return };
        let DiffView {
            key: DiffKey::Commit { hash, path },
            data: Some(Ok(data)),
            lines: None,
            ..
        } = &*view
        else {
            return;
        };
        if !data.revertable {
            return;
        }
        // A submodule's change: `git apply` would say it did it, and change nothing.
        if let Err(e) = git::check_revertable(&data.header, path) {
            self.status = Some((e, true));
            return;
        }
        let Some(patch) = diff::partial_patch(&data.header, &data.lines, lines, diff::Pick::Revert)
        else {
            return;
        };
        let n = lines.len();
        // The hunk's button picks all its changes.
        let whole_hunk = (0..data.lines.len())
            .filter(|&h| data.lines[h].kind == diff::Kind::Hunk)
            .any(|h| diff::hunk_changes(&data.lines, h) == *lines);
        let label = if whole_hunk {
            format!("Revert a hunk of {path} from {}", short(hash))
        } else {
            format!(
                "Revert {n} line{} of {path} from {}",
                if n == 1 { "" } else { "s" },
                short(hash)
            )
        };
        view.chosen.clear();
        view.anchor = None;
        let repo = self.path.clone();
        self.start_revert(label, ctx, move || git::save_patch_for(&repo, &patch));
    }

    /// Runs a revert in the background: `prepare` writes its patch where git reads it (on
    /// another machine for a repository there), then it is reverse-applied.
    fn start_revert(
        &mut self,
        label: String,
        ctx: &egui::Context,
        prepare: impl FnOnce() -> Result<String, String> + Send + 'static,
    ) {
        if self.refused_while_busy(&label) {
            return;
        }
        self.busy_op = Some(label.clone());
        self.op_started = Some(Instant::now());
        self.status = None;
        let repo = self.path.clone();
        // Ended all the same if it panics: the tab stays busy until then.
        let failed = {
            let label = label.clone();
            move |e| Msg::OpDone {
                label,
                result: Err(e),
                refresh: Refresh::Status,
                kind: OpKind::plain(),
                quiet: false,
                stopped: 0,
            }
        };
        spawn_or(&self.tx, ctx, failed, move || {
            let outcome = prepare()
                .and_then(|file| git::reverse_apply(&repo, &file).map(|done| (file, done)));
            Msg::Then(Box::new(move |tab, ctx| tab.reverted(&label, outcome, ctx)))
        });
    }

    /// Whether another operation is running, which `label` then waits for (the user is told).
    fn refused_while_busy(&mut self, label: &str) -> bool {
        let Some(busy) = &self.busy_op else {
            return false;
        };
        self.status = Some((
            format!("{label}: not done, {busy} was still running. Try again once it's done."),
            true,
        ));
        true
    }

    /// Says how the revert went, and makes one that went in as it was undoable.
    fn reverted(
        &mut self,
        label: &str,
        outcome: Result<(String, Reverted), String>,
        ctx: &egui::Context,
    ) {
        self.busy_op = None;
        self.op_started = None;
        self.status = Some(match outcome {
            Ok((file, Reverted::Clean)) => {
                // Applying the same patch forward brings the changes back.
                // Whatever `apply.whitespace` says: the revert itself ignored it too.
                let undo = Undo::Custom {
                    undo: ops(&[&["apply", "--whitespace=nowarn", &file]]),
                    redo: ops(&[&["apply", "--reverse", "--whitespace=nowarn", &file]]),
                };
                self.record_undo(label, OpKind::Normal(undo, self.refs.head.clone()), ctx);
                (
                    format!(
                        "{label}: done. The change is in the working tree, to look over before \
                         committing (Ctrl+Z undoes it)."
                    ),
                    false,
                )
            }
            Ok((_, Reverted::Merged)) => (
                format!(
                    "{label}: the file has changed since, so the revert was merged in with a \
                     3-way merge and staged. Look it over before committing; Ctrl+Z can't undo \
                     this one (unstage the file and discard its changes instead)."
                ),
                false,
            ),
            Ok((_, Reverted::Conflicts(files))) => {
                self.focus_wip = true;
                let files = if files.is_empty() {
                    crate::i18n::text("repo-core-2511").to_owned()
                } else {
                    files.join(", ")
                };
                (
                    format!(
                        "{label}: conflicts with changes made since, marked in {files}. Settle \
                         them under Conflicted, or discard the file's changes to drop the revert."
                    ),
                    true,
                )
            }
            Err(e) => (crate::i18n::message("repo-core-0270", &[("label", format!("{}", label).into()), ("e", format!("{}", e).into())]), true),
        });
        self.reload_status(ctx);
    }
}
