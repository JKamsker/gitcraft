//! Hover preview request identity, background execution and completion.

use super::*;

impl RepoTab {
    /// Once a frame, after drawing: starts, keeps or ends the ghost run of what the pointer is
    /// on.
    pub(in crate::repo) fn ghost_frame(&mut self, ctx: &egui::Context) {
        let offered = offered(ctx).filter(|_| self.busy_op.is_none() && !self.loading);
        let Some(op) = offered else {
            if self.ghost.hover.is_some() || self.ghost.shown.is_some() {
                self.ghost.end();
                ctx.request_repaint();
            }
            return;
        };
        if self.ghost.hover.as_ref().map(|(o, _)| o) != Some(&op) {
            self.ghost.end();
            self.ghost.hover = Some((op.clone(), Instant::now()));
        }
        let waited = self
            .ghost
            .hover
            .as_ref()
            .map_or(Duration::ZERO, |(_, at)| at.elapsed());
        let due = if ctx.input(|i| i.modifiers.alt) {
            ALT_DELAY
        } else {
            HOVER_DELAY
        };
        if waited < due {
            ctx.request_repaint_after(due - waited);
            return;
        }
        let key = self.ghost_key(op);
        let history = self.history_gen;
        if let Some(shown) = &self.ghost.shown
            && shown.key == key
            && shown.history == history
        {
            return;
        }
        if self.ghost.job.as_ref().is_some_and(|j| j.key == key) {
            return;
        }
        self.start_ghost(key, ctx);
    }

    pub(in crate::repo) fn timeline_ghost_active(&self) -> bool {
        self.ghost
            .hover
            .as_ref()
            .is_some_and(|(op, _)| matches!(op, GhostOp::Timeline { .. }))
    }

    /// The key `op`'s outcome is cached by.
    pub(super) fn ghost_key(&mut self, op: GhostOp) -> Key {
        let mut h = DefaultHasher::new();
        self.refs_fingerprint().hash(&mut h);
        // What a rebase checks the working tree for (untracked files don't count).
        op::tracked_changes(&self.work).hash(&mut h);
        if matches!(op, GhostOp::ResetHard { .. }) {
            self.work_read.hash(&mut h);
        }
        Key {
            op,
            refs: h.finish(),
        }
    }

    /// A hash of the commits every ref points at, worked out once per read of them (there may
    /// be tens of thousands, and the key is asked for every frame of a hover).
    pub(super) fn refs_fingerprint(&mut self) -> u64 {
        if let Some((read, fp)) = self.ghost.refs_fp
            && read == self.history_gen
        {
            return fp;
        }
        let mut h = DefaultHasher::new();
        let refs = &self.refs;
        (&refs.head, &refs.current).hash(&mut h);
        for b in refs
            .local
            .iter()
            .chain(refs.tags.iter())
            .chain(refs.remotes.iter().flat_map(|(_, list)| list))
        {
            (&b.name, &b.target, &b.upstream).hash(&mut h);
        }
        let fp = h.finish();
        self.ghost.refs_fp = Some((self.history_gen, fp));
        fp
    }

