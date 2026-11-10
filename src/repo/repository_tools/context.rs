//! The central lifecycle must call invalidation at each transition, including A→B→A.
use super::*;
impl RepoTab {
    /// Advance the feature epoch and discard transient reads, cache and consent.
    /// Keep operation handles/accounting: executing mutations still require op_done.
    pub(in crate::repo) fn invalidate_repository_tool_requests(&mut self) {
        self.deferred.reset_requests();
        self.outbox_reading = false;
        self.outbox_last = None;
        if let Some(Dialog::RepositoryTools(owner)) = &mut self.dialog {
            owner.invalidate();
            self.dialog = None;
        }
    }
    /// Observe locally available changes before starting or completing any tool job.
    /// This supplements, but cannot replace, invalidation at central transitions.
    pub(super) fn repository_tool_scope(&mut self, ctx: &egui::Context) -> owner::Scope {
        self.notice_connections(ctx);
        let current = owner::Scope::current(self, ctx);
        if self.deferred.context_changed(&current) {
            self.invalidate_repository_tool_requests();
        }
        let current = owner::Scope::current(self, ctx);
        self.deferred.remember_context(current.clone());
        current
    }
    pub(super) fn complete_repository_tool(
        &mut self,
        ticket: &owner::Request,
        result: Result<Data, String>,
        ctx: &egui::Context,
    ) {
        let current = self.repository_tool_scope(ctx);
        if let Some(Dialog::RepositoryTools(state)) = &mut self.dialog {
            let effect = state.complete(ticket, &current, result);
            actions::settlement(self, effect);
        }
    }
    pub(super) fn prepare_repository_tool(
        &mut self,
        tool: Tool,
        ctx: &egui::Context,
    ) -> owner::Request {
        let scope = self.repository_tool_scope(ctx);
        let read = effects::Read {
            tool,
            query: String::new(),
        };
        let (state, ticket) = Owner::open(&read, scope);
        self.dialog = Some(Dialog::RepositoryTools(Box::new(state)));
        ticket
    }
}

impl owner::Scope {
    pub(super) fn current(tab: &RepoTab, ctx: &egui::Context) -> Self {
        let profile = crate::settings::Settings::get(ctx).profile();
        Self {
            path: tab.path.clone(),
            profile: crate::git::profile_config(),
            epoch: tab.deferred.epoch(),
            connection: tab.connection_context.clone(),
            selected_connection: crate::connections::current(),
            profile_id: profile.id,
            choices: profile.connections,
            signin: tab.https_generation,
        }
    }
}
