mod deferred;
mod lists;
pub(super) mod palette;
mod receipts;
pub(super) use deferred::Cache as DeferredCache;
mod outbox;
mod sharing;
mod subtrees;
mod turns;
mod virtuals;
use crate::git::repository_tools::sharing as borrowing;
mod dependencies;
use crate::git::repository_tools::dependencies as dependency;
mod submodule_conflict;
use crate::git::repository_tools::submodule_conflict as subconflict;
mod push_continuation;
mod routes;
use crate::git::repository_tools::routes as routing;
mod publication;
mod remotes;
// Reachable repository internals dialogs; stale background answers never replace a new view.
use super::{Action, Dialog, RepoTab};
use crate::git::repository_tools::{
    all_refs, guard, object, publication as publishing, reftable, remote_config, replacements,
};
use std::sync::atomic::{AtomicU64, Ordering};
#[derive(Clone, Copy, PartialEq, Eq)]
enum Tool {
    StateReceipts,
    Subtrees,
    Refs,
    Object,
    Reftable,
    Replacements,
    Remote,
    Publication,
    Routes,
    ForceRoute,
    SubmoduleConflict,
    Dependencies,
    Sharing,
    Outbox,
    Turn,
    AggregateTurn,
    Lists,
    Virtuals,
    Deferred,
}
static REQUESTS: AtomicU64 = AtomicU64::new(1);

pub(crate) enum Act {
    StateReceipts,
    Subtrees(String),
    SubtreeJob(
        Box<crate::git::repository_tools::subtrees::View>,
        subtrees::Job,
    ),
    SubtreePreparePush(
        Box<crate::git::repository_tools::subtrees::View>,
        String,
        bool,
    ),
    SubtreeSend(Box<crate::git::repository_tools::subtrees::Prepared>),
    Deferred,
    IsolatedStack(Vec<crate::git::stack::restack::Preview>),
    StartDeferred(
        Box<crate::git::drafts::deferred::View>,
        String,
        Vec<crate::git::drafts::deferred::Spec>,
    ),
    ContinueDeferred(Box<crate::git::drafts::deferred::View>, String),
    AcceptDeferred(Box<crate::git::drafts::deferred::View>, String, bool),
    CloseDeferred(Box<crate::git::drafts::deferred::View>, String),
    ReconcileDeferred(Box<crate::git::drafts::deferred::View>, String),
    Virtuals,
    EditVirtual(
        Box<crate::git::drafts::virtuals::View>,
        crate::git::drafts::virtuals::Edit,
    ),
    ToggleVirtual(Box<crate::git::drafts::virtuals::View>, String),
    UndoVirtual(Box<crate::git::drafts::virtuals::View>),
    PrepareVirtual(Box<crate::git::drafts::virtuals::View>, String),
    CommitVirtual(Box<crate::git::drafts::virtuals::Prepared>),
    Lists,
    EditList(
        Box<crate::git::drafts::lists::View>,
        crate::git::drafts::lists::Edit,
    ),
    CommitList(Box<crate::git::drafts::lists::View>, String),
    ShelveList(Box<crate::git::drafts::lists::View>, String, bool),
    Turn(String, String, String),
    AggregateTurn(String, String),
    FinishAggregate(Box<crate::git::drafts::turn::Review>),
    MarkTurn(Box<crate::git::drafts::turn::Review>, Vec<usize>),
    PreviewInverse(Box<crate::git::drafts::turn::Review>),
    RevertTurn(Box<crate::git::drafts::turn::Inverse>),
    Outbox,
    QueuePush(Box<crate::git::repository_tools::outbox::View>, bool),
    PrepareOutbox(String),
    ReviewOutbox(String),
    SendOutbox(Box<crate::git::repository_tools::outbox::Prepared>),
    AutoOutbox(String),
    RemoveOutbox(Box<crate::git::repository_tools::outbox::View>, String),
    Sharing,
    MakeIndependent(Box<borrowing::Plan>),
    AllRefs(String),
    Object(String),
    Reftable,
    Migrate(Box<reftable::Storage>, String),
    Replacements,
    Graft(Box<replacements::Histories>, String, String),
    RemoveReplacement(Box<replacements::Histories>, String, String),
    ConvertGrafts(Box<replacements::Histories>),
    Remote(String),
    RemotePreview(Box<remote_config::Editor>),
    SaveRemote(Box<remote_config::Editor>),
    Publication(String),
    SaveGroup(Box<publishing::Manager>),
    PreparePublication(Box<publishing::Manager>),
    PublishGroup(Box<publishing::Manager>, bool),
    Routes(String),
    SaveRoutes(Box<routing::Browser>, String),
    PushBranch(String, String),
    ForceBranch(String, String),
    ForceConfirmed(Box<routing::Force>),
    SubmoduleConflict(String),
    ResolveSubmodule(Box<subconflict::Conflict>, String),
    Dependencies,
    CheckDependencies(Box<dependency::Plan>),
    PublishDependencies(Box<dependency::Plan>),
}
enum Data {
    Subtrees(crate::git::repository_tools::subtrees::View),
    SubtreePush(Box<crate::git::repository_tools::subtrees::Prepared>),
    StateReceipts(Vec<crate::git::drafts::publication::Report>),
    Deferred(crate::git::drafts::deferred::View),
    DeferredStack(
        crate::git::drafts::deferred::View,
        Vec<crate::git::drafts::deferred::Spec>,
    ),
    Virtual(crate::git::drafts::virtuals::View),
    VirtualCommit(Box<crate::git::drafts::virtuals::Prepared>),
    Lists(crate::git::drafts::lists::View),
    Turn(crate::git::drafts::turn::Review),
    Inverse(Box<crate::git::drafts::turn::Inverse>),
    Outbox(crate::git::repository_tools::outbox::View),
    PreparedOutbox(Box<crate::git::repository_tools::outbox::Prepared>),
    Sharing(borrowing::Sharing),
    Refs(all_refs::Browser),
    Object(object::Inspection),
    Reftable(reftable::Storage),
    Replacements(replacements::Histories),
    Remote(remote_config::Editor),
    Publication(publishing::Manager),
    Routes(routing::Browser),
    Force(routing::Force),
    SubmoduleConflict(subconflict::Conflict),
    Dependencies(dependency::Plan),
}
pub(crate) type State = Owner;

