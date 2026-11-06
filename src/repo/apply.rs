//! Carrying out the actions the views asked for.

use std::path::Path;
use std::time::Instant;

use super::{
    Action, Compare, DiffKey, DiffView, Msg, OpKind, OpenRequest, Refresh, RepoTab, Undo, diff,
    ops, spawn,
};
use crate::git;
use crate::settings::Settings;

impl RepoTab {
    pub(crate) fn apply(&mut self, action: Action, ctx: &egui::Context) {
        let plain = OpKind::plain();
        match action {
            Action::Select(row) => {
                self.close_pr();
                self.ci_view = None;
                self.select(row, ctx);
            }
            Action::SelectHash(hash) => {
                self.close_pr();
                self.ci_view = None;
                self.goto(&hash, ctx);
            }
            Action::SelectStash(i) => {
                self.close_pr();
                self.ci_view = None;
                self.select_stash(i, ctx);
            }
            Action::OpenDiff(key) => self.open_diff(key, ctx),
            Action::OpenPr(number) => {
                // It refuses while an editor takes the big area: the builds shown stay then.
                let requests = self.pr_requests;
                self.open_pr(number, ctx);
                if self.pr_requests != requests {
                    self.ci_view = None;
                }
            }
            Action::OpenPrWorktree {
                number,
                branch,
                sha,
            } => self.open_pr_worktree(number, &branch, &sha, ctx),
            Action::ClosePr => {
                self.close_pr();
                self.diff = None;
            }
            Action::ReloadPr => self.load_pr(ctx),
            Action::Pr(act) => self.pr_act(act, ctx),
            Action::OpenCi { sha, run } => self.open_ci(sha, run, ctx),
            Action::CloseCi => self.ci_view = None,
            Action::Ci(act) => self.ci_act(act, ctx),
            Action::CloseDiff => {
                self.diff = None;
                self.conflict = None;
            }
            Action::Ops {
                label,
                ops,
                refresh,
            } => self.run_ops(&label, ops, refresh, plain, ctx),
            Action::UndoableOps {
                label,
                ops,
                refresh,
                undo,
            } => self.run_undoable(&label, ops, refresh, undo, ctx),
            Action::Checkout(name) => self.checkout(&name, ctx),
            Action::CheckoutPull(name) => self.checkout_pull(&name, ctx),
            Action::CheckoutRemote(name) => self.checkout_remote(&name, ctx),
            Action::Dialog(dialog) => self.dialog = Some(dialog),
            Action::OpenDrift(path) => self.open_drift(&path, ctx),
            Action::DeleteTag { name, target } => self.delete_tag(name, target, ctx),
            Action::Commit => self.commit(ctx),
            Action::Fetch => {
                self.last_fetch = Instant::now();
                self.run_ops(
                    "Fetch",
                    ops(&[&["fetch", "--all", "--prune"]]),
                    Refresh::All,
                    plain,
                    ctx,
                )
            }
            Action::Pull => self.pull(Settings::get(ctx).pull_mode, ctx),
            Action::PullWith(mode) => self.pull(mode, ctx),
            Action::Push => self.push(ctx),
            Action::Reload => self.reload(ctx),
            Action::LoadMore => self.load_more(ctx),
            Action::Undo => self.undo(ctx),
            Action::Redo => self.redo(ctx),
            Action::StashPush(message) => self.stash_push(&message, ctx),
            Action::OpenTab(path) => self.open_request = Some(OpenRequest::NewTab(path)),
            Action::OpenWorktree(path) => self.open_request = Some(OpenRequest::Worktree(path)),
            Action::ToggleBranches => {
                let mut settings = Settings::get(ctx);
                settings.show_branches = !settings.show_branches;
                settings.set(ctx);
            }
            Action::BranchVisibility(change) => self.change_visibility(change, ctx),
            Action::OpenBranches(section) => {
                let mut settings = Settings::get(ctx);
                settings.show_branches = true;
                settings.set(ctx);
                match section {
                    Some(key) => self.sidebar_reveal = Some(key),
                    None => self.focus_branch_filter = true,
                }
            }
            Action::ToggleDetail => {
                let mut settings = Settings::get(ctx);
                settings.show_detail = !settings.show_detail;
                settings.set(ctx);
            }
            Action::ToggleTerminal => self.toggle_terminal(ctx),
            Action::NewTerminal { shell, cwd } => self.start_terminal(shell, cwd, ctx),
            Action::CloseTerminal(i) => self.close_terminal(i),
            Action::SelectTerminal(i) => self.active_terminal = i,
            Action::ToggleSearch => self.toggle_search(),
            Action::SearchStep(d) => self.search_step(d, ctx),
            Action::CreatePr {
                branch,
                base,
                title,
                body,
                draft,
            } => self.create_pr(branch, base, title, body, draft, ctx),
            Action::OpenUrl(url) => {
                if crate::pull_request::is_web_url(&url) {
                    let _ = open::that_detached(url);
                }
            }
            Action::OpenPath(rel) => {
                if let Err(e) = crate::remote::open_here(crate::remote::join(
                    &self.path,
                    &rel.to_string_lossy(),
                )) {
                    self.status = Some((e, true));
                }
            }
            Action::OpenInEditor(rel) => {
                let editor = Settings::get(ctx).editor;
                if let Err(e) = open_in_editor(
                    &editor,
                    &crate::remote::join(&self.path, &rel.to_string_lossy()),
                ) {
                    self.status = Some((e, true));
                }
            }
            Action::SetIdentity(name, email) => self.run_ops(
                "Set identity",
                ops(&[
                    &["config", "--local", "user.name", &name],
                    &["config", "--local", "user.email", &email],
                ]),
                Refresh::All,
                plain,
                ctx,
            ),
            Action::ClearIdentity { name, email } => {
                let mut list: Vec<&[&str]> = Vec::new();
                if name {
                    list.push(&["config", "--local", "--unset-all", "user.name"]);
                }
                if email {
                    list.push(&["config", "--local", "--unset-all", "user.email"]);
                }
                if !list.is_empty() {
                    self.run_ops(
                        "Use the profile's author",
                        ops(&list),
                        Refresh::All,
                        plain,
                        ctx,
                    );
                }
            }
            Action::SetDiffOptions {
                split,
                ignore_ws,
                whole_file,
            } => self.set_diff_options(split, ignore_ws, whole_file, ctx),
            Action::DiffAsFile(on) => self.show_diff_as_file(on, ctx),
            Action::DiffRendered(on) => self.show_diff_rendered(on, ctx),
            Action::DiffStep(delta) => {
                let rendered = self.diff_rendered
                    && self
                        .diff
                        .as_ref()
                        .is_some_and(|v| diff::rendered::is_markdown(v.key.path()));
                if let Some(view) = self
                    .diff
                    .as_mut()
                    .filter(|_| !self.diff_as_file && !rendered)
                {
                    view.step_change(Settings::get(ctx).diff_split, delta);
                }
            }
            Action::ViewAllFiles(on) => self.all_files.on = on,
            Action::FileView { path, rev, tab } => self.open_file_view(path, rev, tab, ctx),
            Action::CloseFileView => self.file_view = None,
            Action::InteractiveRebase { onto, label } => self.interactive_rebase(onto, label, ctx),
            Action::Reword { hash, message } => self.reword_commit(hash, message, ctx),
            Action::EditMessage(hash) => self.edit_message(hash, ctx),
            Action::SavePatch { hash, summary } => self.save_patch(hash, &summary, ctx),
            Action::ApplyPatch => self.apply_patch(ctx),
            Action::QuickEdit { edit, hash } => self.quick_edit(edit, hash, ctx),
            Action::RunRebase => self.run_rebase(ctx),
            Action::CloseRebase => self.rebase = None,
            Action::SignIn(creds) => self.sign_in(creds, ctx),
            Action::Ignore { pattern, path } => self.ignore(&pattern, &path, ctx),
            Action::PickLines { pick, lines } => self.pick_lines(pick, &lines, ctx),
            Action::Compare {
                base,
                base_label,
                target,
                target_label,
                since_fork,
            } => self.compare(
                Compare {
                    base,
                    base_label,
                    target,
                    target_label,
                    since_fork,
                    data: None,
                    request: 0,
                },
                ctx,
            ),
            Action::CompareSinceFork(on) => {
                if let Some(c) = &mut self.compare {
                    c.since_fork = on;
                    self.reload_compare(ctx);
                }
            }
            Action::SwapCompare => self.swap_compare(ctx),
            Action::CloseCompare => {
                self.compare = None;
                self.close_range_diff();
            }
            Action::SearchCode(on) => {
                if self.search.code != on {
                    self.search.code = on;
                    self.run_search(ctx);
                }
                self.search.focus = true;
            }
            Action::SolveWithMergiraf => self.solve_with_mergiraf(ctx),
            Action::SaveConflict => self.save_conflict(ctx),
        }
    }

