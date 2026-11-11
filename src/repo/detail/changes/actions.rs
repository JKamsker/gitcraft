//! Typed row outcomes and staging/discard command construction.

use super::*;

pub(super) enum RowClick {
    None,
    Open,
    Button(usize),
    /// Ctrl+click (Cmd on macOS): picked to stash, or unpicked.
    Pick,
}

pub(in crate::repo) fn stage_action(label: &str, steps: Vec<Vec<&str>>) -> Action {
    Action::Ops {
        label: label.to_owned(),
        ops: steps
            .into_iter()
            .map(|s| s.into_iter().map(str::to_owned).collect())
            .collect(),
        refresh: Refresh::Status,
    }
}

/// `git reset` of every path, or `git rm --cached` of them all before the first commit (no
/// HEAD to reset to).
pub(super) fn unstage_all_args(head: &str) -> Vec<&'static str> {
    if head.is_empty() {
        vec![
            "--literal-pathspecs",
            "rm",
            "--cached",
            "-r",
            "-q",
            "--",
            ".",
        ]
    } else {
        // With a path: a bare `reset` would also end the merge or cherry-pick in progress
        // (MERGE_HEAD gone, and its commit no merge).
        vec!["--literal-pathspecs", "reset", "-q", "--", "."]
    }
}

pub(in crate::repo) fn discard_dialog(f: &StatusFile) -> Dialog {
    if f.area == Area::Untracked {
        Dialog::confirm(
            "Delete new file",
            crate::i18n::message("repo-core-0852", &[("path", f.path.to_string().into())]),
            crate::i18n::text("repo-core-0853"),
            ops(&[&["--literal-pathspecs", "clean", "-f", "-q", "--", &f.path]]),
            None,
        )
        .refresh(Refresh::Status)
        .preview(
            crate::git::PreviewAsk::files(vec![f.path.clone()]),
            String::new(),
        )
        .files_heading(crate::i18n::text("repo-core-0855"))
    } else {
        Dialog::confirm(
            "Discard changes",
            crate::i18n::message("repo-core-0857", &[("path", f.path.to_string().into())]),
            crate::i18n::text("repo-core-0858"),
            // Not `--literal-pathspecs`: restore runs the post-checkout hook, which would get it.
            ops(&[&["restore", "--", &literal_path(&f.path)]]),
            None,
        )
        .refresh(Refresh::Status)
        .preview(
            crate::git::PreviewAsk::files(vec![f.path.clone()]),
            String::new(),
        )
        .files_heading(crate::i18n::text("repo-core-0860"))
    }
}
