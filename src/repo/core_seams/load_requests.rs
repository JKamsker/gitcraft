use super::*;

impl RepoTab {
    /// Re-reads refs, history, working tree, worktrees, stashes, remotes and submodules in the
    /// background.
    pub fn reload(&mut self, ctx: &egui::Context) {
        self.work_epoch = self.work_epoch.wrapping_add(1);

        self.saved_views.cancel();
        // Not read yet, or suspended: woken as when shown, which watches and reads once.
        if self.sleeping() {
            self.start_deferred(ctx);
            return;
        }
        // Changes seen on disk so far are read by this reload (or the one queued after the
        // current one): reloading for them again would read the same.
        self.fs_pending = None;
        // What merges couldn't be redone for (a machine not reached…) is tried again.
        self.remerges.forget_failures();
        if self.loading {
            // What changed may have been read before it changed: read again once this is in.
            self.reload_again = true;
            return;
        }
        self.loading = true;
        self.load_started = Some(Instant::now());
        self.last_load_started = self.load_started;
        let read = self.next_read();
        let forge_generation = self.forge_generation;
        let path = self.path.clone();
        // Keep the pages already loaded.
        let count = self.history.len().clamp(git::LOG_PAGE, MAX_HISTORY);
        let mut visibility = Visibility::load(ctx, self.repo_key());
        let order = crate::repo::graph_order::load(ctx, self.repo_key());
        let excluded = crate::repo::history_filter::rules(ctx, self.repo_key()).excludes();
        // Answered all the same if it panics: the tab reloads only once it is.
        let failed = |e| Msg::Loaded(Err(e));
        crate::repo::messages::spawn_or(&self.tx, ctx, failed, move || {
            crate::repo::messages::discover_fetch_key(&path);
            Msg::Loaded((|| {
                // Leaf reads share one fixed pool across every tab. The parent waits in
                // the job pool; no reload creates its own scoped OS threads.
                let read_log = |from: Option<Vec<String>>| {
                    let (path, excluded) = (path.clone(), excluded.clone());
                    crate::tasks::read(move || {
                        git::log_scoped(&path, from.as_deref(), order, 0, count, &excluded)
                    })
                };
                // With every branch shown, the history doesn't depend on the refs: read at
                // the same time as them, not after.
                let early_history = (!visibility.is_filtering())
                    .then(|| read_log(None))
                    .transpose()?;
                let refs = git::refs(&path)?;
                visibility.prune(&refs);
                let log_from = visibility.revisions(&refs);
                let history = match early_history {
                    Some(history) => history,
                    None => read_log(log_from.clone())?,
                };
                let work_trees = {
                    let path = path.clone();
                    crate::tasks::read(move || {
                        let work = git::status(&path)?;
                        let changes = Some(work.changed_files());
                        // The other worktrees' changes are counted after the graph shows (see
                        // `count_worktrees`): a status each, which a first read needn't wait on.
                        let worktrees = git::worktree_list(&path, changes).unwrap_or_default();
                        Ok::<_, String>((work, worktrees))
                    })?
                };
                let stashes = {
                    let path = path.clone();
                    crate::tasks::read(move || git::stashes(&path).unwrap_or_default())?
                };
                let remotes = {
                    let path = path.clone();
                    crate::tasks::read(move || git::remotes(&path))?
                };
                let submodules = {
                    let path = path.clone();
                    crate::tasks::read(move || git::submodules(&path).unwrap_or_default())?
                };
                let lfs = {
                    let path = path.clone();
                    crate::tasks::read(move || git::uses_lfs(&path) && git::lfs_installed(&path))?
                };
                let issues = {
                    let path = path.clone();
                    crate::tasks::read(move || crate::repo::issue_work::read(&path))?
                };
                // Two walks of the whole history, for a count: alongside the others.
                let excluded_namespace_count = {
                    let (path, from, excluded) = (path.clone(), log_from.clone(), excluded.clone());
                    crate::tasks::read(move || {
                        git::history_scope::excluded_namespace_count(
                            &path,
                            from.as_deref(),
                            &excluded,
                        )
                        .ok()
                    })?
                };
                let identity = {
                    let path = path.clone();
                    crate::tasks::read(move || git::identity(&path))?
                };
                let worktree_root = git::read_worktree_root(&path).map_err(|e| {
                    crate::i18n::message("repo-core-2893", &[("e", e.to_string().into())])
                })?;
                let (excluded_namespace_count, identity) =
                    (excluded_namespace_count.join()?, identity.join()?);
                let history = history.join()??.into_iter().map(Arc::new).collect();
                let (work, worktrees) = work_trees.join()??;
                let (stashes, remotes, submodules, lfs, issues) = (
                    stashes.join()?,
                    remotes.join()??,
                    submodules.join()?,
                    lfs.join()?,
                    issues.join()?,
                );
                let path = path.as_path();
                let gerrit = crate::gerrit::detect(path, &remotes);
                let forge_remotes = forge::detect_remotes(path, &remotes);
                Ok(Box::new(Loaded {
                    refs,
                    history,
                    history_namespaces: excluded,
                    excluded_namespace_count,
                    work,
                    worktrees,
                    worktree_root,
                    stashes,
                    remotes,
                    forge_remotes,
                    forge_generation,
                    gerrit,
                    submodules,
                    lfs,
                    practice: crate::practice::load(path),
                    identity,
                    requested: count,
                    log_from,
                    order,
                    read,
                    issues,
                }))
            })())
        });
        // The merges put aside for an agent: rows of the graph of their own.
        self.read_parked(ctx);
    }

