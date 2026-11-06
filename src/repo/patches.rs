//! Patches: saving a commit as one (`git format-patch`), and applying one from a file. A mail
//! like format-patch writes becomes commits (`git am`), any other diff goes on the working tree
//! (`git apply`).

use std::path::{Path, PathBuf};

use super::{Msg, Refresh, RepoTab, Undo, ops, spawn};
use crate::git;

impl RepoTab {
    /// Asks where to save the patch of commit `hash`, then writes it there.
    pub(super) fn save_patch(&mut self, hash: String, summary: &str, ctx: &egui::Context) {
        let mut dialog = rfd::FileDialog::new().set_title("Save patch");
        // Saved on this machine: next to the repository when it is here (a folder in WSL
        // would make the dialog wait for WSL).
        if !crate::remote::is_remote(&self.path) {
            dialog = dialog.set_directory(&self.path);
        }
        let Some(file) = dialog
            .set_file_name(patch_file_name(summary))
            .add_filter("Patch", &["patch"])
            .save_file()
        else {
            return;
        };
        let path = self.path.clone();
        spawn(&self.tx, ctx, move || {
            let result = git::run_bytes(Some(&path), &["format-patch", "-1", "--stdout", &hash])
                .and_then(|patch| {
                    std::fs::write(&file, patch).map_err(|e| format!("could not save it: {e}"))
                });
            Msg::PatchSaved(file, result)
        });
    }

    pub(super) fn patch_saved(&mut self, file: PathBuf, result: Result<(), String>) {
        self.status = Some(match result {
            Ok(()) => (format!("Patch saved to {}", file.display()), false),
            Err(e) => (format!("Create patch failed: {e}"), true),
        });
    }

    /// Asks for a patch file and applies it, undoably.
    pub(super) fn apply_patch(&mut self, ctx: &egui::Context) {
        let mut dialog = rfd::FileDialog::new().set_title("Apply patch");
        if !crate::remote::is_remote(&self.path) {
            dialog = dialog.set_directory(&self.path);
        }
        let Some(file) = dialog
            .add_filter("Patch", &["patch", "diff", "eml", "mbox"])
            .add_filter("All files", &["*"])
            .pick_file()
        else {
            return;
        };
        self.apply_patch_from(file, ctx);
    }

    /// Applies the patch file `file` (of this machine) to the repository, undoably: a mail as
    /// commits (`git am`), a bare diff on the working tree (`git apply`). For a patch dropped on
    /// the window; the outcome shows in the tab's status line.
    pub fn apply_dropped_patch(&mut self, file: PathBuf, ctx: &egui::Context) {
        if let Some(busy) = &self.busy_op {
            self.status = Some((
                format!("Apply {}: wait until {busy} is done", file_name(&file)),
                true,
            ));
            return;
        }
        self.status = None;
        self.apply_patch_from(file, ctx);
    }

    /// Whether a patch given to [`Self::apply_dropped_patch`] is still being applied (or put on
    /// the repository's machine first), or the tab is busy otherwise: another has to wait.
    pub fn applying_patch(&self) -> bool {
        self.patch_pending || self.is_busy()
    }

    /// Whether the last thing done in the tab failed (its status line says why).
    pub fn last_failed(&self) -> bool {
        matches!(self.status, Some((_, true)))
    }

    /// Reads `file` and applies it (see [`Self::apply_dropped_patch`]).
    fn apply_patch_from(&mut self, file: PathBuf, ctx: &egui::Context) {
        // Copied as it is (a patch can carry bytes of any encoding); read as text only to tell
        // a mail from a bare diff.
        let bytes = match std::fs::read(&file) {
            Ok(bytes) => bytes,
            Err(e) => {
                self.status = Some((format!("Apply patch: could not read it: {e}"), true));
                return;
            }
        };
        let label = format!("Apply {}", file_name(&file));
        let mail = is_mail(&String::from_utf8_lossy(&bytes));
        // For a repository on another machine, git there reads a copy put there (in the
        // background).
        let repo = self.path.clone();
        self.patch_pending = true;
        self.here_or_later(
            ctx,
            move || match crate::remote::parse(&repo) {
                Some(target) => crate::remote::host_temp_file(&target, "apply.patch", &bytes),
                None => Ok(file.to_string_lossy().into_owned()),
            },
            move |tab, file, ctx| {
                tab.patch_pending = false;
                match file {
                    Ok(file) => tab.apply_patch_file(&label, &file, mail, ctx),
                    Err(e) => tab.status = Some((format!("Apply patch: {e}"), true)),
                }
            },
        );
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