    /// Side by side and the whole file are kept for next time; whitespace is per tab.
    fn set_diff_options(
        &mut self,
        split: bool,
        ignore_ws: bool,
        whole_file: bool,
        ctx: &egui::Context,
    ) {
        let mut settings = Settings::get(ctx);
        let reload = ignore_ws != self.diff_ignore_ws || whole_file != settings.diff_whole_file;
        settings.diff_split = split;
        settings.diff_whole_file = whole_file;
        settings.set(ctx);
        self.diff_ignore_ws = ignore_ws;
        if reload {
            self.reopen_diff(ctx);
        }
    }

    /// Adds `pattern` to `.gitignore`, and stops tracking `path` if git tracks it.
    fn ignore(&mut self, pattern: &str, path: &str, ctx: &egui::Context) {
        // On another machine, the file is read and written there, in the background.
        let (repo, pattern, path) = (self.path.clone(), pattern.to_owned(), path.to_owned());
        self.here_or_later(
            ctx,
            move || {
                let added = git::add_to_gitignore(&repo, &pattern);
                let tracked = added.is_ok() && git::is_tracked(&repo, &path);
                (pattern, path, added, tracked)
            },
            |tab, (pattern, path, added, tracked), ctx| {
                tab.ignored(&pattern, &path, added, tracked, ctx);
            },
        );
    }