/// Private feature owner. Central repository code only holds the opaque State alias.
pub(crate) struct Owner {
    id: u64,
    query: String,
    result: Option<Result<Data, String>>,
    filter: String,
    tool: Tool,
    acknowledged: bool,
    compatible: bool,
    parent: String,
    effective: bool,
}

#[cfg(test)]
fn retire_origin_dialog(dialog: &mut Option<Dialog>, origin: Option<u64>) {
    owner::retire(dialog, lifecycle::Teardown::new(origin));
}
#[cfg(test)]
mod ui_completion_pure {
    use super::*;
    fn dialog(id: u64) -> Option<Dialog> {
        Some(Dialog::RepositoryTools(Box::new(State {
            id,
            query: String::new(),
            result: None,
            filter: String::new(),
            tool: Tool::Refs,
            acknowledged: false,
            compatible: false,
            parent: String::new(),
            effective: true,
        })))
    }
    #[test]
    fn completion_does_not_close_a_newer_tools_dialog() {
        let mut newer = dialog(2);
        retire_origin_dialog(&mut newer, Some(1));
        assert!(matches!(newer, Some(Dialog::RepositoryTools(ref s)) if s.id == 2));
        retire_origin_dialog(&mut newer, None);
        assert!(newer.is_some());
        retire_origin_dialog(&mut newer, Some(2));
        assert!(newer.is_none());
    }
    #[test]
    fn all_refs_renders_only_visible_rows() {
        let refs: Vec<_> = (0..10000)
            .map(|i| all_refs::Reference {
                name: format!("refs/gitcraft/{i}"),
                oid: "a".repeat(40),
                kind: "commit".into(),
                symbolic: String::new(),
            })
            .collect();
        let context = egui::Context::default();
        let mut rendered = 0;
        let mut output = context.run_ui(egui::RawInput::default(), |ui| {
            rendered = refs_list(ui, &refs, "", &mut Vec::new());
        });
        output.textures_delta.clear();
        assert!(rendered > 0 && rendered < 40, "rendered {rendered} rows");
    }
}

mod actions;
mod effects;
mod lifecycle;
mod mutation;
mod owner;
mod read;
mod view;
#[cfg(test)]
use view::refs_list;
pub(super) use view::show;
mod object_view;
use object_view::object_view;
mod reference_storage;
use reference_storage::{reftable_view, replacements_view};
mod dependency_jobs;
mod publication_jobs;

mod context;
