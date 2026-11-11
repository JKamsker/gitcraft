use super::*;

impl RepoTab {
    /// Drops the merge put aside as `id` without bringing it back: its files go on a branch of
    /// their own, so nothing of it is lost; its run (and the agent's work) goes.
    pub(in crate::repo) fn drop_parked(&mut self, id: &str, ctx: &egui::Context) {
        // Never while it is brought back (or anything else runs on the worktree).
        if self.busy_op.is_some() {
            self.status = Some(crate::repo::notice::Notice::plain((
                "Wait for the running operation to finish".to_owned(),
                true,
            )));
            return;
        }
        let Some(parked) = self.resolver.parked.iter().find(|p| p.id == id).cloned() else {
            return;
        };
        let working = self.resolver.run.as_ref().is_some_and(|r| {
            r.id == id && matches!(r.phase, Phase::Preparing | Phase::Working | Phase::Checking)
        });
        if working {
            self.status = Some(crate::repo::notice::Notice::plain((
                "Stop the agent first".to_owned(),
                true,
            )));
            return;
        }
        let repo = self.path.clone();
        self.busy_op = Some(DROPPING.to_owned());
        self.then(
            ctx,
            move || park::drop_to_branch(&repo, &parked).map(|b| (parked, b)),
            |tab, dropped, ctx| tab.parked_dropped(dropped, ctx),
        );
    }

    pub(in crate::repo) fn parked_dropped(
        &mut self,
        dropped: Result<(Parked, String), String>,
        ctx: &egui::Context,
    ) {
        if self.busy_op.as_deref() == Some(DROPPING) {
            self.busy_op = None;
        }
        let (parked, branch) = match dropped {
            Ok(dropped) => dropped,
            Err(e) => {
                self.status = Some(crate::repo::notice::Notice::plain((
                    format!("Not dropped: {e}"),
                    true,
                )));
                return;
            }
        };
        self.resolver.parked.retain(|p| p.id != parked.id);
        self.resolver.parked_changed();
        if let Some(run) = self.resolver.run.as_mut().filter(|r| r.id == parked.id) {
            run.parked = None;
        }
        if self.resolver_owns(&parked.id) {
            self.resolver_act(super::super::ResolverAct::Dismiss, ctx);
        }
        self.status = Some(crate::repo::notice::Notice::plain((
            format!(
                "The {} was dropped: its files are on {branch}. Original recovery files remain available from its graph row menu.",
                parked.what()
            ),
            false,
        )));
        self.reload(ctx);
    }
}
