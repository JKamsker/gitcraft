use super::*;

impl RepoTab {
    /// Rebuilds the graph rows from `history` and `work`, keeping the selection.
    pub(in crate::repo) fn rebuild(&mut self, ctx: &egui::Context) {
        // Asleep, or woken and not read again: there is no history to lay out.
        if self.sleeping() || self.resuming {
            return;
        }
        let previous = self.selected_hash();
        self.rebuild_rows();
        self.saved_views.dirty = true;
        self.restore_selection(previous, ctx);
        self.take_prepared_message();
        self.refresh_work_diff(ctx);
        if let Some(hash) = self.pending_goto.clone() {
            self.goto(&hash, ctx);
        }
    }

    /// After a status read that leaves the rows as they were (the changes row neither appeared
    /// nor went away): what `rebuild` does besides laying out the graph again.
    pub(in crate::repo) fn refresh_work(&mut self, ctx: &egui::Context) {
        if let Some(wip) = self.commits.first_mut()
            && wip.hash == WIP_HASH
        {
            Arc::make_mut(wip).time = git::now();
        }
        self.take_prepared_message();
        self.refresh_work_diff(ctx);
    }

    /// The WIP pseudo-commit (when the tree is dirty), then `history` with its stashes, laid out
    /// as the graph.
    pub(in crate::repo) fn rebuild_rows(&mut self) {
        self.saved_views.dirty = true;
        self.history_index = self
            .history
            .iter()
            .enumerate()
            .map(|(i, c)| (c.hash.clone(), i))
            .collect();
        let selected_hash = self.selected_hash();
        let mut protected = self.history_filter.revealed.clone();
        protected.extend(self.selected_hash());
        protected.extend(self.pending_goto.clone());
        protected.extend(self.search.matches.iter().cloned());
        protected.insert(self.refs.head.clone());
        protected.extend(self.stashes.iter().map(|s| s.base.clone()));
        let rules = self.history_filter.applied.clone().unwrap_or_default();
        let (visible, graph_history, hidden, kept) = crate::repo::history_filter::compact(
            &self.history,
            &rules,
            &protected,
            self.log_order.first_parent,
            self.has_more,
        );
        self.history_filter.hidden_loaded = hidden;
        self.history_filter.kept_loaded = kept;
        let old_rows = std::mem::take(&mut self.commits);
        if !self.work.is_clean() {
            self.commits.push(Arc::new(Commit {
                hash: WIP_HASH.to_owned(),
                parents: if self.refs.head.is_empty() {
                    Vec::new()
                } else {
                    vec![self.refs.head.clone()]
                },
                time: git::now(),
                ..Commit::default()
            }));
        }
        // Each stash sits right above the commit it was made on, as a commit on no branch.
        let mut stashes_on: HashMap<&str, Vec<&Stash>> = HashMap::new();
        for stash in &self.stashes {
            stashes_on
                .entry(stash.base.as_str())
                .or_default()
                .push(stash);
        }
        self.commits
            .reserve(self.history.len() + self.stashes.len());
        // A merge put aside for an agent is something going on, as the changes are: at the top,
        // its lane running down to the commit it was put aside on.
        self.commits
            .extend(crate::repo::resolver::parked_rows(&self.resolver.parked));
        for commit in &visible {
            for stash in stashes_on.get(commit.hash.as_str()).into_iter().flatten() {
                self.commits.push(Arc::new(Commit {
                    hash: stash.hash.clone(),
                    parents: vec![stash.base.clone()],
                    author: stash.author.clone(),
                    email: stash.email.clone(),
                    time: stash.time,
                    date: stash.date.clone(),
                    summary: stash.message.clone(),
                    ..Commit::default()
                }));
            }
            // Shared with `history`, not copied: tens of thousands of commits may be loaded.
            self.commits.push(Arc::clone(commit));
        }
        if let Some(pinned) = &self.history_filter.pinned
            && selected_hash.as_ref() == Some(&pinned.hash)
            && !self.commits.iter().any(|c| c.hash == pinned.hash)
        {
            let at = usize::from(self.commits.first().is_some_and(|c| c.hash == WIP_HASH));
            self.commits.insert(at, Arc::clone(pinned));
        }
        self.insert_pr_graph_head();
        let pinned_layout =
            self.history_filter.pinned_layout.as_ref().map(|p| {
                crate::repo::history_filter::pinned_projection(p, &self.history, &visible)
            });
        let layout_by_hash: HashMap<&str, &Arc<Commit>> =
            graph_history.iter().map(|c| (c.hash.as_str(), c)).collect();
        let layout_rows: Vec<Arc<Commit>> = self
            .commits
            .iter()
            .map(|c| {
                if let Some(pinned) = pinned_layout.as_ref().filter(|p| p.hash == c.hash) {
                    return Arc::clone(pinned);
                }
                layout_by_hash
                    .get(c.hash.as_str())
                    .map_or_else(|| Arc::clone(c), |c| Arc::clone(c))
            })
            .collect();
        let graph = graph::layout_with(&layout_rows, self.log_order.first_parent);
        let index = self
            .commits
            .iter()
            .enumerate()
            .map(|(i, c)| (c.hash.clone(), i))
            .collect();
        let old_graph = std::mem::replace(&mut self.graph, graph);
        let old_index = std::mem::replace(&mut self.index, index);
        self.rebuild_graph_labels();
        self.queue_branch_bands();
        self.queue_graph_topology();
        self.cancel_minimap();
        // Freeing tens of thousands of commits (those a reload replaced), rows and index entries
        // takes a while: not on the UI thread.
        if old_rows.len() > 5000 {
            crate::connections::spawn(move || drop((old_rows, old_graph, old_index)));
        }
    }

