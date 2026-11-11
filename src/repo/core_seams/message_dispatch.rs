use super::*;

impl RepoTab {
    /// Drains finished background work and file-system events.
    pub fn poll(&mut self, ctx: &egui::Context) {
        self.poll_as(ctx, true);
    }

    /// [`Self::poll`] for a worktree view parked behind the one shown: its own work is seen
    /// through, but it doesn't fetch, follow builds or reload for changes on disk. The view
    /// shown does that for the repository they share, and this one reloads when shown again.
    pub(crate) fn poll_parked_view(&mut self, ctx: &egui::Context) {
        self.poll_as(ctx, false);
    }

    /// Settles background work for either view, reserving active-view refreshes for `shown`.
    fn poll_as(&mut self, ctx: &egui::Context, shown: bool) {
        #[cfg(test)]
        super::super::hang_diagnostics::checkpoint(format_args!("poll connections and filesystem"));
        self.notice_connections(ctx);
        self.local_ci.poll(ctx);
        self.shown = shown;
        if !shown || self.fs_pending.is_some() {
            self.discard_decrypted();
        }
        self.sync_forge_selection(ctx);
        if let Some(poller) = &mut self.poller {
            poller.tick(ctx);
        }
        // Changes on disk, debounced. Taken in before the finished work, so that a reload
        // that work starts covers what it wrote itself (see `reload`).
        if let Some(rx) = &self.fs_rx {
            while let Ok(change) = rx.try_recv() {
                self.working_drafts.observe();
                let was = self.fs_pending.map(|(c, _)| c);
                self.fs_pending = Some((was.max(Some(change)).unwrap_or(change), Instant::now()));
            }
        }

        while let Ok(msg) = self.rx.try_recv() {
            #[cfg(test)]
            let _stage = super::super::hang_diagnostics::message(&msg);
            self.handle(msg, ctx);
        }
        #[cfg(test)]
        super::super::hang_diagnostics::checkpoint(format_args!("poll feature readers"));
        if !self.sleeping() {
            self.sync_branch_bands(ctx);
            self.poll_branch_bands(ctx);
            self.poll_graph_topology(ctx);
            self.poll_postcards(ctx);
            self.poll_history_activity(ctx);
        }
        self.agent_step(ctx);
        self.poll_outbox(ctx);
        self.poll_deferred(ctx);
        // An agent resolving conflicts that ended is seen to here, the tab shown or not.
        self.resolver_poll(ctx);
        // A tab not read yet, or suspended, neither fetches nor follows builds.
        self.poll_saved_views(ctx);
        self.poll_content_recovery(ctx);
        self.poll_line_dossier(ctx);
        self.poll_symbol_biography(ctx);
        self.poll_cochanges(ctx);
        self.poll_working_drafts(ctx);
        if shown {
            self.poll_file_view(ctx);
        }
        if !shown || self.sleeping() {
            return;
        }
        #[cfg(test)]
        super::super::hang_diagnostics::checkpoint(format_args!("poll filesystem reload"));
        if let Some(wait) = self.folding.tick(Instant::now()) {
            ctx.request_repaint_after(wait);
        }
        if let Some((change, at)) = self.fs_pending
            && at.elapsed() > Duration::from_millis(400)
            && !self.loading
            && self.busy_op.is_none()
        {
            self.fs_pending = None;
            crate::timing::log(format_args!(
                "changes on disk ({change:?}): {}; reading {:?} again",
                crate::repo::watch::take_seen(&self.path).unwrap_or_else(|| "(polled)".into()),
                change.rereads()
            ));
            for reread in change.rereads() {
                match reread {
                    Reread::All => self.reload(ctx),
                    Reread::Status => self.reload_status(ctx),
                    Reread::Submodules => self.reload_submodules(ctx),
                }
            }
        }

        #[cfg(test)]
        super::super::hang_diagnostics::checkpoint(format_args!("poll background fetch"));
        // Background fetch.
        let minutes = Settings::get(ctx).auto_fetch_minutes;
        if minutes > 0
            && !crate::tray::watch::owns_fetch(&fetch_key(&self.path))
            && !self.remotes.is_empty()
            && self.busy_op.is_none()
            && crate::repo::offline::background_fetch_may_run(&self.path)
            && self.last_fetch.elapsed() > Duration::from_secs(u64::from(minutes) * 60)
        {
            self.last_fetch = Instant::now();
            self.run_ops_ex(
                "Background fetch",
                ops(&[&["fetch", "--all", "--prune", "--quiet"]]),
                Refresh::All,
                OpKind::plain(),
                true,
                ctx,
            );
        }
        if minutes > 0 {
            ctx.request_repaint_after(Duration::from_secs(30));
        }

        #[cfg(test)]
        super::super::hang_diagnostics::checkpoint(format_args!("poll network and CI state"));
        self.follow_network(ctx);
        self.poll_ci(ctx);
        self.poll_parallel(ctx);
        self.recount_siblings(ctx);
        self.poll_ci_view(ctx);

        #[cfg(test)]
        super::super::hang_diagnostics::checkpoint(format_args!("poll search"));
        // Debounced search while typing.
        if let Some(at) = self.search.edited
            && at.elapsed() > Duration::from_millis(350)
        {
            self.run_search(ctx);
        } else if self.search.edited.is_some() {
            ctx.request_repaint_after(Duration::from_millis(100));
        }
    }