    pub(in crate::repo) fn reload_status(&mut self, ctx: &egui::Context) {
        self.work_epoch = self.work_epoch.wrapping_add(1);
        let read = self.next_read();
        self.work_pending = read;
        let path = self.path.clone();
        let asked = std::time::Instant::now();
        spawn(&self.tx, ctx, move || {
            let _scope = crate::timing::scope(format!("status#{read}"));
            crate::timing::step("waited for a worker", asked);
            Msg::Status(read, git::status(&path))
        });
    }

    /// The watcher is there now. What changed while it was being set up (it walks the whole
    /// worktree, which takes a while) made no event: whatever was read before may have missed
    /// it. Not for a first read yet to start (it reads after this), nor for a parked view
    /// (it reloads when shown).
    pub(in crate::repo) fn watcher_arrived(&mut self, ctx: &egui::Context) {
        if self.sleeping() || !self.shown {
            return;
        }
        if self.loading {
            self.recheck_after_load = true;
        } else {
            self.recheck(ctx);
        }
    }

    /// Counts, in the background, the changes of the worktrees the last read left uncounted (a
    /// status each: the graph doesn't wait for them), one count at a time.
    /// Every count is made again (those shown are the last ones, kept until then); a read
    /// coming in while one runs counts again after it.
    pub(in crate::repo) fn count_worktrees(&mut self, ctx: &egui::Context) {
        if self.counting_worktrees {
            self.recount_worktrees = true;
            return;
        }
        let mut list = self.worktrees.clone();
        for wt in list.iter_mut().filter(|w| !w.current) {
            wt.changes = None;
        }
        if !list.iter().any(git::Worktree::needs_count) {
            return;
        }
        self.counting_worktrees = true;
        self.counted_at = Instant::now();
        let path = self.path.clone();
        // Answered with no counts should it fail, so that counting isn't held up for good.
        let failed = |_| Msg::WorktreeCounts(Vec::new());
        crate::repo::messages::spawn_or(&self.tx, ctx, failed, move || {
            git::count_changes(&path, &mut list);
            Msg::WorktreeCounts(
                list.into_iter()
                    .filter_map(|w| Some((w.path, w.changes?)))
                    .collect(),
            )
        });
    }

    /// Counts the other worktrees of a repository elsewhere again every so often while the
    /// window has the focus: the poller looks only at this one, so a change made in another
    /// (an editor there) would otherwise show only on the next reload.
    pub(in crate::repo) fn recount_siblings(&mut self, ctx: &egui::Context) {
        let others = self
            .worktrees
            .iter()
            .any(|w| !w.current && !w.bare && w.prunable.is_none());
        if !crate::remote::is_remote(&self.path)
            || !others
            || self.loading
            || self.counting_worktrees
            || !ctx.input(|i| i.focused)
        {
            return;
        }
        let since = self.counted_at.elapsed();
        if since < RECOUNT_EVERY {
            ctx.request_repaint_after(RECOUNT_EVERY - since);
            return;
        }
        self.count_worktrees(ctx);
    }

    /// Takes in the counts of [`Self::count_worktrees`].
    pub(in crate::repo) fn worktrees_counted(
        &mut self,
        counts: Vec<(PathBuf, usize)>,
        ctx: &egui::Context,
    ) {
        self.counting_worktrees = false;
        for wt in self.worktrees.iter_mut().filter(|w| !w.current) {
            if let Some((_, n)) = counts.iter().find(|(p, _)| *p == wt.path) {
                wt.changes = Some(*n);
            }
        }
        if std::mem::take(&mut self.recount_worktrees) {
            self.count_worktrees(ctx);
        }
    }

    /// Reads the working tree and submodules again, and the refs to see whether anything
    /// moved: rather than a whole reload, which on a large repository is far longer.
    pub(in crate::repo) fn recheck(&mut self, ctx: &egui::Context) {
        self.reload_status(ctx);
        self.reload_submodules(ctx);
        let path = self.path.clone();
        spawn(&self.tx, ctx, move || Msg::RefsCheck(git::refs(&path)));
    }

    /// The number of a read starting now.
    pub(in crate::repo) fn next_read(&mut self) -> u64 {
        self.discard_decrypted();
        if let Some(state) = &mut self.file_comparison {
            state.invalidate();
        }
        self.reads += 1;
        self.reads
    }

