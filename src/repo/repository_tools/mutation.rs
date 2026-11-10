//! Shared repository-tool mutation scheduling and originating-dialog retirement.

use super::*;

impl RepoTab {
    pub(super) fn repository_tool_mutation(
        &mut self,
        label: &str,
        network: bool,
        ctx: &egui::Context,
        work: impl FnOnce(&guard::Runner<'_>) -> Result<String, String> + Send + 'static,
    ) {
        if let Some(busy) = &self.busy_op {
            self.status = Some(crate::repo::Notice::plain((
                format!("{busy} is still running"),
                true,
            )));
            return;
        }
        if network && let Some(why) = crate::network::refused() {
            self.status = Some(crate::repo::Notice::plain((why.why().to_owned(), true)));
            return;
        }
        let teardown = match &self.dialog {
            Some(Dialog::RepositoryTools(state)) => state.teardown(),
            _ => lifecycle::Teardown::new(None),
        };
        let ops = crate::repo::ops(&[&[if network { "push" } else { "refs" }]]);
        let control = self.follow_op(&ops, false, ctx);
        self.busy_op = Some(label.into());
        self.op_started = Some(std::time::Instant::now());
        self.last_op = None;
        self.timeline_op_started(false, crate::repo::Refresh::All);
        let repo = self.path.clone();
        let label = label.to_owned();
        let credentials = self.credentials.clone();
        self.then_or(
            ctx,
            true,
            Err,
            move || {
                crate::tasks::with_cancellation(control.cancellation(), || {
                    work(&guard::Runner {
                        repo: &repo,
                        control: &control,
                        credentials: credentials.as_ref(),
                    })
                })
            },
            move |tab, result, ctx| {
                tab.op_done(
                    label,
                    result,
                    crate::repo::Refresh::All,
                    crate::repo::OpKind::plain(),
                    false,
                    ctx,
                );
                // Drop snapshots after a mutation. A subsequent action must reread all guards.
                owner::retire(&mut tab.dialog, teardown);
            },
        );
    }
}