    /// Stops tracking `path` once `pattern` is in `.gitignore`, if it was tracked.
    fn ignored(
        &mut self,
        pattern: &str,
        path: &str,
        added: Result<bool, String>,
        tracked: bool,
        ctx: &egui::Context,
    ) {
        match added {
            Err(e) => self.status = Some((e, true)),
            Ok(added) => {
                if tracked {
                    self.run_prepared(
                        &format!("Ignore {pattern}"),
                        &format!("{pattern} is in .gitignore, but {path} is still tracked"),
                        ops(&[&[
                            "--literal-pathspecs",
                            "rm",
                            "-r",
                            "--cached",
                            "--quiet",
                            "--",
                            path,
                        ]]),
                        Refresh::Status,
                        OpKind::plain(),
                        ctx,
                    );
                } else {
                    let msg = if added {
                        format!("Added {pattern} to .gitignore")
                    } else {
                        format!("{pattern} was already in .gitignore")
                    };
                    self.status = Some((msg, false));
                    self.reload_status(ctx);
                }
            }
        }
    }

    /// Stages, unstages or discards the chosen lines of the open working-tree diff.
    fn pick_lines(
        &mut self,
        pick: diff::Pick,
        lines: &std::collections::BTreeSet<usize>,
        ctx: &egui::Context,
    ) {
        let Some(DiffView {
            key: DiffKey::Work(file),
            data: Some(Ok(data)),
            ..
        }) = &self.diff
        else {
            return;
        };
        let Some(patch) = diff::partial_patch(&data.header, &data.lines, lines, pick) else {
            return;
        };
        let n = lines.len();
        let label = format!(
            "{} {n} line{} of {}",
            pick.verb(),
            if n == 1 { "" } else { "s" },
            file.path
        );
        // Written where git runs: on another machine, in the background.
        let repo = self.path.clone();
        self.here_or_later(
            ctx,
            move || git::save_patch_for(&repo, &patch),
            move |tab, saved, ctx| tab.apply_picked(pick, &label, saved, ctx),
        );
    }

