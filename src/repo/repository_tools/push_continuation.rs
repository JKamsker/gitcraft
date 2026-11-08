//! Sidebar and dependency pushes resume the ordinary root operation, rather than
//! acquiring a direct endpoint-send continuation.
use super::{Dialog, REQUESTS, RepoTab, routing};
use crate::settings::Settings;
use std::sync::{Arc, atomic::Ordering};

impl RepoTab {
    pub(super) fn prepare_branch_review(
        &mut self,
        branch: Option<(String, String)>,
        force: Option<routing::Force>,
        ctx: &egui::Context,
    ) {
        if self.busy_op.is_some() || self.echo_push_pending() {
            return;
        }
        let id = REQUESTS.fetch_add(1, Ordering::Relaxed);
        let token = Arc::new(());
        let alive = Arc::downgrade(&token);
        self.dialog = Some(Dialog::PushLoading { id, token });
        let repo = self.path.clone();
        let path = repo.clone();
        let profile = Settings::get(ctx).profile();
        let context = crate::connections::current();
        let generation = self.https_generation;
        self.then_or(
            ctx,
            false,
            Err,
            move || {
                if alive.strong_count() == 0 { return Err("Push preparation canceled".into()); }
                match (branch, force) {
                    (Some((name, oid)), None) => routing::prepare_push(&repo, &name, &oid),
                    (None, Some(plan)) => Ok(routing::PreparedPush::from(&plan)),
                    _ => Err("Invalid selected push continuation".into()),
                }
            },
            move |tab, result, ctx| {
                if !matches!(&tab.dialog, Some(Dialog::PushLoading {id: current,..}) if *current == id) { return; }
                if tab.path != path || Settings::get(ctx).profile() != profile
                    || crate::connections::current() != context || tab.connection_context != context
                    || tab.https_generation != generation
                {
                    tab.dialog = Some(Dialog::PushError("The repository, profile or sign-in changed. Choose the push action again.".into()));
                    return;
                }
                match result {
                    Ok(prepared) => tab.review_prepared_push(prepared, ctx),
                    Err(error) => tab.dialog = Some(Dialog::PushError(error)),
                }
            },
        );
    }

    pub(super) fn review_prepared_push(
        &mut self,
        prepared: routing::PreparedPush,
        ctx: &egui::Context,
    ) {
        self.with_selected_push(prepared, |tab, ops| {
            tab.run_ops_ex(
                "Push selected route",
                ops,
                crate::repo::Refresh::All,
                crate::repo::OpKind::plain(),
                false,
                ctx,
            );
        });
    }

    /// The sole handback boundary accepts a root gate, never an endpoint sender.
    /// begin_push_preflight consumes the constraint synchronously in run_ops_ex.
    fn with_selected_push(
        &mut self,
        prepared: routing::PreparedPush,
        root_gate: impl FnOnce(&mut RepoTab, Vec<Vec<String>>),
    ) {
        let ops = prepared.route.operations();
        self.preflight_selection = Some(prepared);
        root_gate(self, ops);
        // Refusal (busy/offline) must not constrain a later unrelated push.
        self.preflight_selection = None;
    }
}

pub(super) fn parent_continuation(
    plan: &crate::git::repository_tools::dependencies::Plan,
) -> Option<routing::PreparedPush> {
    (plan.checked
        && plan.parent_result.as_ref().is_some_and(Result::is_ok)
        && plan
            .dependencies
            .iter()
            .all(|node| node.problem.is_none() && node.result.as_ref().is_some_and(Result::is_ok)))
    .then(|| routing::PreparedPush {
        snapshot: plan.snapshot.clone(),
        route: routing::PushRoute {
            branch: plan.branch.clone(),
            commit: plan.commit.clone(),
            remote: plan.parent_remote.clone(),
            destination: format!("refs/heads/{}", plan.parent_branch),
            urls: vec![plan.parent_url.clone()],
            lease: None,
        },
    })
}

#[cfg(test)]
mod action_continuation_recording {
    use super::*;
    use crate::git::repository_tools::{dependencies::Plan, guard::Snapshot};