    pub(in crate::repo) fn handle(&mut self, msg: Msg, ctx: &egui::Context) {
        match msg {
            Msg::ForgeResult(generation, operation, msg) => {
                // A review sent before a same-repository rename still has to settle its
                // posting editor. It is safe only if no repository move occurred since it
                // was sent; A→B→A cannot revive that old review. All reads stay invalidated.
                let same_review = matches!(msg.as_ref(), Msg::PrPosted { hosting, .. }
                    if generation >= self.forge_repository_generation
                        && crate::repo::load::same_repository(Some(hosting.as_ref()), self.hosting.as_ref()));
                if generation == self.forge_generation || same_review {
                    self.handle(*msg, ctx);
                } else if operation {
                    self.stale_fork_operation_finished();
                    // The mutation used its captured old target. Let the busy slot go, but
                    // never run old continuations (or retries) against the new hosting.
                    self.busy_op = None;
                    self.op_started = None;
                    self.status = Some(crate::repo::Notice::plain(("The operation on the previous primary forge finished; refresh that repository to check its result".into(), true)));
                    self.reload(ctx);
                }
            }
            Msg::Loaded(result) => self.loaded(result, ctx),
            Msg::Submodules(read, list) => {
                if Self::newer(read, &mut self.submodules_read) {
                    self.submodules = list;
                }
            }
            Msg::WorktreeCounts(counts) => self.worktrees_counted(counts, ctx),
            Msg::ConflictOpened(generation, path, editor) => {
                self.conflict_opened(generation, path, editor)
            }
            Msg::ConflictChanged(generation, Some(path)) => {
                self.conflict_changed(generation, path, ctx)
            }
            Msg::ConflictChanged(_, None) => {}
            Msg::ConflictSaved(path, saved) => {
                // Editable again (if still open), whatever came of it.
                if let Some(ed) = self.conflict.as_mut().filter(|ed| ed.path == path) {
                    ed.saving = false;
                }
                self.conflict_saved(path, saved, ctx)
            }
            Msg::Identity(identity) => {
                self.identity = identity;
                self.saved_views.dirty = true;
            }
            // One asked for before the tab was suspended (or watched again) sends to a channel
            // no longer read: dropped, not kept.
            Msg::Watcher(watch, watcher) => {
                if watch == self.watches && !self.sleeping() {
                    self.watcher = watcher.map(|w| *w);
                    self.watcher_arrived(ctx);
                }
            }
            Msg::RefsCheck(refs) => {
                // Moved before the watcher could see it.
                if refs.is_ok_and(|refs| refs != self.refs) {
                    self.reload(ctx);
                }
            }
            Msg::More(generation, _, _) if generation != self.history_gen => {}
            Msg::More(_, excluded, _)
                if excluded
                    != crate::repo::history_filter::rules(ctx, self.repo_key()).excludes() =>
            {
                self.loading_more = false;
                self.log_stream = Default::default();
            }
            Msg::More(_, _, result) => self.more_loaded(result, ctx),
            // Older than the working tree shown (a reload started since came in first).
            Msg::Status(read, _) if read <= self.work_read => {}
            Msg::Status(read, result) => self.status_arrived(read, result, ctx),
            Msg::Detail(hash, result) => self.detail_arrived(hash, result, ctx),
            Msg::DetailPrefetched(result) => self.detail_prefetched(result),
            Msg::FilesPrefetched(asked, result) => self.files_prefetched(asked, result, ctx),
            Msg::RebaseCommits {
                onto,
                label,
                result,
            } => self.open_rebase(onto, label, result),
            Msg::RewordCommits {
                hash,
                message,
                result,
            } => self.reword_older(hash, message, result, ctx),
            Msg::PatchSaved(file, result) => self.patch_saved(file, result),
            Msg::BundleDone(label, result, refresh) => {
                self.bundle_done(label, result, refresh, ctx)
            }
            Msg::QuickEditCommits {
                edit,
                hash,
                branch_guard,
                result,
            } => self.run_quick_edit(edit, hash, branch_guard, result, ctx),
            Msg::FileHistory(generation, path, rev, result) => {
                // Only the history of the revision shown: an older read may answer last. Nor
                // the whole file's, once the history of some of its lines is shown.
                if let Some(view) = &mut self.file_view
                    && view.generation == generation
                    && view.path == path
                    && view.rev == rev
                    && view.lines.is_none()
                {
                    view.history = Some(result);
                }
            }
            Msg::Blame(generation, path, rev, result) => {
                if let Some(view) = &mut self.file_view
                    && view.blame_generation == generation
                    && view.path == path
                    && view.rev == rev
                {
                    view.blame = Some(result);
                    view.blame_needs_refresh = false;
                }
            }
            Msg::FileText(generation, path, rev, result) => {
                if let Some(view) = &mut self.file_view
                    && view.generation == generation
                    && view.path == path
                    && view.rev == rev
                {
                    view.text = Some(result);
                }
            }
            Msg::TreeFiles(hash, result) => self.all_files.arrived(&hash, result),
            Msg::Diff(request, result) => self.diff_arrived(request, result.map(|data| *data), ctx),
            Msg::DiffFile(request, result) => {
                if let Some(view) = &mut self.diff {
                    view.file_arrived(request, result);
                }
            }
            Msg::DiffRich(request, kind, result) => {
                if let Some(view) = &mut self.diff {
                    view.rich_arrived(request, kind, *result);
                }
            }
            Msg::DiffRendered(request, result) => {
                if let Some(view) = &mut self.diff {
                    view.rendered_arrived(request, *result, ctx);
                }
            }
            Msg::DiffStructural(ids, result) => self.structural_arrived(ids, *result, ctx),
            Msg::PrDetails(request, result) => self.pr_details_arrived(request, *result, ctx),
            Msg::PrFiles(request, result) => self.pr_files_arrived(request, result, ctx),
            Msg::PrPosted { hosting, .. }
                if !crate::repo::load::same_repository(Some(&*hosting), self.hosting.as_ref()) => {}
            Msg::PrPosted {
                number,
                what,
                result,
                ..
            } => self.pr_posted(number, what, result, ctx),
            Msg::Compare(request, result) => {
                if let Some(c) = &mut self.compare
                    && c.request == request
                {
                    c.data = Some(result);
                }
            }
            Msg::Mergiraf(path, result) => self.mergiraf_done(path, result),
            Msg::JobPanicked(e) => self.status = Some(crate::repo::Notice::plain((e, true))),
            Msg::Then(then) => then(self, ctx),
            Msg::Search(request, result) => self.search_done(request, result, ctx),
            // Asked before the list asked for last (one read before a merge answering after
            // the one read since), or of the forge the remotes no longer point to.
            Msg::Prs(asked, _) if self.prs_loaded != Some(asked) => {}
            Msg::Prs(_, result) => match result {
                Ok(prs) => self.prs_arrived(prs, ctx),
                Err(e) => self.pr_error = Some(e),
            },
            Msg::CiStatuses(fresh, result) => self.ci_arrived(fresh, result),
            Msg::CiRuns(request, result) => self.ci_runs_arrived(request, result, ctx),
            Msg::CiJobs(request, run, result) => self.ci_jobs_arrived(request, run, result, ctx),
            Msg::CiLog(request, result) => self.ci_log_arrived(request, result),
            Msg::CiActed(label, result) => self.ci_acted(label, result),
            Msg::OpDone {
                label,
                result,
                refresh,
                kind,
                quiet,
                stopped,
            } => {
                if result.is_err() && !quiet {
                    self.keep_unfinished(stopped);
                }
                self.op_done(label, result, refresh, kind, quiet, ctx)
            }
        }
    }
}