    /// Applies the patch of lines picked, saved at `saved` (see [`Self::pick_lines`]).
    fn apply_picked(
        &mut self,
        pick: diff::Pick,
        label: &str,
        saved: Result<String, String>,
        ctx: &egui::Context,
    ) {
        let patch = match saved {
            Ok(p) => p,
            Err(e) => {
                self.status = Some((e, true));
                return;
            }
        };
        let mut args = pick.args().to_vec();
        args.push(&patch);
        let kind = if pick == diff::Pick::Discard {
            // Applying the same patch forward brings the lines back.
            let undo = Undo::Custom {
                undo: ops(&[&["apply", &patch]]),
                redo: ops(&[&args]),
            };
            OpKind::Normal(undo, self.refs.head.clone())
        } else {
            OpKind::plain()
        };
        self.run_prepared(
            label,
            &format!("{label}: not done"),
            ops(&[&args]),
            Refresh::Status,
            kind,
            ctx,
        );
    }

    /// Asks Mergiraf to solve what it can of the open conflict.
    fn solve_with_mergiraf(&mut self, ctx: &egui::Context) {
        if let Some(ed) = self.conflict.as_mut().filter(|ed| !ed.solving) {
            ed.solving = true;
            let (repo, file) = (self.path.clone(), ed.path.clone());
            spawn(&self.tx, ctx, move || {
                let result = crate::mergiraf::solve(&repo, &file);
                Msg::Mergiraf(file, result)
            });
        }
    }

    /// Takes Mergiraf's answer into the conflict editor (if it's still on that file), and says
    /// how much it solved.
    pub(super) fn mergiraf_done(&mut self, path: String, result: Result<String, String>) {
        let Some(ed) = self.conflict.as_mut().filter(|ed| ed.path == path) else {
            return;
        };
        ed.solving = false;
        self.status = Some(match result {
            Ok(text) => {
                let before = ed.on_disk;
                let left = ed.take_solved(text);
                let solved = before.saturating_sub(left);
                let message = match (solved, left) {
                    (0, _) => format!("Mergiraf could not solve the conflicts in {path}"),
                    (_, 0) => format!(
                        "Mergiraf solved every conflict in {path}: check the result, then save"
                    ),
                    _ => format!(
                        "Mergiraf solved {solved} of {before} conflicts in {path}; {left} left \
                         to choose"
                    ),
                };
                (message, false)
            }
            Err(e) => (e, true),
        });
    }

    /// Writes the conflict editor's file and marks it resolved.
    fn save_conflict(&mut self, ctx: &egui::Context) {
        let Some(editor) = &mut self.conflict else {
            return;
        };
        // Elsewhere, written in the background (see `conflict_saved`), one save at a time: the
        // editor waits (see `ConflictEditor::saving`) so a later one can't be overtaken.
        if crate::remote::is_remote(&self.path) {
            if editor.saving {
                return;
            }
            match editor.solved() {
                Ok(text) => {
                    editor.saving = true;
                    let (repo, path) = (self.path.clone(), editor.path.clone());
                    spawn(&self.tx, ctx, move || {
                        let saved = super::conflict::save_solved(&repo, &path, &text);
                        Msg::ConflictSaved(path, saved)
                    });
                }
                Err(e) => self.status = Some((e, true)),
            }
            return;
        }
        let saved = editor.save(&self.path);
        let file = editor.path.clone();
        self.conflict_saved(file, saved, ctx);
    }

