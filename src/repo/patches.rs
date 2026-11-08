//! Patches: saving a commit as one (`git format-patch`), and applying one from a file. A mail
//! like format-patch writes becomes commits (`git am`), any other diff goes on the working tree
//! (`git apply`).

use std::path::{Path, PathBuf};

use super::{Msg, Refresh, RepoTab, Undo, ops, spawn};
use crate::git;

impl RepoTab {
    /// Asks where to save the patch of commit `hash`, then writes it there.
    pub(super) fn save_patch(&mut self, hash: String, summary: &str, ctx: &egui::Context) {
        let mut dialog = rfd::FileDialog::new().set_title(crate::i18n::text("repo-core-3012"));
        // Saved on this machine: next to the repository when it is here (a folder in WSL
        // would make the dialog wait for WSL).
        if !crate::remote::is_remote(&self.path) {
            dialog = dialog.set_directory(&self.path);
        }
        let Some(file) = dialog
            .set_file_name(patch_file_name(summary))
            .add_filter(crate::i18n::text("repo-core-3013"), &["patch"])
            .save_file()
        else {
            return;
        };
        let path = self.path.clone();
        spawn(&self.tx, ctx, move || {
            let result = git::run_to_file(&path, &["format-patch", "-1", "--stdout", &hash], &file);
            Msg::PatchSaved(file, result)
        });
    }

    pub(super) fn patch_saved(&mut self, file: PathBuf, result: Result<(), String>) {
        self.status = Some(match result {
            Ok(()) => (
                crate::i18n::message(
                    "repo-core-3016",
                    &[("file", format!("{}", file.display()).into())],
                ),
                false,
            ),
            Err(e) => (
                crate::i18n::message("repo-core-3017", &[("e", format!("{}", e).into())]),
                true,
            ),
        });
    }

    /// Asks for a patch file and applies it, undoably.
    pub(super) fn apply_patch(&mut self, ctx: &egui::Context) {
        let mut dialog = rfd::FileDialog::new().set_title(crate::i18n::text("repo-core-3018"));
        if !crate::remote::is_remote(&self.path) {
            dialog = dialog.set_directory(&self.path);
        }
        let Some(file) = dialog
            .add_filter(
                crate::i18n::text("repo-core-3013"),
                &["patch", "diff", "eml", "mbox"],
            )
            .add_filter(crate::i18n::text("repo-core-3021"), &["*"])
            .pick_file()
        else {
            return;
        };
        self.apply_patch_from(file, false, ctx);
    }

    /// Applies the patch file `file` (of this machine) to the repository, undoably: a mail as
    /// commits (`git am`), a bare diff on the working tree (`git apply`). For a patch dropped on
    /// the window; how it went is told by [`Self::take_dropped_patch_outcome`]. Refused while
    /// the tab runs an operation or applies another dropped patch.
    pub fn apply_dropped_patch(
        &mut self,
        file: PathBuf,
        ctx: &egui::Context,
    ) -> Result<(), String> {
        if let Some(busy) = &self.busy_op {
            return Err(crate::i18n::message(
                "repo-core-3022",
                &[
                    ("name", format!("{}", self.name).into()),
                    (
                        "busy",
                        crate::repo::messages::display_operation(&busy).into(),
                    ),
                ],
            ));
        }
        if matches!(
            self.dropped_patch,
            DroppedPatch::Preparing | DroppedPatch::Running(_)
        ) {
            return Err(crate::i18n::message(
                "repo-core-3023",
                &[("name", format!("{}", self.name).into())],
            ));
        }
        self.status = None;
        self.dropped_patch = DroppedPatch::Preparing;
        self.apply_patch_from(file, true, ctx);
        Ok(())
    }

    /// Whether a dropped patch given now would have to wait: the tab runs an operation, or
    /// applies the patch dropped before.
    pub fn dropped_patch_waits(&self) -> bool {
        self.busy_op.is_some()
            || matches!(
                self.dropped_patch,
                DroppedPatch::Preparing | DroppedPatch::Running(_)
            )
    }

    /// How the dropped patch went, once it is done (taken: asked once).
    pub fn take_dropped_patch_outcome(&mut self) -> Option<Result<(), String>> {
        match std::mem::take(&mut self.dropped_patch) {
            DroppedPatch::Done(result) => Some(result),
            other => {
                self.dropped_patch = other;
                None
            }
        }
    }

    /// The operation `label` ended: when it applied the dropped patch, that is how it went.
    pub(super) fn dropped_patch_done(&mut self, label: &str, result: &Result<String, String>) {
        if matches!(&self.dropped_patch, DroppedPatch::Running(l) if l == label) {
            self.dropped_patch = DroppedPatch::Done(result.clone().map(|_| ()));
        }
    }

