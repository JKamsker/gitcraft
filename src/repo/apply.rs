//! Carrying out the actions the views asked for.

use std::path::Path;
use std::time::Instant;

use super::detached::Leave;
use super::{
    Action, Compare, DiffKey, DiffView, Msg, OpKind, OpenRequest, Refresh, RepoTab, Undo, diff,
    guide, ops, spawn,
};
use crate::git;
use crate::settings::Settings;

impl RepoTab {
    pub(crate) fn apply(&mut self, action: Action, ctx: &egui::Context) {
        let Some(action) = self.popout_intercept(action, ctx) else {
            return;
        };
        let plain = OpKind::plain();
        // Fetch, Pull and Push asked for while something runs start once it is done.
        if self.busy_op.is_some()
            && let Some(waiter) = super::running::Waiter::of(&action, ctx)
        {
            self.wait_for_running(waiter);
            return;
        }
        let Some(action) = self.fence_gate(action, ctx) else {
            return;
        };
        if matches!(
            &action,
            Action::OpenDiff(_)
                | Action::OpenPr(_)
                | Action::OpenCi { .. }
                | Action::FileView { .. }
        ) {
            self.file_comparison = None;
        }
        match action {
            Action::DvcPull => {
                self.dialog=Some(super::Dialog::confirm("DVC: Pull", "Download the data referenced by this checkout from its configured DVC remote and materialize it in the working tree? DVC must be installed on the repository machine. Review local data edits first; this operation has no automatic undo.".into(), "Pull data",git::dvc::pull_steps(),None));
            }

            Action::OpenFileComparison(page) => self.open_file_comparison(page, ctx),
            Action::RefreshFileComparison => self.load_file_comparison(ctx),
            Action::CloseFileComparison => self.file_comparison = None,
            Action::ComparisonMove { request, index } => self.comparison_move(request, index, ctx),
            Action::ComparisonCell {
                request,
                path,
                column,
            } => self.comparison_cell(request, path, column, ctx),
            Action::ComparisonPick {
                request,
                boundary,
                lines,
                whole,
            } => self.comparison_pick(request, boundary, lines, whole, ctx),
            Action::ComparisonNormalPatch => self.comparison_normal_patch(ctx),
            Action::Select(row) => {
                self.leave_pages();
                self.doctor = None;
                self.select(row, ctx);
                self.hint_after_select(row, ctx);
            }
            Action::SelectHash(hash) => {
                self.leave_pages();
                // Following a link is a step of its own, back and forward.
                self.nav_jump = true;
                self.doctor = None;
                self.goto(&hash, ctx);
            }
            Action::SelectPr(number) => self.select_pr(number, ctx),
            Action::SelectStash(i) => {
                self.leave_pages();
                self.doctor = None;
                self.select_stash(i, ctx);
            }
            Action::OpenDiff(key) => {
                self.open_diff(key, ctx);
                if self.diff.is_some() {
                    self.offer_hint(guide::Hint::ChangeNavigation, ctx);
                }
            }
            Action::Custom(act) => self.custom_act(act, ctx),
            Action::OpenPr(number) => {
                // It refuses while an editor takes the big area: the builds shown stay then.
                let requests = self.pr_requests;
                self.open_pr(number, ctx);
                if self.pr_requests != requests {
                    self.ci_view = None;
                    self.doctor = None;
                    self.bakeoff.view = None;
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
            Action::Versions(act) => self.versions_act(act, ctx),
            Action::OpenCi { sha, run } => {
                self.bakeoff.view = None;
                self.open_ci(sha, run, ctx);
                // Over the doctor, which then closes: the toolbar's wrench says what shows.
                if self.ci_view.is_some() {
                    self.doctor = None;
                }
            }
            Action::CloseCi => self.ci_view = None,
            Action::Ci(act) => self.ci_act(act, ctx),
            Action::Reflog(act) => self.reflog_act(act, ctx),
            Action::CloseReflog => self.close_reflog(ctx),
            Action::Issue(act) => self.issue_act(act, ctx),
            Action::OpenDoctor(section) => self.open_doctor(section, ctx),
            Action::CloseDoctor => self.doctor = None,
            Action::Doctor(act) => self.doctor_act(act, ctx),
            Action::Agent(act) => self.agent_act(act, ctx),
            Action::Bake(act) => self.bake_act(act, ctx),
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
            // From a detached HEAD, only once what that would leave behind is known.
            Action::Checkout(name) => self.leave_to(Leave::Checkout(name), ctx),
            Action::CheckoutPull(name) => self.leave_to(Leave::CheckoutPull(name), ctx),
            Action::CheckoutRemote(name) => self.leave_to(Leave::CheckoutRemote(name), ctx),
            Action::Detached(act) => self.detached_act(act, ctx),
            Action::Dialog(dialog) => self.open_dialog(dialog, ctx),
            Action::SelectStaleBranches => self.dialog = Some(self.cleanup_dialog(ctx)),
            Action::OpenCollisions => self.open_collisions(),
            Action::OpenFence => self.open_fence(ctx),
            Action::OpenDrift(path) => self.open_drift(&path, ctx),
            Action::AgentFilter(on) => self.toggle_agent_filter(on, ctx),
            Action::BranchAuthorship(name) => self.branch_authorship(&name, ctx),
            Action::DeleteTag { name, target } => self.delete_tag(name, target, ctx),
            Action::Commit => self.commit(ctx),
            Action::Key(command) => self.run_key(command, ctx),
            Action::FocusCommit => self.focus_commit(ctx),
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
            Action::PushSkippingHooks => self.push_skipping_hooks(ctx),
            Action::Sync => self.sync(ctx),
            Action::PushFlow(act) => self.push_act(act, ctx),
            Action::Reload => self.refresh(ctx),
            Action::CancelOp => self.cancel_op(),
            Action::ToggleActivity => self.activity.open = !self.activity.open,
            Action::ShowActivity(id) => self.activity.reveal(id),
            Action::SetOffline(offline) => super::offline::set_offline(ctx, offline),
            Action::LoadMore => self.load_more(ctx),
            Action::Undo => {
                if !self.refuse_undo() {
                    self.undo(ctx);
                }
            }
            Action::Redo => self.redo(ctx),
            Action::ToggleTimeline => self.toggle_timeline(),
            Action::Timeline(act) => self.timeline_act(act, ctx),
            Action::Stash(request) => self.stash(request, ctx),
            Action::PickToStash(path) => self.stash_picks.toggle(&path),
            Action::ClearStashPicks => self.stash_picks.picked.clear(),
            Action::OpenTab(path) => self.open_request = Some(OpenRequest::NewTab(path)),
            Action::OpenWorktree(path) => self.open_request = Some(OpenRequest::Worktree(path)),
            Action::ToggleBranches => {
                let mut settings = Settings::get(ctx);
                settings.show_branches = !settings.show_branches;
                settings.set(ctx);
            }
            Action::BranchVisibility(change) => {
                let current = self.refs.current.as_deref();
                let hint = guide::visibility_hint(&change, &self.visibility, current);
                self.change_visibility(change, ctx);
                if let Some(hint) = hint {
                    self.offer_hint(hint, ctx);
                }
            }
            Action::GraphOrder(order) => self.set_log_order(order, ctx),
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
            Action::Popout(request) => self.popout_request(request, ctx),
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
                reviewers,
            } => self.create_pr(branch, base, title, body, draft, reviewers, ctx),
            Action::OpenUrl(url) => {
                if crate::pull_request::is_web_url(&url) {
                    let _ = open::that_detached(url);
                }
            }
            Action::ForgeLink(request) => self.forge_link(request, ctx),
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
            Action::OpenInDiffTool(key) => self.open_in_diff_tool(key, ctx),
            Action::ResolveInMergeTool(path) => self.resolve_in_merge_tool(path, ctx),
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
            Action::SetDiffAlgorithm(algorithm) => self.set_diff_algorithm(algorithm, ctx),
            Action::UnfoldDiff => {
                if let Some(view) = &mut self.diff {
                    view.unfolded = true;
                }
            }
            Action::FoldAttributes(paths) => self.ask_fold_attributes(&paths, ctx),
            Action::DiffAsFile(on) => self.show_diff_as_file(on, ctx),
            Action::DiffRendered(on) => self.show_diff_rendered(on, ctx),
            Action::DiffRich(kind) => self.show_diff_rich(kind, ctx),
            Action::DiffGoToLine { old, new } => self.diff_go_to_line(old, new),
            Action::DiffSvgSource(on) => {
                if let Some(view) = &mut self.diff {
                    view.svg_source = on;
                }
            }
            Action::DiffDecrypted(on) => self.show_diff_decrypted(on, ctx),
            Action::DiffDecryptedEncoding(encoding) => {
                if let Some(view) = &mut self.diff {
                    view.encrypted.close();
                    view.encrypted.json_encoding = encoding;
                }
            }
            Action::DiffStructural(on) => self.show_diff_structural(on, ctx),
            Action::DiffStep(delta) => {
                // Shown in place of the lines (not fallen back to them).
                let rich = self.diff_rich_kind().is_some()
                    && self.diff.as_ref().is_some_and(|v| {
                        matches!(&v.rich, Some((_, Ok(diff::rich::Rich::Ready(_)))))
                    });
                let rendered =
                    self.diff_rendered && self.diff.as_ref().is_some_and(|v| v.key.renders());
                if let Some(view) = self.diff.as_mut().filter(|v| {
                    !self.diff_as_file && !rendered && !rich && v.structural.shows_patch()
                }) {
                    view.step_change(Settings::get(ctx).diff_split, delta);
                }
            }
            Action::DiffMoved(block) => {
                if let Some(view) = &mut self.diff {
                    view.moved = match (&view.data, block) {
                        (Some(Ok(data)), Some(b)) => diff::moved_view::Opened::new(data, b),
                        _ => None,
                    };
                }
            }
            Action::DiffUnfold { gap, how } => {
                if let Some(view) = &mut self.diff {
                    view.unfold(gap, how);
                }
            }
            Action::DiffJumpLine(i) => {
                if let Some(view) = &mut self.diff {
                    view.jump_line = Some(i);
                }
            }
            Action::ViewAllFiles(on) => self.all_files.on = on,
            Action::ShowRemerge(on) => {
                self.remerges.on = on;
                // Its files are no longer listed: their diff goes with them.
                if !on && matches!(&self.diff, Some(v) if matches!(v.key, DiffKey::Remerge { .. }))
                {
                    self.diff = None;
                }
            }
            Action::FileView { path, rev, tab } => {
                self.open_file_view(path, rev, tab, ctx);
                // A line marked by an earlier `BlameAt` isn't what this one asks for.
                if let Some(view) = &mut self.file_view {
                    view.marked_line = None;
                }
            }
            Action::CloseFileView => self.file_view = None,
            Action::Navigate(step) => self.navigate(step, ctx),
            Action::LineHistory { path, at, range } => self.open_line_history(path, at, range, ctx),
            Action::OpenDiffOfLines { hash, path, lines } => {
                let lines = (!lines.is_empty()).then_some(lines);
                self.open_diff_of_lines(DiffKey::Commit { hash, path }, lines, ctx);
            }
            Action::RevertFile { hash, path } => self.revert_file(hash, path, ctx),
            Action::BlameAt { path, rev, line } => {
                self.open_file_view(path, rev, super::file_view::FileTab::Blame, ctx);
                if let Some(view) = &mut self.file_view {
                    view.goto_line.set(Some(line));
                    view.marked_line = Some(line);
                }
            }
            Action::InteractiveRebase { onto, label } => self.interactive_rebase(onto, label, ctx),
            Action::Reword { hash, message } => self.reword_commit(hash, message, ctx),
            Action::EditMessage(hash) => self.edit_message(hash, ctx),
            Action::ReviewBranch { branch, base } => self.start_review(ctx, branch, base),
            Action::SavePatch { hash, summary } => self.save_patch(hash, &summary, ctx),
            Action::ApplyPatch => self.apply_patch(ctx),
            Action::OpenBundleExport => self.open_bundle_export(ctx),
            Action::OpenBundleInspect => self.open_bundle_inspect(ctx),
            Action::ExportBundle { refs, base } => self.export_bundle(refs, base, ctx),
            Action::QuickEdit { edit, hash } => self.quick_edit(edit, hash, ctx),
            Action::DropMenu(menu) => self.drop_menu = Some(menu),
            Action::CommitDrop(drop) => self.commit_drop(drop, ctx),
            Action::RunRebase => self.run_rebase(ctx),
            Action::FoldInto(hash) => self.fold_into(hash, ctx),
            Action::RunFold(plan) => self.run_fold(*plan, ctx),
            Action::AbortFold => self.abort_fold(ctx),
            Action::CloseRebase => self.rebase = None,
            Action::OpenSplit(hash) => self.open_split(hash, ctx),
            Action::OpenMove(hash) => self.open_move(hash, ctx),
            Action::CheckSplit => self.check_split(ctx),
            Action::RunSplit => self.run_split(ctx),
            Action::CloseSplit => self.split = None,
            Action::OpenAbsorb => self.open_absorb(ctx),
            Action::CheckAbsorb => self.check_absorb(ctx),
            Action::RunAbsorb => self.run_absorb(ctx),
            Action::RunGitAbsorb => self.run_git_absorb(ctx),
            Action::CloseAbsorb => self.absorb = None,
            Action::Stack(act) => self.stack_act(act, ctx),
            Action::SignIn(creds) => self.sign_in(creds, ctx),
            Action::Ignore { pattern, path } => self.ignore(&pattern, &path, ctx),
            Action::PickLines {
                pick: diff::Pick::Revert,
                lines,
            } => self.revert_lines(&lines, ctx),
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
                self.pr_selection_request = None;
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
            Action::LfsLocks(act) => self.apply_lock_act(act, ctx),
            Action::Bisect(act) => self.bisect_act(act, ctx),
            Action::ParallelBisect(act) => self.parallel_act(act, ctx),
            Action::Guide(act) => self.guide_act(act, ctx),
            Action::LfsStorage(act) => self.apply_storage_act(act, ctx),
            Action::Unity(act) => self.apply_unity_act(act, ctx),
            Action::TestLane(act) => self.test_lane_act(act, ctx),
            Action::Rehearse(act) => self.rehearse_act(act, ctx),
            Action::Interaction(act) => self.interaction_act(act, ctx),
            Action::StripNotebookOutputs(on) => self.strip_notebook_outputs(on, ctx),
            Action::SaveConflict => self.save_conflict(ctx),
            Action::KeepBoth(path) => self.keep_both(path, ctx),
            Action::Archaeology(act) => self.archaeology_act(act, ctx),
            Action::Dco(act) => self.dco_act(act, ctx),
            Action::Owners(act) => self.owners_act(act, ctx),
            Action::Rerere(act) => self.rerere_act(act, ctx),
            Action::Decisions(act) => self.decisions_act(act, ctx),
            Action::Fork(act) => self.fork_act(act, ctx),
            Action::Release(act) => self.release_act(act, ctx),
            Action::Hotfix(act) => self.hotfix_act(act, ctx),
            Action::Package(act) => self.package_act(act, ctx),
        }
        self.drop_unfocused_pr_head();
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
        if self
            .diff
            .as_ref()
            .is_none_or(|v| v.encrypted.enabled || v.request != v.data_request)
        {
            return;
        }
        let request = self.diff.as_ref().map(|v| v.request);
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
            move |tab, saved, ctx| {
                if tab.diff.as_ref().is_some_and(|v| {
                    Some(v.request) == request
                        && v.data_request == v.request
                        && !v.encrypted.enabled
                }) {
                    tab.apply_picked(pick, &label, saved, ctx);
                }
            },
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

    /// Turns stripping notebooks' outputs when committing on or off, then shows the diff again.
    fn strip_notebook_outputs(&mut self, on: bool, ctx: &egui::Context) {
        let path = self.path.clone();
        self.then(
            ctx,
            move || {
                let done = git::set_strip_outputs(&path, on);
                (done, git::strip_state(&path))
            },
            move |tab, (result, state), ctx| {
                tab.status = Some(match result {
                    Ok(Some(command)) => {
                        let tool = if command == "nbstripout" { "nbstripout" } else { "Python" };
                        (
                            format!(
                                "Notebooks are staged without their outputs from now on (by \
                                 {tool}, in this clone): git status may show them changed until \
                                 they are committed once so"
                            ),
                            false,
                        )
                    }
                    Ok(_) => match state {
                        git::StripState::Other(name) => (
                            format!("gitcraft no longer strips outputs, but the repository's {name} filter still does"),
                            false,
                        ),
                        _ => ("Notebooks are staged with their outputs again".to_owned(), false),
                    },
                    Err(e) => (
                        format!("Could not change how notebooks are staged: {e}"),
                        true,
                    ),
                });
                tab.reload_status(ctx);
                tab.refresh_diff(ctx);
            },
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
        // Its decisions, as they are now (see `record_decisions`).
        if self
            .conflict
            .as_ref()
            .is_some_and(|ed| !ed.saving && ed.solved().is_ok())
        {
            self.prepare_decisions();
        }
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
            Ok(()) => {
                self.run_prepared(
                    "Mark resolved",
                    &format!("Saved {file}, but not marked resolved"),
                    ops(&[&["--literal-pathspecs", "add", "--", &file]]),
                    Refresh::Status,
                    OpKind::plain(),
                    ctx,
                );
                self.record_decisions(&file, ctx);
            }
            Err(e) => {
                // Not saved: its decisions aren't kept.
                self.decisions_pending = None;
                self.status = Some((e, true));
            }
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