    /// Marks `file` resolved once its conflicts were saved.
    pub(super) fn conflict_saved(
        &mut self,
        file: String,
        saved: Result<(), String>,
        ctx: &egui::Context,
    ) {
        match saved {
            Ok(()) => self.run_prepared(
                "Mark resolved",
                &format!("Saved {file}, but not marked resolved"),
                ops(&[&["--literal-pathspecs", "add", "--", &file]]),
                Refresh::Status,
                OpKind::plain(),
                ctx,
            ),
            Err(e) => self.status = Some((e, true)),
        }
    }

    /// Runs `ops`, the git step that follows what was just done (on another machine, in the
    /// background meanwhile). When another operation started in the meantime, it can't: the
    /// user is told (`left`: what is left undone) rather than left to think it ran.
    #[allow(clippy::too_many_arguments)]
    fn run_prepared(
        &mut self,
        label: &str,
        left: &str,
        ops: Vec<Vec<String>>,
        refresh: Refresh,
        kind: OpKind,
        ctx: &egui::Context,
    ) {
        if let Some(busy) = &self.busy_op {
            self.status = Some((
                format!("{left}: {busy} was still running. Try again once it's done."),
                true,
            ));
            return;
        }
        self.run_ops(label, ops, refresh, kind, ctx);
    }
}

/// Runs the user's editor command with `path` (through `cmd` on Windows, where editors are
/// usually `.cmd` scripts).
pub(crate) fn open_in_editor(editor: &str, path: &Path) -> Result<(), String> {
    crate::remote::openable_here(path)?;
    let editor = if editor.trim().is_empty() {
        "code"
    } else {
        editor.trim()
    };
    #[cfg(windows)]
    let mut cmd = {
        use std::os::windows::process::CommandExt;
        let mut c = std::process::Command::new("cmd");
        // The path in quotes and through a variable: a file named `a&calc` must not run `calc`,
        // and one named `%COMSPEC%` must not open another (cmd expands `%…%` even in quotes,
        // but not again inside what a variable held). `/S` takes off only the outer quotes,
        // leaving `editor "path"`.
        let mut target = path.as_os_str().to_owned();
        // A backslash before the closing quote would escape it for the editor (the folder
        // itself ends with one): doubled, it is just a backslash.
        if target.to_string_lossy().ends_with('\\') {
            target.push("\\");
        }
        c.env("GITCRAFT_OPEN_PATH", target);
        let mut line = std::ffi::OsString::from("/S /C \"");
        line.push(editor);
        line.push(" \"%GITCRAFT_OPEN_PATH%\"\"");
        c.raw_arg(line);
        c.creation_flags(0x0800_0000);
        c
    };
    #[cfg(not(windows))]
    let mut cmd = {
        // A command with arguments (`code -n`, `flatpak run …`), as `cmd` reads it on Windows:
        // through the shell, the path still one argument (`$1`) it does not read. A program
        // whose own path has a space in it is run as it is.
        let mut c = if editor.contains(char::is_whitespace) && !Path::new(editor).exists() {
            let mut c = std::process::Command::new("sh");
            c.arg("-c").arg(format!("{editor} \"$1\"")).arg("sh");
            c
        } else {
            std::process::Command::new(editor)
        };
        c.arg(path);
        crate::shell_path::apply(&mut c);
        c
    };
    let mut child = cmd
        .spawn()
        .map_err(|e| format!("Could not run {editor}: {e}"))?;
    // Reaped once it ends (most editor commands hand the file over and exit at once), or each
    // file opened would leave a zombie process for as long as the app runs.
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(())
}
