//! Construction and settlement belong to the feature; adapters supply the current scope
//! and execute returned effects. State's fields remain private to repository_tools.
use super::*;
use lifecycle::{Completion, Ticket};
use std::path::PathBuf;

#[derive(Clone, PartialEq, Eq)]
pub(super) struct Scope {
    pub(super) path: PathBuf,
    pub(super) profile: Option<PathBuf>,
    pub(super) epoch: u64,
    pub(super) connection: crate::connections::Context,
    pub(super) selected_connection: crate::connections::Context,
    pub(super) profile_id: u64,
    pub(super) choices: crate::connections::Choices,
    pub(super) signin: u64,
}
pub(super) struct Request {
    ticket: Ticket<Scope>,
    update: Update,
}
pub(super) enum Settlement {
    None,
    ObserveDeferred(u64),
    Notice(String),
}
#[derive(Clone, Copy)]
pub(super) enum Update {
    Read,
    Prepared,
    Marks,
    RemotePreview,
    PreparedPublication,
    Published,
    Dependencies,
}
impl Owner {
    pub(super) fn new(id: u64, tool: Tool, query: String) -> Self {
        Self {
            id,
            tool,
            query,
            result: None,
            filter: String::new(),
            acknowledged: false,
            compatible: false,
            parent: String::new(),
            effective: true,
        }
    }
    pub(super) fn open(read: &effects::Read, scope: Scope) -> (Self, Request) {
        let owner = Self::new(
            REQUESTS.fetch_add(1, Ordering::Relaxed),
            read.tool,
            read.query.clone(),
        );
        let ticket = owner.request(scope, Update::Read);
        (owner, ticket)
    }
    pub(super) fn request(&self, scope: Scope, update: Update) -> Request {
        Request {
            ticket: Ticket::new(self.id, scope),
            update,
        }
    }
    pub(super) fn restart(&mut self, scope: Scope, update: Update) -> Request {
        self.id = REQUESTS.fetch_add(1, Ordering::Relaxed);
        self.acknowledged = false;
        self.compatible = false;
        if matches!(update, Update::Prepared) {
            self.result = None;
        }
        self.request(scope, update)
    }
    pub(super) fn complete(
        &mut self,
        ticket: &Request,
        current: &Scope,
        result: Result<Data, String>,
    ) -> Settlement {
        let update = ticket.update;
        match ticket.ticket.settle(self.id, current, result) {
            Completion::Ignore => Settlement::None,
            Completion::Failed(error) => {
                if matches!(update, Update::Read | Update::Prepared) {
                    self.result = Some(Err(error));
                    Settlement::None
                } else {
                    Settlement::Notice(if matches!(update, Update::RemotePreview) {
                        format!("Remote preview: {error}")
                    } else {
                        error
                    })
                }
            }
            Completion::Ready(data) => {
                if matches!(
                    update,
                    Update::PreparedPublication
                        | Update::Dependencies
                        | Update::Prepared
                        | Update::Marks
                ) {
                    self.acknowledged = false;
                }
                if matches!(update, Update::Read) && self.tool == Tool::Subtrees {
                    self.query.clear();
                }
                let observe = matches!(&data, Data::Deferred(_));
                self.result = Some(Ok(data));
                if observe {
                    Settlement::ObserveDeferred(self.id)
                } else {
                    Settlement::None
                }
            }
        }
    }
    pub(super) fn invalidate(&mut self) {
        self.id = REQUESTS.fetch_add(1, Ordering::Relaxed);
        self.result = None;
        self.acknowledged = false;
        self.compatible = false;
    }
    pub(super) fn teardown(&self) -> lifecycle::Teardown {
        lifecycle::Teardown::new(Some(self.id))
    }
}

pub(super) fn retire(dialog: &mut Option<Dialog>, teardown: lifecycle::Teardown) {
    if let Some(Dialog::RepositoryTools(owner)) = dialog
        && teardown.retires(owner.id)
    {
        *dialog = None;
    }
}

#[cfg(test)]
#[path = "owner_tests.rs"]
mod tests;
