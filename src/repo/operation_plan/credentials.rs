use super::*;

impl RepoTab {
    /// Leaves out of the operation kept to run again after signing in the steps that ran
    /// before step `stopped` failed: they are done, and most would fail a second time (a tag or
    /// remote added again, a branch deleted again), so that the step that needed signing in
    /// would never run.
    pub(in crate::repo) fn keep_unfinished(&mut self, stopped: usize) {
        if let Some(op) = &mut self.last_op {
            retain_unfinished(&mut op.ops, stopped);
        }
    }

    /// Runs `ops` as an undoable step.
    pub(in crate::repo) fn run_undoable(
        &mut self,
        label: &str,
        ops: Vec<Vec<String>>,
        refresh: Refresh,
        undo: Undo,
        ctx: &egui::Context,
    ) {
        let before = self.refs.head.clone();
        self.run_ops(label, ops, refresh, OpKind::Normal(undo, before), ctx);
    }

    /// Clear all views of this endpoint, including parked worktrees. A cached
    /// transient secret already has an exact scope; fetch URL guesses are not needed.
    pub(crate) fn forget_https_memory(&mut self, path: &std::path::Path, url: &str) {
        if self.path == path
            || self
                .credentials
                .as_ref()
                .is_some_and(|c| c.scope.as_ref().is_some_and(|scope| scope.matches_url(url)))
        {
            let had_signin = self.credentials.is_some()
                || self.https_challenge.is_some()
                || matches!(self.dialog, Some(Dialog::Credentials { .. }));
            self.credentials = None;
            if had_signin {
                self.invalidate_https_challenge();
            }
        }
        for parked in &mut self.parked {
            parked.forget_https_memory(path, url);
        }
    }
    pub(in crate::repo) fn switch_https(&mut self, url: String, ctx: &egui::Context) {
        if !crate::settings::network_page::valid_https(&url) {
            return;
        }
        if let Some(busy) = &self.busy_op {
            self.status = Some(crate::repo::Notice::plain((
                crate::i18n::message(
                    "repo-core-2950",
                    &[(
                        "busy",
                        crate::repo::messages::display_operation(busy).into(),
                    )],
                ),
                true,
            )));
            return;
        }
        let challenge = self
            .https_challenge
            .as_ref()
            .filter(|ticket| self.https_ticket_current(ticket))
            .cloned();
        let Some(binding) = challenge.as_ref().and_then(|ticket| ticket.binding.clone()) else {
            self.invalidate_https_challenge();
            self.status = Some(crate::repo::Notice::plain((
                crate::i18n::text("repo-core-2951").into(),
                true,
            )));
            return;
        };
        if !challenge
            .as_ref()
            .is_some_and(|ticket| ticket.authorizes_endpoint(&url))
        {
            self.inspect_unbound_https(vec![url], ctx);
            return;
        }
        let path = self.path.clone();
        self.forget_https_memory(&path, &url);
        crate::settings::network_page::rejected(ctx, path.clone(), url.clone());
        let mut ticket = self.https_ticket(url.clone());
        ticket.binding = Some(binding.clone());
        let reads = self.reads;
        self.then_or(
            ctx,
            true,
            |error| (Err(error), false),
            move || {
                if let Err(error) = binding.verify(&path) {
                    return (Err(error), false);
                }
                let result = git::https_reject(&path, &url);
                let route_current = binding.verify(&path).is_ok();
                (result, route_current)
            },
            move |tab, (result, route_current), _| {
                if tab.reads != reads || !route_current {
                    tab.invalidate_https_challenge();
                }
                tab.https_rejection_done(ticket, result);
            },
        );
    }

    /// Remembers the credentials and runs the operation that needed them again.
    pub(in crate::repo) fn sign_in(&mut self, creds: git::Credentials, ctx: &egui::Context) {
        let challenge = self.https_challenge.take();
        if !challenge.as_ref().is_some_and(|ticket| {
            self.https_ticket_current(ticket)
                && creds
                    .scope
                    .as_ref()
                    .is_some_and(|scope| ticket.endpoints.iter().any(|url| scope.matches_url(url)))
        }) {
            self.status = Some(crate::repo::Notice::plain((
                crate::i18n::text("repo-core-2952").into(),
                true,
            )));
            return;
        }
        let Some(ticket) = challenge else {
            return;
        };
        let Some(binding) = ticket.binding.clone() else {
            self.invalidate_https_challenge();
            self.status = Some(crate::repo::Notice::plain((
                crate::i18n::text("repo-core-2953").into(),
                true,
            )));
            return;
        };
        let selected = ticket.endpoints.iter().find(|url| {
            creds
                .scope
                .as_ref()
                .is_some_and(|scope| scope.matches_url(url))
        });
        if !selected.is_some_and(|url| ticket.authorizes_endpoint(url)) {
            self.inspect_unbound_https(selected.cloned().into_iter().collect(), ctx);
            return;
        }
        self.https_checking = true;
        self.status = Some(crate::repo::Notice::plain((
            crate::i18n::text("repo-core-2954").into(),
            false,
        )));
        let path = self.path.clone();
        self.then_or(
            ctx,
            true,
            Err,
            move || binding.verify(&path),
            move |tab, result, ctx| {
                if !tab.https_ticket_current(&ticket) {
                    tab.status = Some(crate::repo::Notice::plain((
                        crate::i18n::text("repo-core-2955").into(),
                        true,
                    )));
                    return;
                }
                tab.https_checking = false;
                if let Err(error) = result {
                    tab.invalidate_https_challenge();
                    tab.status = Some(crate::repo::Notice::plain((error, true)));
                    return;
                }
                if let Some(binding) = ticket.binding
                    && let Some(op) = &mut tab.last_op
                {
                    match serde_json::to_string(&binding) {
                        Ok(json) => op.ops.insert(0, vec![EXPECT_HTTPS_ROUTE.into(), json]),
                        Err(_) => {
                            tab.status = Some(crate::repo::Notice::plain((
                                crate::i18n::text("repo-core-2956").into(),
                                true,
                            )));
                            return;
                        }
                    }
                }
                tab.accept_scoped_signin(creds, ctx);
            },
        );
    }
    pub(in crate::repo) fn accept_scoped_signin(
        &mut self,
        creds: git::Credentials,
        ctx: &egui::Context,
    ) {
        self.invalidate_https_challenge();
        self.credentials = Some(creds);
        if let Some(busy) = &self.busy_op
            && let Some(op) = &self.last_op
        {
            // It would not start: say so, rather than drop it.
            self.status = Some(crate::repo::Notice::plain((
                crate::i18n::message(
                    "repo-core-2957",
                    &[
                        (
                            "label",
                            crate::repo::messages::display_operation(&op.label).into(),
                        ),
                        (
                            "busy",
                            crate::repo::messages::display_operation(busy).into(),
                        ),
                    ],
                ),
                true,
            )));
        } else {
            self.run_last_op_again(ctx);
        }
    }

    /// Runs the last operation started again (what of it didn't run, after a failure).
    pub(in crate::repo) fn run_last_op_again(&mut self, ctx: &egui::Context) {
        if let Some(op) = self.last_op.take() {
            self.run_ops_ex(&op.label, op.ops, op.refresh, op.kind, op.quiet, ctx);
        }
    }
}