    /// Reads `file` and applies it (see [`Self::apply_dropped_patch`]); `dropped` keeps track
    /// of how it goes.
    fn apply_patch_from(&mut self, file: PathBuf, dropped: bool, ctx: &egui::Context) {
        // Copied as it is (a patch can carry bytes of any encoding); read as text only to tell
        // a mail from a bare diff.
        let bytes = match std::fs::read(&file) {
            Ok(bytes) => bytes,
            Err(e) => {
                let why = crate::i18n::message(
                    "repo-core-3024",
                    &[
                        ("file", format!("{}", file_name(&file)).into()),
                        ("e", format!("{}", e).into()),
                    ],
                );
                self.status = Some((
                    crate::i18n::message("repo-core-3025", &[("why", format!("{}", why).into())]),
                    true,
                ));
                if dropped {
                    self.dropped_patch = DroppedPatch::Done(Err(why));
                }
                return;
            }
        };
        let label = format!("Apply {}", file_name(&file));
        let mail = is_mail(&String::from_utf8_lossy(&bytes));
        // For a repository on another machine, git there reads a copy put there (in the
        // background). A panic there still comes back as an error, so that a dropped patch
        // isn't waited for forever.
        let repo = self.path.clone();
        self.here_or_later(
            ctx,
            move || {
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(
                    || match crate::remote::parse(&repo) {
                        Some(target) => {
                            crate::remote::host_temp_file(&target, "apply.patch", &bytes)
                        }
                        None => Ok(file.to_string_lossy().into_owned()),
                    },
                ))
                .unwrap_or_else(|_| Err(crate::i18n::text("repo-core-3028").to_owned()))
            },
            move |tab, file, ctx| tab.apply_prepared_patch(label, file, mail, dropped, ctx),
        );
    }

    /// The patch is where git reads it (`file`), or couldn't be put there: applies it.
    fn apply_prepared_patch(
        &mut self,
        label: String,
        file: Result<String, String>,
        mail: bool,
        dropped: bool,
        ctx: &egui::Context,
    ) {
        let started = match file {
            Err(e) => Err(e),
            // Another operation started meanwhile: it would not start, silently.
            Ok(_) if self.busy_op.is_some() => Err(crate::i18n::message(
                "repo-core-3029",
                &[(
                    "self",
                    format!("{}", self.busy_op.clone().unwrap_or_default()).into(),
                )],
            )),
            Ok(file) => {
                self.apply_patch_file(&label, &file, mail, ctx);
                Ok(())
            }
        };
        match started {
            Ok(()) if dropped => self.dropped_patch = DroppedPatch::Running(label),
            Ok(()) => {}
            Err(e) => {
                self.status = Some((
                    crate::i18n::message(
                        "repo-core-0270",
                        &[
                            (
                                "label",
                                crate::repo::messages::display_operation(&label).into(),
                            ),
                            ("e", format!("{}", e).into()),
                        ],
                    ),
                    true,
                ));
                if dropped {
                    self.dropped_patch = DroppedPatch::Done(Err(e));
                }
            }
        }
    }

    /// Applies the patch at `file` (as git reads it), as commits when it is a `mail`.
    fn apply_patch_file(&mut self, label: &str, file: &str, mail: bool, ctx: &egui::Context) {
        let label = label.to_owned();
        if mail {
            self.run_undoable(
                &label,
                ops(&[&["am", "--3way", file]]),
                Refresh::All,
                Undo::HeadMove { mode: "--keep" },
                ctx,
            );
        } else {
            self.run_undoable(
                &label,
                ops(&[&["apply", file]]),
                Refresh::All,
                Undo::Custom {
                    undo: ops(&[&["apply", "-R", file]]),
                    redo: ops(&[&["apply", file]]),
                },
                ctx,
            );
        }
    }
}

/// Where a patch dropped on the window is.
#[derive(Debug, Default)]
pub(super) enum DroppedPatch {
    #[default]
    None,
    /// Being read, or copied to the repository's machine.
    Preparing,
    /// Being applied by the operation so labelled.
    Running(String),
    /// Done: how it went.
    Done(Result<(), String>),
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}

/// The name `git format-patch` gives a commit's patch: `0001-` and the summary with anything but
/// letters, digits, `.` and `_` turned into single dashes.
fn patch_file_name(summary: &str) -> String {
    let mut name = String::new();
    for c in summary.chars() {
        if c.is_ascii_alphanumeric() || c == '.' || c == '_' {
            name.push(c);
        } else if !name.is_empty() && !name.ends_with('-') {
            name.push('-');
        }
    }
    let name: String = name.trim_end_matches(['-', '.']).chars().take(53).collect();
    let name = name.trim_end_matches(['-', '.']);
    if name.is_empty() {
        "0001.patch".to_owned()
    } else {
        format!("0001-{name}.patch")
    }
}

/// Whether `patch` is a mail (as `git format-patch` writes them), with the author and message
/// for `git am`, rather than a bare diff.
fn is_mail(patch: &str) -> bool {
    let head = patch.split("\ndiff --git ").next().unwrap_or(patch);
    let mut lines = head.lines().skip_while(|l| l.trim().is_empty());
    let from = lines.next().is_some_and(|l| l.starts_with("From "));
    from && head.lines().any(|l| l.starts_with("Subject:"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn patch_names_like_git() {
        assert_eq!(
            patch_file_name("Fix the parser: don't panic!"),
            "0001-Fix-the-parser-don-t-panic.patch"
        );
        assert_eq!(patch_file_name("v1.2 release."), "0001-v1.2-release.patch");
        assert_eq!(patch_file_name("???"), "0001.patch");
        assert!(patch_file_name(&"a".repeat(200)).len() <= 64);
    }

    #[test]
    fn mails_and_diffs() {
        let mail = "From 5356c75e Mon Sep 17 00:00:00 2001\nFrom: t <t@t>\nDate: Thu, 5 Nov 2026\nSubject: [PATCH] c2\n\n---\n f2 | 1 +\n\ndiff --git a/f2 b/f2\n";
        assert!(is_mail(mail));
        let diff = "diff --git a/f2 b/f2\nindex 0..1\n--- a/f2\n+++ b/f2\n";
        assert!(!is_mail(diff));
        // A diff that adds a line "From …" to a file isn't a mail.
        let tricky = "diff --git a/f b/f\n+From here on\n+Subject: x\n";
        assert!(!is_mail(tricky));
    }
}
