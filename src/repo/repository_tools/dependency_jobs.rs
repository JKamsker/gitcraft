//! Dependency publication with exact parent, profile, and sign-in bindings.

use super::*;

impl RepoTab {
    pub(super) fn dependency_job(
        &mut self,
        plan: dependency::Plan,
        publish: bool,
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
        let Some(Dialog::RepositoryTools(state)) = &mut self.dialog else {
            return;
        };
        let ticket = state.restart(scope, owner::Update::Dependencies);
        let id = state.id;
        let label = if publish {
            "Publish dependencies before parent review"
        } else {
            "Check submodule publication dependencies"
        };
        let control = self.follow_op(&crate::repo::ops(&[&["push"]]), false, ctx);
        self.busy_op = Some(label.into());
        self.op_started = Some(std::time::Instant::now());
        self.last_op = None;
        self.timeline_op_started(false, crate::repo::Refresh::All);
        let repo = self.path.clone();
        let expected_path = repo.clone();
        let profile = crate::settings::Settings::get(ctx).profile();
        let context = crate::connections::current();
        let generation = self.https_generation;
        let credentials = self.credentials.clone();
        self.then_or(
            ctx,
            true,
            Err,
            move || {
                let runner = guard::Runner {
                    repo: &repo,
                    control: &control,
                    credentials: credentials.as_ref(),
                };
                if publish {
                    dependency::publish_children(&runner, plan)
                } else {
                    dependency::check(&runner, plan)
                }
            },
            move |tab, result, ctx| {
                let parent = if publish {
                    result.as_ref().ok().and_then(push_continuation::parent_continuation)
                } else {
                    None
                };
                let outcome = result.as_ref().map_err(Clone::clone).and_then(|p| {
                    if p.parent_result.as_ref().is_some_and(Result::is_err) {
                        Err(crate::i18n::text("repo-workflows-repository_tools-dependency-job-96731680").into())
                    } else if p.dependencies.iter().any(|n| n.problem.is_some()) {
                        Err(crate::i18n::text("repo-workflows-repository_tools-dependency-job-f7c1dc97").into())
                    } else {
                        Ok(crate::i18n::text("repo-workflows-repository_tools-dependency-job-80d8056a").into())
                    }
                });
                tab.op_done(
                    label.into(),
                    outcome,
                    crate::repo::Refresh::All,
                    crate::repo::OpKind::plain(),
                    false,
                    ctx,
                );
                let current = tab.repository_tool_scope(ctx);
                if let Some(Dialog::RepositoryTools(state)) = &mut tab.dialog {
                    let effect = state.complete(&ticket, &current, result.map(Data::Dependencies));
                    actions::settlement(tab, effect);
                }
                if let Some(parent) = parent
                    && matches!(&tab.dialog, Some(Dialog::RepositoryTools(state)) if state.id == id)
                {
                    if tab.path != expected_path || crate::settings::Settings::get(ctx).profile() != profile
                        || crate::connections::current() != context || tab.connection_context != context
                        || tab.https_generation != generation
                    {
                        tab.status = Some(crate::repo::Notice::plain(("Dependencies finished, but the parent repository, profile or sign-in changed. Prepare its push again.".into(), true)));
                        return;
                    }
                    tab.review_prepared_push(parent, ctx);
                }
            },
        );
    }
}
