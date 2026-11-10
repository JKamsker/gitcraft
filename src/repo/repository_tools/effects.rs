//! Feature decisions produce explicit effects; only adapters execute jobs on a tab.
use super::*;

type Work = Box<dyn FnOnce(&guard::Runner<'_>) -> Result<String, String> + Send>;
pub(super) struct Mutation {
    pub label: &'static str,
    pub network: bool,
    pub work: Work,
}
impl Mutation {
    fn new(
        label: &'static str,
        network: bool,
        work: impl FnOnce(&guard::Runner<'_>) -> Result<String, String> + Send + 'static,
    ) -> Self {
        Self {
            label,
            network,
            work: Box::new(work),
        }
    }
}
pub(super) enum Effect {
    None,
    Read(Read),
    Mutation(Mutation),
    Dependencies(dependency::Plan, bool),
    Branch(Option<(String, String)>, Option<routing::Force>),
    PreparePublication(publishing::Manager),
    Publish(publishing::Manager, bool),
    Preview(remote_config::Editor),
}
pub(super) struct Read {
    pub tool: Tool,
    pub query: String,
}
impl Owner {
    pub(super) fn transition(act: Act) -> Effect {
        match act {
            Act::MakeIndependent(plan) => Effect::Mutation(Mutation::new(
                "Make clone independent",
                false,
                move |runner| borrowing::independent(runner, &plan),
            )),
            Act::CheckDependencies(plan) => Effect::Dependencies(*plan, false),
            Act::PublishDependencies(plan) => Effect::Dependencies(*plan, true),
            Act::ResolveSubmodule(c, oid) => Effect::Mutation(Mutation::new(
                "Resolve submodule pointer",
                false,
                move |runner| subconflict::resolve(runner, &c, &oid),
            )),
            Act::ForceConfirmed(plan) => Effect::Branch(None, Some(*plan)),
            Act::SaveRoutes(browser, name) => Effect::Mutation(Mutation::new(
                "Save pull and push routes",
                false,
                move |runner| routing::save(runner, &browser, &name),
            )),
            Act::PushBranch(name, oid) => Effect::Branch(Some((name, oid)), None),
            Act::SaveGroup(manager) => Effect::Mutation(Mutation::new(
                "Save publication group",
                false,
                move |runner| publishing::save_group(runner, &manager),
            )),
            Act::PreparePublication(manager) => Effect::PreparePublication(*manager),
            Act::PublishGroup(manager, retry) => Effect::Publish(*manager, retry),
            Act::SaveRemote(editor) => Effect::Mutation(Mutation::new(
                "Save remote configuration",
                false,
                move |runner| remote_config::save(runner, &editor),
            )),
            Act::RemotePreview(editor) => Effect::Preview(*editor),
            Act::Migrate(snapshot, target) => {
                Effect::Mutation(Mutation::new("Migrate ref storage", false, move |runner| {
                    reftable::migrate(runner, &snapshot, &target)
                }))
            }
            Act::Graft(history, commit, parent) => Effect::Mutation(Mutation::new(
                "Join imported histories temporarily",
                false,
                move |runner| replacements::graft(runner, &history, &commit, &parent),
            )),
            Act::RemoveReplacement(history, original, effective) => {
                Effect::Mutation(Mutation::new("Remove replace ref", false, move |runner| {
                    replacements::remove(runner, &history, &original, &effective)
                }))
            }
            Act::ConvertGrafts(history) => Effect::Mutation(Mutation::new(
                "Convert legacy grafts to removable replace refs",
                false,
                move |runner| replacements::convert(runner, &history),
            )),
            act => Self::read(act).map_or(Effect::None, Effect::Read),
        }
    }
    fn read(act: Act) -> Option<Read> {
        let (query, tool) = match act {
            Act::StateReceipts => (String::new(), Tool::StateReceipts),
            Act::Subtrees(q) => (q, Tool::Subtrees),
            Act::SubtreeJob(..) | Act::SubtreePreparePush(..) | Act::SubtreeSend(..) => {
                return None;
            }
            Act::Deferred => (String::new(), Tool::Deferred),
            Act::IsolatedStack(..)
            | Act::StartDeferred(..)
            | Act::ContinueDeferred(..)
            | Act::AcceptDeferred(..)
            | Act::CloseDeferred(..)
            | Act::ReconcileDeferred(..) => return None,
            Act::Virtuals => (String::new(), Tool::Virtuals),
            Act::EditVirtual(..)
            | Act::ToggleVirtual(..)
            | Act::UndoVirtual(..)
            | Act::PrepareVirtual(..)
            | Act::CommitVirtual(..) => return None,
            Act::Lists => (String::new(), Tool::Lists),
            Act::EditList(..) | Act::CommitList(..) | Act::ShelveList(..) => return None,
            Act::AggregateTurn(session, endpoint) => {
                (format!("{endpoint}\0{session}"), Tool::AggregateTurn)
            }
            Act::FinishAggregate(..) => return None,
            Act::Turn(parent, turn, session) => {
                (format!("{parent}\0{turn}\0{session}"), Tool::Turn)
            }
            Act::MarkTurn(..) | Act::PreviewInverse(..) | Act::RevertTurn(..) => return None,
            Act::Outbox => (String::new(), Tool::Outbox),
            Act::ReviewOutbox(..)
            | Act::QueuePush(..)
            | Act::PrepareOutbox(..)
            | Act::SendOutbox(..)
            | Act::RemoveOutbox(..)
            | Act::AutoOutbox(..) => return None,
            Act::Sharing => (String::new(), Tool::Sharing),
            Act::MakeIndependent(..) => return None,
            Act::AllRefs(q) => (q, Tool::Refs),
            Act::Object(q) => (q, Tool::Object),
            Act::Reftable => (String::new(), Tool::Reftable),
            Act::Replacements => (String::new(), Tool::Replacements),
            Act::Remote(name) => (name, Tool::Remote),
            Act::Publication(name) => (name, Tool::Publication),
            Act::Routes(name) => (name, Tool::Routes),
            Act::ForceBranch(name, oid) => (format!("{name}\0{oid}"), Tool::ForceRoute),
            Act::ForceConfirmed(..) => return None,
            Act::SubmoduleConflict(path) => (path, Tool::SubmoduleConflict),
            Act::Dependencies => (String::new(), Tool::Dependencies),
            Act::CheckDependencies(..) | Act::PublishDependencies(..) => return None,
            Act::ResolveSubmodule(..) => return None,
            Act::SaveRoutes(..) | Act::PushBranch(..) => return None,
            Act::SaveGroup(..) | Act::PreparePublication(..) | Act::PublishGroup(..) => {
                return None;
            }
            Act::SaveRemote(..) | Act::RemotePreview(..) => return None,
            Act::Migrate(..)
            | Act::Graft(..)
            | Act::RemoveReplacement(..)
            | Act::ConvertGrafts(..) => return None,
        };
        Some(Read { tool, query })
    }
}