    /// Whether an answer of `read` is newer than what is shown from the read `shown`; if so,
    /// it becomes what is shown.
    pub(in crate::repo) fn newer(read: u64, shown: &mut u64) -> bool {
        let newer = read > *shown;
        if newer {
            *shown = read;
        }
        newer
    }

    /// Editing transport identity invalidates credentials and account-dependent read results.
    /// Already submitted operations keep their immutable original context.
    pub(crate) fn notice_connections(&mut self, ctx: &egui::Context) {
        let now = crate::connections::current();
        let generation = crate::connections::generation();
        if self.connection_context == now
            && self.connection_generation == generation
            && generation != u64::MAX
        {
            return;
        }
        self.connection_generation = generation;
        self.connection_context = now;
        self.invalidate_https_challenge();
        self.credentials = None;
        if matches!(self.dialog, Some(crate::repo::Dialog::Credentials { .. })) {
            self.dialog = None;
        }
        self.prs.clear();
        self.prs_from_cache = false;
        self.pending_prs = None;
        self.pr_error = None;
        self.prs_loaded = Some(Instant::now());
        self.forget_merged_prs();
        self.ci.account_changed();
        self.ci_view = None;
        self.avatars_hinted = false;
        self.pr_requests += 1;
        self.invalidate_pr_document();
        if let Some(view) = &mut self.pr_view {
            view.account_changed(self.pr_requests);
        }
        for parked in &mut self.parked {
            parked.notice_connections(ctx);
        }
        if self.sleeping() {
            return;
        }
        self.reload_identity(ctx);
        if crate::network::background_may_run() {
            self.load_prs(ctx);
            self.refresh_ci(ctx);
            if self.pr_view.is_some() {
                self.load_pr(ctx);
            }
            self.hint_avatars(ctx);
        }
    }

    /// Re-reads the submodules: the commits they should be at are in the index, which can
    /// change without any ref moving (`git add` of a submodule, `git submodule add`).
    /// Reads again who commits are made as: the profile's author changed.
    pub fn reload_identity(&mut self, ctx: &egui::Context) {
        let path = self.path.clone();
        spawn(&self.tx, ctx, move || Msg::Identity(git::identity(&path)));
    }

    pub(in crate::repo) fn reload_submodules(&mut self, ctx: &egui::Context) {
        let read = self.next_read();
        let path = self.path.clone();
        spawn(&self.tx, ctx, move || {
            Msg::Submodules(read, git::submodules(&path).unwrap_or_default())
        });
    }

    pub(in crate::repo) fn load_more(&mut self, ctx: &egui::Context) {
        if self.saved_views.active() {
            self.load_graph_view_page();
            return;
        }
        if self.loading_more || !self.has_more || self.history.len() >= MAX_HISTORY {
            return;
        }
        self.loading_more = true;
        let path = self.path.clone();
        let skip = self.history.len();
        let generation = self.history_gen;
        let from = self.log_from.clone();
        let order = self.log_order;
        let excluded = self
            .history_filter
            .read_namespaces
            .clone()
            .unwrap_or_else(git::history_scope::default_hidden_namespaces);
        let stream = Arc::clone(&self.log_stream);
        // Answered all the same if it panics: no more is read until it is.
        let failed_excluded = excluded.clone();
        let failed = move |e| Msg::More(generation, failed_excluded, Err(e));
        crate::repo::messages::spawn_or(&self.tx, ctx, failed, move || {
            let page = (|| {
                let mut slot = stream
                    .lock()
                    .map_err(|_| crate::i18n::text("repo-core-2894").to_owned())?;
                // The first page of this history (since it was last reloaded) starts the
                // `git log` the next ones are read from.
                if slot
                    .as_ref()
                    .is_none_or(|(made_for, _)| *made_for != generation)
                {
                    *slot = None;
                    *slot = Some((
                        generation,
                        git::LogStream::start_scoped(
                            &path,
                            from.as_deref(),
                            order,
                            skip,
                            &excluded,
                        )?,
                    ));
                }
                let page = match slot.as_mut() {
                    Some((_, log)) => log.next_page(git::LOG_PAGE),
                    None => Ok(Vec::new()),
                };
                // At the end of history (or after an error): nothing more to read from it.
                if !matches!(&page, Ok(p) if p.len() == git::LOG_PAGE) {
                    *slot = None;
                }
                page
            })();
            Msg::More(generation, excluded, page)
        });
    }

    pub(in crate::repo) fn load_prs(&mut self, ctx: &egui::Context) {
        let Some(hosting) = self.hosting.clone() else {
            return;
        };
        // Until the forge answers, the list it gave last time (if any).
        self.seed_prs(&hosting, ctx);
        let asked = Instant::now();
        self.prs_loaded = Some(asked);
        let path = self.path.clone();
        spawn(&self.forge_sender(), ctx, move || {
            Msg::Prs(asked, hosting.forge.provider().list(&path, &hosting))
        });
        self.load_merged_prs(ctx);
    }
}
