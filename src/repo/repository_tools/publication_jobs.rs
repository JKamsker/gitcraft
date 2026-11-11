//! Remote previews and publication preparation, execution, and settlement.

use super::*;

impl RepoTab {
    pub(super) fn remote_config_preview(
        &mut self,
        editor: remote_config::Editor,
        ctx: &egui::Context,
    ) {
        let scope = self.repository_tool_scope(ctx);
        let Some(Dialog::RepositoryTools(state)) = &mut self.dialog else {
            return;
        };
        let ticket = state.restart(scope, owner::Update::RemotePreview);
        let repo = self.path.clone();
        self.then_or(
            ctx,
            false,
            Err,
            move || remote_config::preview(&repo, editor),
            move |tab, result, ctx| {
                let current = tab.repository_tool_scope(ctx);
                if let Some(Dialog::RepositoryTools(state)) = &mut tab.dialog {
                    let effect = state.complete(&ticket, &current, result.map(Data::Remote));
                    actions::settlement(tab, effect);
                }
            },
        );
    }
}

impl RepoTab {
    pub(super) fn prepare_publication(
        &mut self,
        manager: publishing::Manager,
        ctx: &egui::Context,
    ) {
        let scope = self.repository_tool_scope(ctx);
        let Some(Dialog::RepositoryTools(state)) = &mut self.dialog else {
            return;
        };
        let ticket = state.restart(scope, owner::Update::PreparedPublication);
        let repo = self.path.clone();
        self.then_or(
            ctx,
            false,
            Err,
            move || publishing::prepare(&repo, manager),
            move |tab, result, ctx| {
                let current = tab.repository_tool_scope(ctx);
                if let Some(Dialog::RepositoryTools(state)) = &mut tab.dialog {
                    let effect = state.complete(&ticket, &current, result.map(Data::Publication));
                    actions::settlement(tab, effect);
                }
            },
        );
    }
    pub(super) fn publish_group(
        &mut self,
        manager: publishing::Manager,
        retry: bool,
        ctx: &egui::Context,
    ) {
        if self.busy_op.is_some() {
            return;
        }
        if let Some(pause) = crate::network::refused() {
            self.status = Some(crate::repo::Notice::plain((pause.why().into(), true)));
            return;
        }
        let scope = self.repository_tool_scope(ctx);
        let Some(Dialog::RepositoryTools(state)) = &self.dialog else {
            return;
        };
        let ticket = state.request(scope, owner::Update::Published);
        let control = self.follow_op(&crate::repo::ops(&[&["push"]]), false, ctx);
        self.busy_op = Some("Publish group".into());
        self.op_started = Some(std::time::Instant::now());
        self.last_op = None;
        self.timeline_op_started(false, crate::repo::Refresh::All);
        let repo = self.path.clone();
        let credentials = self.credentials.clone();
        self.then_or(
            ctx,
            true,
            Err,
            move || {
                publishing::publish(
                    &guard::Runner {
                        repo: &repo,
                        control: &control,
                        credentials: credentials.as_ref(),
                    },
                    manager,
                    retry,
                )
            },
            move |tab, result, ctx| {
                let outcome = result.as_ref().map_err(Clone::clone).and_then(|m| {
                    let failed = m.plan.as_ref().map_or(0, |p| {
                        p.destinations
                            .iter()
                            .filter(|d| matches!(d.result, Some(Err(_))))
                            .count()
                    });
                    if failed > 0 {
                        Err(crate::i18n::message(
                            "repo-workflows-repository_tools-publish-group-1f0d64fa",
                            &[("failed", (failed).into())],
                        ))
                    } else {
                        Ok(crate::i18n::text(
                            "repo-workflows-repository_tools-publish-group-449d962a",
                        )
                        .into())
                    }
                });
                tab.op_done(
                    crate::i18n::text("repo-workflows-repository_tools-publish-group-3cc9a2c6")
                        .into(),
                    outcome,
                    crate::repo::Refresh::All,
                    crate::repo::OpKind::plain(),
                    false,
                    ctx,
                );
                let current = tab.repository_tool_scope(ctx);
                if let Some(Dialog::RepositoryTools(state)) = &mut tab.dialog {
                    let effect = state.complete(&ticket, &current, result.map(Data::Publication));
                    actions::settlement(tab, effect);
                }
            },
        );
    }
}
