//! Carrying out the actions the views asked for.

#[path = "core_seams/working_tree_edits.rs"]
mod working_tree_edits;
pub(crate) use working_tree_edits::open_in_editor;

#[path = "core_seams/partial_patch_edits.rs"]
mod partial_patch_edits;
#[cfg(test)]
pub(in crate::repo) use partial_patch_edits::PreparedPick;

#[path = "core_seams/conflict_edits.rs"]
mod conflict_edits;

#[path = "core_seams/action_preconditions.rs"]
mod action_preconditions;
#[path = "core_seams/display_actions.rs"]
mod display_actions;

use std::path::{Path, PathBuf};
use std::time::Instant;

use super::detached::Leave;
use super::{
    Action, Compare, DiffKey, Msg, OpKind, OpenRequest, Refresh, RepoTab, Undo, diff, guide, ops,
    spawn,
};
use crate::git;
use crate::settings::Settings;

impl RepoTab {
    pub(crate) fn apply(&mut self, action: Action, ctx: &egui::Context) {
        let Some(action) = self.prepare_repo_action(action, ctx) else {
            return;
        };
        let plain = OpKind::plain();
        match action {
            Action::KeyboardStep(step) => self.keyboard_step(step),
            Action::FocusPane(pane) => self.focus_pane(pane, ctx),
            Action::Keyboard(request) => self.apply_keyboard(request, ctx),
            Action::SubmitPreflight(plan) => self.submit_preflight(*plan, ctx),
            Action::FixPreflight(plan, kind) => self.fix_preflight(*plan, &kind, ctx),
            Action::PreflightMessages(plan) => self.preflight_messages(*plan, ctx),
            Action::ReloadPreflight(plan) => self.reload_preflight(*plan, ctx),
            Action::RepositoryTools(act) => self.repository_tools_act(act, ctx),
            Action::DvcPull => {
                self.dialog = Some(super::Dialog::confirm(
                    "DVC: Pull",
                    crate::i18n::text("repo-core-0143").into(),
                    crate::i18n::text("repo-core-0144"),
                    git::dvc::pull_steps(),
                    None,
                ));
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
            Action::ReviewTools(act) => self.review_tools_act(act, ctx),
            Action::Story(act) => self.story_act(act, ctx),
            Action::Freshness(act) => self.freshness_act(act, ctx),
            Action::Review(act) => self.review_act(act, ctx),
            Action::Select(row) => {
                self.leave_pages();
                self.doctor = None;
                self.select_history(row, ctx);
                self.hint_after_select(row, ctx);
            }
            Action::ActivityCommit(hash) => self.activity_commit(hash, ctx),
            Action::OpenCommitChanges(hash) => self.open_commit_changes(hash, ctx),
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
                self.keyboard.borrow_mut().pane = Some(super::keyboard::Pane::Graph);
                self.open_diff(key, ctx);
                if self.diff.is_some() {
                    self.offer_hint(guide::Hint::ChangeNavigation, ctx);
                }
            }
            Action::Custom(act) => self.custom_act(act, ctx),
            Action::OpenLinkedPr { .. } => unreachable!("linked PR normalized above"),
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
            Action::Insights(act) => self.insights_act(act, ctx),
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
            Action::LoadPromptSession(hash) => self.load_prompt_session(hash, ctx),
            Action::ShowPromptHunk(seed) => self.show_prompt_hunk(seed, ctx),
            Action::OpenLineDossier(seed) => {
                self.symbols.close();
                self.open_line_dossier(seed, ctx)
            }
            Action::OpenSymbolBiography(seed) => self.open_symbol_biography(seed, ctx),
            Action::CloseSymbolBiography => self.symbols.close(),
            Action::CloseLineDossier => self.dossier.close(),
            Action::OpenPinnedFileLine(seed) => {
                if !self.open_pinned_file_line(seed, ctx) {
                    return;
                }
            }
            Action::EarlierVersions => self.earlier_versions(ctx),
            Action::DraftEnable(on) => self.enable_working_drafts(on, ctx),
            Action::DraftSnapshot => self.save_working_draft(false, ctx),
            Action::DraftUndo => self.undo_working_draft(ctx),
            Action::DraftPreview { id, path, index } => {
                self.preview_working_draft(id, path, index, ctx)
            }
            Action::DraftRecover { preview, hunks } => {
                self.recover_working_draft(*preview, hunks, ctx)
            }
            Action::OpenContentRecovery => self.dialog = Some(super::Dialog::ContentRecovery),
            Action::ContentSearch(query) => self.content_search(query, ctx),
            Action::ContentInspect(hit, source) => self.content_inspect(hit, source, ctx),
            Action::ContentPrepare(target) => self.content_prepare(target, ctx),
            Action::ContentPerform(prepared) => self.content_perform(prepared, ctx),
            Action::LocalCi(act) => self.local_ci_act(act, ctx),
            Action::Email(act) => self.email_act(act, ctx),
            Action::Devcontainer(act) => self.devcontainer_act(act, ctx),
            Action::Reflog(act) => self.reflog_act(act, ctx),
            Action::CloseReflog => self.close_reflog(ctx),
            Action::Issue(act) => self.issue_act(act, ctx),
            Action::ApplyPendingPrs => self.apply_pending_prs(ctx),
            Action::OpenSshSetup => self.open_ssh_setup(ctx),
            Action::OpenTour => self.open_tour(true, ctx),
            Action::OpenDoctor(section) => self.open_doctor(section, ctx),
            Action::CloseDoctor => self.doctor = None,
            Action::Doctor(act) => self.doctor_act(act, ctx),
            Action::Agent(act) => self.agent_act(act, ctx),
            Action::Resolver(act) => self.resolver_act(act, ctx),
            Action::Bake(act) => self.bake_act(act, ctx),
            Action::CloseDiff => {
                self.keyboard.borrow_mut().pane = Some(super::keyboard::Pane::Detail);
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
            Action::CarryCheckout(blocked) => self.carry_checkout(*blocked, ctx),
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
            Action::CopyText(text) => ctx.copy_text(text),
            Action::FocusCommit => self.focus_commit(ctx),
            Action::Fetch => self.fetch_now(ctx),
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
            Action::PrimaryForge(remote) => self.set_primary_forge(remote, ctx),
            Action::GraphOrder(order) => self.set_log_order(order, ctx),
            Action::OpenBranches(section) => self.reveal_branches(section, ctx),
            Action::ToggleDetail => {
                let mut settings = Settings::get(ctx);
                settings.show_detail = !settings.show_detail;
                settings.set(ctx);
            }
            Action::Terminal(action) => self.terminals.apply(action, &self.path, ctx),
            Action::Popout(request) => self.popout_request(request, ctx),
            Action::ToggleSearch => self.toggle_search(),
            Action::FocusSearch => self.focus_search(ctx),
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
                    self.status = Some(crate::repo::Notice::plain((e, true)));
                }
            }
            Action::OpenInEditor(rel) => self.open_configured_editor(rel, ctx),
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
            Action::ClearIdentity { name, email } => self.clear_identity(name, email, ctx),
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
            Action::DiffBesideGraph(on) => self.set_diff_beside_graph(on, ctx),
            Action::DiffRich(kind) => self.show_diff_rich(kind, ctx),
            Action::DiffGoToLine { old, new } => self.diff_go_to_line(old, new),
            Action::DiffWrap(on) => self.wrap_diff(on, ctx),
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
            Action::DiffStep(delta) => self.step_displayed_diff(delta, ctx),
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
            Action::OpenBehaviour => self.open_behaviour(),
            Action::OpenLinkedMachines => self.open_linked_machines(ctx),
            Action::OpenBundleExport => self.open_bundle_export(ctx),
            Action::OpenBundleInspect => self.open_bundle_inspect(ctx),
            Action::ExportBundle { refs, base } => self.export_bundle(refs, base, ctx),
            Action::QuickEdit {
                edit,
                hash,
                branch_guard,
            } => self.quick_edit(edit, hash, branch_guard, ctx),
            Action::DropMenu(menu) => self.drop_menu = Some(menu),
            Action::CommitDrop(drop) => self.commit_drop(drop, ctx),
            Action::RunRebase => self.ask_run_rebase(ctx),
            Action::ConfirmedRebase(id) => {
                self.confirmed_rebase(id, ctx);
            }
            Action::AskRebase { onto, label } => self.ask_plain_rebase(onto, label, ctx),
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
            Action::SwitchHttps(url) => self.switch_https(url, ctx),
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
                captions,
                since_fork,
            } => self.compare(
                Compare {
                    base,
                    base_label,
                    target,
                    target_label,
                    captions,
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
}
