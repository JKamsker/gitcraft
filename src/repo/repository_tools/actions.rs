//! Repository-tool action routing and guarded snapshot loading.

use super::*;

impl RepoTab {
    pub(in crate::repo) fn repository_tools_act(&mut self, act: Act, ctx: &egui::Context) {
        self.repository_tool_scope(ctx);
        self.deferred.automatic = None;
        let act = match subtrees::act(self, act, ctx) {
            Some(act) => act,
            None => return,
        };
        let act = match deferred::act(self, act, ctx) {
            Some(act) => act,
            None => return,
        };
        let act = match virtuals::act(self, act, ctx) {
            Some(act) => act,
            None => return,
        };
        let act = match lists::act(self, act, ctx) {
            Some(act) => act,
            None => return,
        };
        let act = match turns::act(self, act, ctx) {
            Some(act) => act,
            None => return,
        };
        let act = match outbox::act(self, act, ctx) {
            Some(act) => act,
            None => return,
        };
        match Owner::transition(act) {
            effects::Effect::None => {}
            effects::Effect::Mutation(job) => {
                self.repository_tool_mutation(job.label, job.network, ctx, job.work)
            }
            effects::Effect::Dependencies(plan, publish) => self.dependency_job(plan, publish, ctx),
            effects::Effect::Branch(branch, force) => {
                self.prepare_branch_review(branch, force, ctx)
            }
            effects::Effect::PreparePublication(manager) => self.prepare_publication(manager, ctx),
            effects::Effect::Publish(manager, retry) => self.publish_group(manager, retry, ctx),
            effects::Effect::Preview(editor) => self.remote_config_preview(editor, ctx),
            effects::Effect::Read(read) => {
                let (owner, ticket) = Owner::open(&read, self.repository_tool_scope(ctx));
                self.dialog = Some(Dialog::RepositoryTools(Box::new(owner)));
                let path = self.path.clone();
                self.then_or(
                    ctx,
                    false,
                    Err,
                    move || read.run(&path),
                    move |tab, result, ctx| {
                        let current = tab.repository_tool_scope(ctx);
                        if let Some(Dialog::RepositoryTools(owner)) = &mut tab.dialog {
                            let effect = owner.complete(&ticket, &current, result);
                            settlement(tab, effect);
                        }
                    },
                );
            }
        }
    }
}

/// The tab adapter interprets the owner's UI/cache effects.
pub(super) fn settlement(tab: &mut RepoTab, effect: owner::Settlement) {
    match effect {
        owner::Settlement::None => {}
        owner::Settlement::ObserveDeferred(id) => {
            if let Some(Dialog::RepositoryTools(owner)) = &tab.dialog
                && let Some(Ok(Data::Deferred(view))) = &owner.result
            {
                tab.deferred.observe(id, view);
            }
        }
        owner::Settlement::Notice(error) => {
            tab.status = Some(crate::repo::Notice::plain((error, true)));
        }
    }
}