    /// Works out the outcome of `key`'s operation (or takes it from the cache) and lays out
    /// its ghost graph, in the background.
    pub(super) fn start_ghost(&mut self, key: Key, ctx: &egui::Context) {
        if let Some(job) = self.ghost.job.take() {
            job.cancel.store(true, Ordering::Relaxed);
        }
        self.ghost.jobs += 1;
        let id = self.ghost.jobs;
        let cancel = Arc::new(AtomicBool::new(false));
        let scope = self.tx.cancel.child();
        let mut sender = self.tx.clone();
        sender.cancel = scope.clone();
        self.ghost.job = Some(Job {
            id,
            key: key.clone(),
            cancel: Arc::clone(&cancel),
            _scope: crate::tasks::Scope(scope),
        });
        let cached = self.ghost.cached(&key);
        let facts = Facts {
            refs: self.refs.clone(),
            remotes: self.remotes.iter().map(|r| r.name.clone()).collect(),
            dirty: op::tracked_changes(&self.work),
        };
        let rows = self.commits.clone();
        let repo = self.path.clone();
        let version = Arc::clone(&self.ghost.version);
        let history = self.history_gen;
        let op = key.op.clone();
        let failure_key = key.clone();
        super::super::messages::spawn_or(
            &sender,
            ctx,
            move |error| {
                super::super::Msg::Then(Box::new(move |tab, ctx| {
                    tab.ghost_arrived(
                        id,
                        failure_key,
                        history,
                        Arc::new(Err(error.into())),
                        None,
                        ctx,
                    );
                }))
            },
            move || {
                let stop = || cancel.load(Ordering::Relaxed);
                let result = cached.unwrap_or_else(|| {
                    Arc::new(
                        (if matches!(op, GhostOp::Timeline { .. }) {
                            Ok(GitVersion(0, 0, 0))
                        } else {
                            git_version(&repo, &version)
                        })
                        .and_then(|v| plan::plan_with_witness(&repo, &op, &facts, v, &stop))
                        .map(|mut outcome| {
                            if matches!(op, GhostOp::Timeline { .. }) {
                                let loaded: std::collections::HashSet<_> =
                                    rows.iter().map(|c| c.hash.as_str()).collect();
                                outcome
                                    .created
                                    .retain(|c| !loaded.contains(c.hash.as_str()));
                            }
                            outcome
                        }),
                    )
                });
                let head_ref = match &op {
                    GhostOp::Timeline { state, .. } => {
                        (!state.branch.is_empty()).then(|| format!("refs/heads/{}", state.branch))
                    }
                    _ => facts
                        .refs
                        .current
                        .as_deref()
                        .map(|c| format!("refs/heads/{c}")),
                };
                let view = match (&*result, stop()) {
                    (Ok(outcome), false) => Some(view::build(&rows, outcome, head_ref.as_deref())),
                    _ => None,
                };
                super::super::Msg::Then(Box::new(move |tab, ctx| {
                    tab.ghost_arrived(id, key, history, result, view, ctx)
                }))
            },
        );
    }

    pub(super) fn ghost_arrived(
        &mut self,
        id: u64,
        key: Key,
        history: u64,
        result: Arc<Result<Outcome, PreviewFailure>>,
        view: Option<GhostView>,
        ctx: &egui::Context,
    ) {
        let cancelled = matches!(&*result, Err(e) if e.is_cancelled());
        // Only what was worked out: a failure may be passing (a machine not reachable).
        if result.is_ok() {
            self.ghost.remember(key.clone(), &result);
        }
        if self.ghost.job.as_ref().map(|j| j.id) != Some(id) || cancelled {
            return;
        }
        self.ghost.job = None;
        // Read again since it started: what it was laid out from is gone.
        if history != self.history_gen {
            self.start_ghost(key, ctx);
            return;
        }
        let added = view.as_ref().map_or(0, |v| v.added);
        if self.ghost.shifted != 0.0 {
            self.ghost.shift = Some(Shift::Out(std::mem::take(&mut self.ghost.shifted)));
        } else if added > 0 {
            self.ghost.shift = Some(Shift::In(added));
        }
        self.ghost.shown = Some(Shown {
            key,
            history,
            result,
            view,
            swapped: false,
        });
        ctx.request_repaint();
    }

    /// What the strip above the graph says, while a ghost run is on.
    pub(in crate::repo) fn ghost_strip_state(&self) -> Option<Strip<'_>> {
        let (op, _) = self.ghost.hover.as_ref()?;
        let title = op.title(&self.refs);
        match &self.ghost.shown {
            Some(shown) if &shown.key.op == op => Some(match &*shown.result {
                Ok(outcome) => Strip::Ready(title, outcome),
                Err(e) => Strip::Failed(title, e),
            }),
            _ if self.ghost.job.is_some() => Some(Strip::Working(title)),
            _ => None,
        }
    }

    /// Whether row `i`'s commit is one a ghost run would make: it has no menu.
    pub(in crate::repo) fn is_ghost_row(&self, i: usize) -> bool {
        self.ghost.kind(i) == Kind::New
    }
}