    fn route(lease: Option<String>) -> routing::PushRoute {
        routing::PushRoute {
            branch: "topic".into(),
            commit: "a".repeat(40),
            remote: "publish-host".into(),
            destination: "refs/heads/release".into(),
            urls: vec!["https://publish.invalid/team/repo".into()],
            lease,
        }
    }

    #[test]
    fn sidebar_plain_and_force_handback_can_only_enter_the_original_root_gate() -> Result<(), String>
    {
        let ctx = egui::Context::default();
        let mut tab = RepoTab::new_deferred("/recorded/unopened-repo".into(), &ctx);
        for lease in [None, Some("b".repeat(40))] {
            let route = route(lease);
            let mut entered = 0;
            tab.with_selected_push(
                routing::PreparedPush {
                    snapshot: Snapshot::recording(),
                    route: route.clone(),
                },
                |tab, ops| {
                    entered += 1;
                    assert_eq!(ops, route.operations());
                    assert!(crate::push_preflight::has_push(&ops));
                    assert_eq!(
                        tab.preflight_selection.as_ref().map(|p| &p.route),
                        Some(&route)
                    );
                    // Record only the root-gate entry: no command or endpoint runner.
                    tab.preflight_pending = Some(crate::repo::PendingOp {
                        label: "recorded root entry".into(),
                        ops,
                        refresh: crate::repo::Refresh::All,
                        kind: crate::repo::OpKind::plain(),
                        quiet: false,
                    });
                },
            );
            assert_eq!(entered, 1);
            assert!(tab.preflight_selection.is_none());
            let pending = tab
                .preflight_pending
                .take()
                .ok_or("root gate did not retain the original operation")?;
            assert_eq!(pending.ops, route.operations());
            let mut plan = crate::push_preflight::selected_continuation_recording::plan(&route);
            plan.retain_selection(routing::PreparedPush {
                snapshot: Snapshot::recording(),
                route: route.clone(),
            })?;
            // Findings/profile consent remains root UI work. Only its submit step
            // replaces the ordinary push, retaining the selected source guard.
            let approved = crate::push_preflight::approved_ops(plan);
            assert_eq!(approved[0], pending.ops[0]);
            let step = approved.last().ok_or("approval step missing")?;
            assert_eq!(
                step.first().map(String::as_str),
                Some(crate::push_preflight::APPROVED)
            );
            let registered =
                crate::push_preflight::registered(step.get(1).ok_or("approval receipt missing")?)?;
            let exact = crate::push_preflight::push_args(&registered)?;
            assert_eq!(
                &exact[exact.len() - 2..],
                [
                    route.remote.clone(),
                    format!("{}:{}", route.commit, route.destination)
                ]
            );
            if let Some(lease) = &route.lease {
                assert!(
                    exact.contains(&format!("--force-with-lease={}:{lease}", route.destination))
                );
            }
        }
        Ok(())
    }

    #[test]
    fn dependency_success_returns_the_exact_parent_for_review_and_failure_returns_none()
    -> Result<(), String> {
        let route = route(None);
        let mut plan = Plan {
            snapshot: Snapshot::recording(),
            branch: route.branch.clone(),
            commit: route.commit.clone(),
            parent_remote: route.remote.clone(),
            parent_url: route.urls[0].clone(),
            parent_branch: "release".into(),
            dependencies: Vec::new(),
            checked: true,
            parent_result: Some(Ok("children completed; parent not sent".into())),
        };
        let parent = parent_continuation(&plan).ok_or("parent continuation missing")?;
        assert_eq!(parent.route, route);
        assert!(crate::push_preflight::has_push(&parent.route.operations()));
        plan.parent_result = Some(Err("child failed".into()));
        assert!(parent_continuation(&plan).is_none());
        plan.parent_result = None;
        assert!(parent_continuation(&plan).is_none());
        plan.parent_result = Some(Ok("not sufficient without check".into()));
        plan.checked = false;
        assert!(parent_continuation(&plan).is_none());
        Ok(())
    }
}