    /// Keeps the selection; otherwise starts on the changes, or HEAD.
    pub(in crate::repo) fn restore_selection(
        &mut self,
        previous: Option<String>,
        ctx: &egui::Context,
    ) {
        if self
            .compare
            .as_ref()
            .is_some_and(|c| self.pr_selection_request == Some(c.request))
        {
            // A PR head not fetched yet has no row. Keep its comparison while a background
            // refresh chooses a temporary graph cursor; selecting HEAD would close it.
            self.selected = previous
                .as_ref()
                .and_then(|hash| self.index.get(hash).copied())
                .or_else(|| self.index.get(&self.refs.head).copied());
            return;
        }
        // A commit of the reflog view that the graph didn't have: shown there once it has it
        // (made a branch of), kept in the detail pane until then.
        // An operation that left conflicts goes to the changes row first.
        if let Some(hash) = self.reflog_commit_off_graph().filter(|_| !self.focus_wip) {
            if let Some(row) = self.index.get(&hash).copied() {
                self.select(row, ctx);
                self.scroll_to = Some(row);
            }
            return;
        }
        let mut keep = previous.as_ref().and_then(|h| self.index.get(h).copied());
        if std::mem::take(&mut self.focus_wip) && self.index.contains_key(WIP_HASH) {
            keep = None;
            self.stash_view = None;
        }
        if self.stash_view.is_some() {
            self.selected = self
                .stash_view
                .and_then(|i| self.index.get(&self.stashes.get(i)?.hash).copied());
            return;
        }
        let target = keep
            .or_else(|| self.index.get(WIP_HASH).copied())
            .or_else(|| self.index.get(&self.refs.head).copied());
        match target {
            Some(row) if keep == Some(row) => self.selected = Some(row),
            Some(row) => {
                self.selected = None;
                self.select(row, ctx);
                self.scroll_to = Some(row);
            }
            None => {
                self.selected = None;
                self.detail = None;
                self.detail_hash = None;
                self.drop_extras();
            }
        }
    }

    /// A merge or cherry-pick leaves a message to commit with.
    pub(in crate::repo) fn take_prepared_message(&mut self) {
        // Over an empty box, or one that holds only what the template and hook put there.
        if let Some(msg) = self.work.prepared_message.clone() {
            self.take_message(&msg);
        }
    }

    /// The diff of a file that's no longer listed (staged, committed…) is stale.
    pub(in crate::repo) fn refresh_work_diff(&mut self, ctx: &egui::Context) {
        let Some(DiffView {
            key: DiffKey::Work(file),
            ..
        }) = &self.diff
        else {
            return;
        };
        let still = self
            .work
            .staged
            .iter()
            .chain(&self.work.unstaged)
            .chain(&self.work.conflicted)
            .any(|f| f == file);
        let key = DiffKey::Work(file.clone());
        // The conflict editor keeps what was chosen (or Mergiraf's answer) while the file
        // is as it was opened.
        // Elsewhere, kept, and looked at in the background (see `conflict_changed`).
        let remote = crate::remote::is_remote(&self.path);
        let editing = self
            .conflict
            .as_ref()
            .is_some_and(|ed| ed.path == file.path && (remote || !ed.changed_on_disk(&self.path)));
        if remote
            && editing
            && let Some(ed) = &self.conflict
        {
            let (repo, path, opened) = (self.path.clone(), ed.path.clone(), ed.opened().to_owned());
            let generation = ed.generation;
            spawn(&self.tx, ctx, move || {
                let changed = crate::repo::conflict::changed_on_disk(&repo, &path, &opened);
                // Nothing to say when it didn't, nor when it couldn't be read: a dropped
                // connection is no reason to drop the choices made.
                Msg::ConflictChanged(generation, (changed == Some(true)).then_some(path))
            });
        }
        if still && (file.area != git::Area::Conflicted || editing) {
            crate::timing::log("the working tree was read: the open diff is read again");
            // Shown as it was until the new diff is read (no flicker after staging lines).
            self.refresh_diff(ctx);
        } else {
            self.diff = None;
            self.conflict = None;
            if still {
                self.open_diff(key, ctx);
            }
        }
    }

    /// Once per tab, on a forge's repository: tells the avatar loader where its authors' account
    /// pictures are.
    pub(in crate::repo) fn hint_avatars(&mut self, ctx: &egui::Context) {
        // Offline or metered: asked on the next read once that is over.
        if self.avatars_hinted
            || !Settings::get(ctx).avatars
            || !crate::network::background_may_run()
        {
            return;
        }
        // A local mirror has no host to ask.
        let Some(hosting) = self.hosting.clone().filter(|h| !h.host.is_empty()) else {
            return;
        };
        self.avatars_hinted = true;
        let repo = self.path.clone();
        let mut emails: Vec<String> = self.history.iter().map(|c| c.email.clone()).collect();
        emails.sort();
        emails.dedup();
        self.forge_then(
            ctx,
            move || hosting.forge.provider().avatars(&repo, &hosting, &emails),
            |_, hints, ctx| {
                for (email, hint) in hints {
                    crate::avatars::hint(&email, hint);
                }
                ctx.request_repaint();
            },
        );
    }
}
