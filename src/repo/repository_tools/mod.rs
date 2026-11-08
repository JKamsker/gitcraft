mod deferred;
mod lists;
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
pub(crate) struct State {
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
impl RepoTab {
    pub(super) fn repository_tools_act(&mut self, act: Act, ctx: &egui::Context) {
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
        let act = match act {
            Act::MakeIndependent(plan) => {
                self.repository_tool_mutation(
                    "Make clone independent",
                    false,
                    ctx,
                    move |runner| borrowing::independent(runner, &plan),
                );
                return;
            }
            Act::CheckDependencies(plan) => {
                self.dependency_job(*plan, false, ctx);
                return;
            }
            Act::PublishDependencies(plan) => {
                self.dependency_job(*plan, true, ctx);
                return;
            }
            Act::ResolveSubmodule(c, oid) => {
                self.repository_tool_mutation(
                    "Resolve submodule pointer",
                    false,
                    ctx,
                    move |runner| subconflict::resolve(runner, &c, &oid),
                );
                return;
            }
            Act::ForceConfirmed(plan) => {
                self.prepare_branch_review(None, Some(*plan), ctx);
                return;
            }
            Act::SaveRoutes(browser, name) => {
                self.repository_tool_mutation(
                    "Save pull and push routes",
                    false,
                    ctx,
                    move |runner| routing::save(runner, &browser, &name),
                );
                return;
            }
            Act::PushBranch(name, oid) => {
                self.prepare_branch_review(Some((name, oid)), None, ctx);
                return;
            }
            Act::SaveGroup(manager) => {
                self.repository_tool_mutation(
                    "Save publication group",
                    false,
                    ctx,
                    move |runner| publishing::save_group(runner, &manager),
                );
                return;
            }
            Act::PreparePublication(manager) => {
                self.prepare_publication(*manager, ctx);
                return;
            }
            Act::PublishGroup(manager, retry) => {
                self.publish_group(*manager, retry, ctx);
                return;
            }
            Act::SaveRemote(editor) => {
                self.repository_tool_mutation(
                    "Save remote configuration",
                    false,
                    ctx,
                    move |runner| remote_config::save(runner, &editor),
                );
                return;
            }
            Act::RemotePreview(editor) => {
                self.remote_config_preview(*editor, ctx);
                return;
            }
            Act::Migrate(snapshot, target) => {
                self.repository_tool_mutation("Migrate ref storage", false, ctx, move |runner| {
                    reftable::migrate(runner, &snapshot, &target)
                });
                return;
            }
            Act::Graft(history, commit, parent) => {
                self.repository_tool_mutation(
                    "Join imported histories temporarily",
                    false,
                    ctx,
                    move |runner| replacements::graft(runner, &history, &commit, &parent),
                );
                return;
            }
            Act::RemoveReplacement(history, original, effective) => {
                self.repository_tool_mutation("Remove replace ref", false, ctx, move |runner| {
                    replacements::remove(runner, &history, &original, &effective)
                });
                return;
            }
            Act::ConvertGrafts(history) => {
                self.repository_tool_mutation(
                    "Convert legacy grafts to removable replace refs",
                    false,
                    ctx,
                    move |runner| replacements::convert(runner, &history),
                );
                return;
            }
            act => act,
        };
        let (query, tool) = match act {
            Act::StateReceipts => (String::new(), Tool::StateReceipts),
            Act::Subtrees(q) => (q, Tool::Subtrees),
            Act::SubtreeJob(..) | Act::SubtreePreparePush(..) | Act::SubtreeSend(..) => return,
            Act::Deferred => (String::new(), Tool::Deferred),
            Act::IsolatedStack(..)
            | Act::StartDeferred(..)
            | Act::ContinueDeferred(..)
            | Act::AcceptDeferred(..)
            | Act::CloseDeferred(..)
            | Act::ReconcileDeferred(..) => return,
            Act::Virtuals => (String::new(), Tool::Virtuals),
            Act::EditVirtual(..)
            | Act::ToggleVirtual(..)
            | Act::UndoVirtual(..)
            | Act::PrepareVirtual(..)
            | Act::CommitVirtual(..) => return,
            Act::Lists => (String::new(), Tool::Lists),
            Act::EditList(..) | Act::CommitList(..) | Act::ShelveList(..) => return,
            Act::AggregateTurn(session, endpoint) => {
                (format!("{endpoint}\0{session}"), Tool::AggregateTurn)
            }
            Act::FinishAggregate(..) => return,
            Act::Turn(parent, turn, session) => {
                (format!("{parent}\0{turn}\0{session}"), Tool::Turn)
            }
            Act::MarkTurn(..) | Act::PreviewInverse(..) | Act::RevertTurn(..) => return,
            Act::Outbox => (String::new(), Tool::Outbox),
            Act::ReviewOutbox(..)
            | Act::QueuePush(..)
            | Act::PrepareOutbox(..)
            | Act::SendOutbox(..)
            | Act::RemoveOutbox(..)
            | Act::AutoOutbox(..) => return,
            Act::Sharing => (String::new(), Tool::Sharing),
            Act::MakeIndependent(..) => return,
            Act::AllRefs(q) => (q, Tool::Refs),
            Act::Object(q) => (q, Tool::Object),
            Act::Reftable => (String::new(), Tool::Reftable),
            Act::Replacements => (String::new(), Tool::Replacements),
            Act::Remote(name) => (name, Tool::Remote),
            Act::Publication(name) => (name, Tool::Publication),
            Act::Routes(name) => (name, Tool::Routes),
            Act::ForceBranch(name, oid) => (format!("{name}\0{oid}"), Tool::ForceRoute),
            Act::ForceConfirmed(..) => return,
            Act::SubmoduleConflict(path) => (path, Tool::SubmoduleConflict),
            Act::Dependencies => (String::new(), Tool::Dependencies),
            Act::CheckDependencies(..) | Act::PublishDependencies(..) => return,
            Act::ResolveSubmodule(..) => return,
            Act::SaveRoutes(..) | Act::PushBranch(..) => return,
            Act::SaveGroup(..) | Act::PreparePublication(..) | Act::PublishGroup(..) => return,
            Act::SaveRemote(..) | Act::RemotePreview(..) => return,
            Act::Migrate(..)
            | Act::Graft(..)
            | Act::RemoveReplacement(..)
            | Act::ConvertGrafts(..) => return,
        };
        let id = REQUESTS.fetch_add(1, Ordering::Relaxed);
        self.dialog = Some(Dialog::RepositoryTools(Box::new(State {
            id,
            query: query.clone(),
            result: None,
            filter: String::new(),
            tool,
            acknowledged: false,
            compatible: false,
            parent: String::new(),
            effective: true,
        })));
        let path = self.path.clone();
        let expected_path = path.clone();
        let profile = crate::git::profile_config();
        self.then_or(
            ctx,
            false,
            Err,
            move || match tool {
                Tool::Subtrees => {
                    crate::git::repository_tools::subtrees::read(&path, &query).map(Data::Subtrees)
                }
                Tool::StateReceipts => crate::git::drafts::publication::read(&path, &|| {
                    !crate::tasks::cancellation().is_cancelled()
                })
                .map(Data::StateReceipts),
                Tool::Object => object::inspect(&path, &query, &|| {
                    !crate::tasks::cancellation().is_cancelled()
                })
                .map(Data::Object),
                Tool::Deferred => crate::git::drafts::deferred::read(&path, &|| {
                    !crate::tasks::cancellation().is_cancelled()
                })
                .map(Data::Deferred),
                Tool::Virtuals => crate::git::drafts::virtuals::read(&path, &|| {
                    !crate::tasks::cancellation().is_cancelled()
                })
                .map(Data::Virtual),
                Tool::Lists => crate::git::drafts::lists::read(&path, &|| {
                    !crate::tasks::cancellation().is_cancelled()
                })
                .map(Data::Lists),
                Tool::AggregateTurn => {
                    let (endpoint, session) = query.split_once('\0').ok_or(crate::i18n::text(
                        "repo-workflows-repository_tools-repository-tools-act-38bb44e4",
                    ))?;
                    crate::git::drafts::turn::aggregate_review(&path, session, endpoint, &|| {
                        !crate::tasks::cancellation().is_cancelled()
                    })
                    .map(Data::Turn)
                }
                Tool::Turn => {
                    let parts = query.splitn(3, '\0').collect::<Vec<_>>();
                    if parts.len() != 3 {
                        return Err(crate::i18n::text(
                            "repo-workflows-repository_tools-repository-tools-act-e7cf34fe",
                        )
                        .into());
                    }
                    crate::git::drafts::turn::review(&path, parts[0], parts[1], parts[2], &|| {
                        !crate::tasks::cancellation().is_cancelled()
                    })
                    .map(Data::Turn)
                }
                Tool::Outbox => crate::git::repository_tools::outbox::read(&path).map(Data::Outbox),
                Tool::Sharing => borrowing::inspect(&path).map(Data::Sharing),
                Tool::Refs => all_refs::read(&path, &query).map(Data::Refs),
                Tool::Reftable => reftable::inspect(&path).map(Data::Reftable),
                Tool::Replacements => replacements::read(&path).map(Data::Replacements),
                Tool::Remote => remote_config::inspect(&path, &query).map(Data::Remote),
                Tool::Publication => publishing::read(&path, &query).map(Data::Publication),
                Tool::Routes => routing::read(&path, &query).map(Data::Routes),
                Tool::Dependencies => dependency::read(&path).map(Data::Dependencies),
                Tool::SubmoduleConflict => {
                    subconflict::read(&path, &query).map(Data::SubmoduleConflict)
                }
                Tool::ForceRoute => {
                    let (name, oid) = query.split_once('\0').ok_or(crate::i18n::text(
                        "repo-workflows-repository_tools-repository-tools-act-baa3959f",
                    ))?;
                    routing::prepare_force(&path, name, oid).map(Data::Force)
                }
            },
            move |tab, result, _| {
                if tab.path != expected_path || crate::git::profile_config() != profile {
                    return;
                }
                if let Some(Dialog::RepositoryTools(state)) = &mut tab.dialog
                    && state.id == id
                {
                    if let Ok(Data::Deferred(view)) = &result {
                        tab.deferred.observe(id, view);
                    }
                    if state.tool == Tool::Subtrees {
                        state.query.clear();
                    }
                    state.result = Some(result);
                }
            },
        );
    }
}

impl RepoTab {
    fn repository_tool_mutation(
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
        let origin = match &self.dialog {
            Some(Dialog::RepositoryTools(state)) => Some(state.id),
            _ => None,
        };
        let ops = super::ops(&[&[if network { "push" } else { "refs" }]]);
        let control = self.follow_op(&ops, false, ctx);
        self.busy_op = Some(label.into());
        self.op_started = Some(std::time::Instant::now());
        self.last_op = None;
        self.timeline_op_started(false, super::Refresh::All);
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
                    super::Refresh::All,
                    super::OpKind::plain(),
                    false,
                    ctx,
                );
                // Drop snapshots after a mutation. A subsequent action must reread all guards.
                retire_origin_dialog(&mut tab.dialog, origin);
            },
        );
    }
}

pub(super) fn show(ui: &mut egui::Ui, state: &mut State, acts: &mut Vec<Action>) {
    ui.set_width(850.0);
    if state.tool == Tool::Subtrees {
        subtrees::show(ui, state, acts);
        return;
    }
    if state.tool == Tool::StateReceipts {
        receipts::show(ui, state, acts);
        return;
    }
    if state.tool == Tool::Deferred {
        deferred::show(ui, state, acts);
        return;
    }
    if state.tool == Tool::Virtuals {
        virtuals::show(ui, state, acts);
        return;
    }
    if state.tool == Tool::Lists {
        lists::show(ui, state, acts);
        return;
    }
    if matches!(state.tool, Tool::Turn | Tool::AggregateTurn) {
        turns::show(ui, state, acts);
        return;
    }
    if state.tool == Tool::Outbox {
        outbox::show(ui, state, acts);
        return;
    }
    if state.tool == Tool::Sharing {
        sharing::show(ui, state, acts);
        return;
    }
    if state.tool == Tool::Dependencies {
        dependencies::show(ui, state, acts);
        return;
    }
    if state.tool == Tool::SubmoduleConflict {
        submodule_conflict::show(ui, state, acts);
        return;
    }
    if state.tool == Tool::ForceRoute {
        routes::force(ui, state, acts);
        return;
    }
    if state.tool == Tool::Routes {
        routes::show(ui, state, acts);
        return;
    }
    if state.tool == Tool::Publication {
        publication::show(ui, state, acts);
        return;
    }
    if state.tool == Tool::Remote {
        remotes::show(ui, state, acts);
        return;
    }
    if state.tool == Tool::Replacements {
        replacements_view(ui, state, acts);
        return;
    }
    if state.tool == Tool::Reftable {
        reftable_view(ui, state, acts);
        return;
    }
    if state.tool == Tool::Object {
        object_view(ui, state, acts);
        return;
    }
    ui.heading(crate::i18n::text(
        "repo-workflows-repository_tools-show-5d3cc263",
    ));
    ui.label(crate::i18n::text(
        "repo-workflows-repository_tools-show-df2937d4",
    ));
    ui.horizontal(|ui| {
        ui.label(crate::i18n::text(
            "repo-workflows-repository_tools-show-8dd5e5cc",
        ));
        ui.text_edit_singleline(&mut state.query);
        if ui
            .button(crate::i18n::text(
                "repo-workflows-repository_tools-show-9b9a8d05",
            ))
            .clicked()
        {
            acts.push(Action::RepositoryTools(Act::AllRefs(state.query.clone())));
        }
    });
    let Some(result) = &state.result else {
        ui.add(crate::spinner::Spinner::new());
        return;
    };
    let browser = match result {
        Ok(Data::Refs(v)) => v,
        Ok(_) => return,
        Err(e) => {
            ui.colored_label(crate::theme::Tokens::get(ui.ctx()).danger, e);
            return;
        }
    };
    if browser.root_fallback {
        ui.label(crate::i18n::text(
            "repo-workflows-repository_tools-show-f2d4cf1c",
        ));
    }
    ui.horizontal(|ui| {
        ui.label(crate::i18n::text(
            "repo-workflows-repository_tools-show-84e9b213",
        ));
        ui.text_edit_singleline(&mut state.filter);
    });
    refs_list(ui, &browser.refs, &state.filter, acts);
    ui.separator();
    ui.label(crate::i18n::text(
        "repo-workflows-repository_tools-show-e27da1c1",
    ));
    egui::ScrollArea::both()
        .id_salt("namespace_history")
        .max_height(260.0)
        .show(ui, |ui| {
            ui.monospace(&browser.history);
        });
}

fn object_view(ui: &mut egui::Ui, state: &mut State, acts: &mut Vec<Action>) {
    ui.heading(crate::i18n::text(
        "repo-workflows-repository_tools-object-view-8efd32c4",
    ));
    ui.label(crate::i18n::text(
        "repo-workflows-repository_tools-object-view-3e15593a",
    ));
    ui.horizontal(|ui| {
        ui.text_edit_singleline(&mut state.query);
        if ui
            .button(crate::i18n::text(
                "repo-workflows-repository_tools-object-view-e0723a86",
            ))
            .clicked()
        {
            acts.push(Action::RepositoryTools(Act::Object(state.query.clone())));
        }
    });
    let Some(result) = &state.result else {
        ui.add(crate::spinner::Spinner::new());
        return;
    };
    let data = match result {
        Ok(Data::Object(v)) => v,
        Ok(_) => return,
        Err(e) => {
            ui.colored_label(crate::theme::Tokens::get(ui.ctx()).danger, e);
            return;
        }
    };
    egui::ScrollArea::both()
        .id_salt("object_inspector")
        .max_height(600.0)
        .show(ui, |ui| {
            ui.label(crate::i18n::message(
                "repo-workflows-repository_tools-object-view-9147a920",
                &[
                    ("data-format-1", (data.format).clone().into()),
                    (
                        "data-objects-display-2",
                        data.objects.display().to_string().into(),
                    ),
                    ("data-query-3", (data.query).clone().into()),
                ],
            ));
            ui.label(crate::i18n::message(
                "repo-workflows-repository_tools-object-view-ddc47a7a",
                &[
                    ("data-git-dir-1", (data.git_dir).clone().into()),
                    ("data-common-dir-2", (data.common_dir).clone().into()),
                ],
            ));
            if let Some(o) = &data.object {
                ui.monospace(&o.oid);
                ui.label(crate::i18n::message(
                    "repo-workflows-repository_tools-object-view-44de072f",
                    &[
                        ("o-kind-1", (o.kind).clone().into()),
                        ("o-size-2", (o.size).clone().into()),
                        ("o-disk-3", (o.disk).clone().into()),
                    ],
                ));
                if let Some(base) = &o.base
                    && ui
                        .link(crate::i18n::message(
                            "repo-workflows-repository_tools-object-view-b39d687c",
                            &[("base", (base).clone().into())],
                        ))
                        .clicked()
                {
                    acts.push(Action::RepositoryTools(Act::Object(base.clone())));
                }
            }
            if let Some(missing) = &data.missing {
                ui.label(missing);
            }
            for note in &data.notes {
                ui.label(note);
            }
            for representation in &data.representations {
                ui.separator();
                ui.monospace(representation.path.display().to_string());
                if let Some(n) = representation.loose_bytes {
                    ui.label(crate::i18n::message(
                        "repo-workflows-repository_tools-object-view-14a9c8ce",
                        &[("n", (n).clone().into())],
                    ));
                }
                for (i, row) in representation.chain.iter().enumerate() {
                    if ui
                        .link(crate::i18n::message(
                            "repo-workflows-repository_tools-object-view-f4de0102",
                            &[
                                (
                                    "if-i-0-object-else-base-1",
                                    (if i == 0 {
                                        crate::i18n::text(
                                            "repo-workflows-repository_tools-object-view-62a6da87",
                                        )
                                    } else {
                                        crate::i18n::text(
                                            "repo-workflows-repository_tools-object-view-7b47361a",
                                        )
                                    })
                                    .clone()
                                    .into(),
                                ),
                                ("row-oid-2", (row.oid).clone().into()),
                                ("row-kind-3", (row.kind).clone().into()),
                                ("row-disk-4", (row.disk).clone().into()),
                                ("row-size-5", (row.size).clone().into()),
                                ("row-offset-6", (row.offset).clone().into()),
                                ("row-depth-7", (row.depth).clone().into()),
                            ],
                        ))
                        .clicked()
                    {
                        acts.push(Action::RepositoryTools(Act::Object(row.oid.clone())));
                    }
                }
                if representation.chain.iter().any(|r| r.base.is_some()) {
                    ui.label(crate::i18n::text(
                        "repo-workflows-repository_tools-object-view-9d4281bc",
                    ));
                }
            }
            ui.separator();
            ui.label(crate::i18n::text(
                "repo-workflows-repository_tools-object-view-56daa2be",
            ));
            for entry in &data.entries {
                if ui
                    .link(format!("{} · {} · {}", entry.name, entry.kind, entry.oid))
                    .clicked()
                {
                    acts.push(Action::RepositoryTools(Act::Object(entry.oid.clone())));
                }
            }
        });
}

fn reftable_view(ui: &mut egui::Ui, state: &mut State, acts: &mut Vec<Action>) {
    ui.heading(crate::i18n::text(
        "repo-workflows-repository_tools-reftable-view-c2782e54",
    ));
    let Some(result) = &state.result else {
        ui.add(crate::spinner::Spinner::new());
        return;
    };
    let storage = match result {
        Ok(Data::Reftable(v)) => v,
        Ok(_) => return,
        Err(e) => {
            ui.label(e);
            return;
        }
    };
    ui.label(crate::i18n::message(
        "repo-workflows-repository_tools-reftable-view-4101df87",
        &[
            ("storage-git-1", (storage.git).clone().into()),
            ("storage-format-2", (storage.format).clone().into()),
        ],
    ));
    ui.label(crate::i18n::text(
        "repo-workflows-repository_tools-reftable-view-faca3ce1",
    ));
    ui.label(crate::i18n::text(
        "repo-workflows-repository_tools-reftable-view-891eab64",
    ));
    if let Some(why) = &storage.blocked {
        ui.label(why);
    }
    ui.checkbox(
        &mut state.acknowledged,
        crate::i18n::text("repo-workflows-repository_tools-reftable-view-87ed192f"),
    );
    ui.checkbox(
        &mut state.compatible,
        crate::i18n::text("repo-workflows-repository_tools-reftable-view-ea4f0afa"),
    );
    let target = if storage.format == "files" {
        "reftable"
    } else {
        "files"
    };
    if ui
        .add_enabled(
            storage.migrates && storage.blocked.is_none() && state.acknowledged && state.compatible,
            egui::Button::new(crate::i18n::message(
                "repo-workflows-repository_tools-reftable-view-d3ba0aeb",
                &[("target", (target).clone().into())],
            )),
        )
        .clicked()
    {
        acts.push(Action::RepositoryTools(Act::Migrate(
            Box::new(storage.clone()),
            target.into(),
        )));
    }
}

fn replacements_view(ui: &mut egui::Ui, state: &mut State, acts: &mut Vec<Action>) {
    ui.heading(crate::i18n::text(
        "repo-workflows-repository_tools-replacements-view-f5b874e5",
    ));
    let Some(result) = &state.result else {
        ui.add(crate::spinner::Spinner::new());
        return;
    };
    let data = match result {
        Ok(Data::Replacements(v)) => v,
        Ok(_) => return,
        Err(e) => {
            ui.label(e);
            return;
        }
    };
    ui.label(crate::i18n::text(
        "repo-workflows-repository_tools-replacements-view-fcc3aff3",
    ));
    ui.checkbox(
        &mut state.acknowledged,
        crate::i18n::text("repo-workflows-repository_tools-replacements-view-9c43f2c2"),
    );
    for r in &data.replacements {
        ui.horizontal(|ui| {
            if ui
                .link(crate::i18n::message(
                    "repo-workflows-repository_tools-replacements-view-33ea34c6",
                    &[
                        ("r-original-1", (r.original).clone().into()),
                        ("r-effective-2", (r.effective).clone().into()),
                    ],
                ))
                .clicked()
            {
                acts.push(Action::RepositoryTools(Act::Object(r.effective.clone())));
            }
            if ui
                .add_enabled(
                    state.acknowledged,
                    egui::Button::new(crate::i18n::text(
                        "repo-workflows-repository_tools-replacements-view-4953f897",
                    )),
                )
                .clicked()
            {
                acts.push(Action::RepositoryTools(Act::RemoveReplacement(
                    Box::new(data.clone()),
                    r.original.clone(),
                    r.effective.clone(),
                )));
            }
        });
        ui.label(crate::i18n::message(
            "repo-workflows-repository_tools-replacements-view-5fcfdd61",
            &[("r-parents-1", (r.parents).clone().into())],
        ));
    }
    if !data.legacy.is_empty() {
        ui.label(crate::i18n::text(
            "repo-workflows-repository_tools-replacements-view-7b8c9687",
        ));
        ui.monospace(&data.legacy);
        if ui
            .add_enabled(
                state.acknowledged,
                egui::Button::new(crate::i18n::text(
                    "repo-workflows-repository_tools-replacements-view-1714de4d",
                )),
            )
            .clicked()
        {
            acts.push(Action::RepositoryTools(Act::ConvertGrafts(Box::new(
                data.clone(),
            ))));
        }
    }
    ui.horizontal(|ui| {
        ui.selectable_value(
            &mut state.effective,
            true,
            crate::i18n::text("repo-workflows-repository_tools-replacements-view-0a69dfc6"),
        );
        ui.selectable_value(
            &mut state.effective,
            false,
            crate::i18n::text("repo-workflows-repository_tools-replacements-view-4cff5861"),
        );
    });
    egui::ScrollArea::both()
        .id_salt("replacement_history")
        .max_height(280.0)
        .show(ui, |ui| {
            ui.monospace(if state.effective {
                &data.effective
            } else {
                &data.original
            });
        });
    ui.separator();
    ui.label(crate::i18n::text(
        "repo-workflows-repository_tools-replacements-view-e51254f0",
    ));
    ui.horizontal(|ui| {
        ui.label(crate::i18n::text(
            "repo-workflows-repository_tools-replacements-view-84da95ea",
        ));
        ui.text_edit_singleline(&mut state.query);
    });
    ui.horizontal(|ui| {
        ui.label(crate::i18n::text(
            "repo-workflows-repository_tools-replacements-view-b022884c",
        ));
        ui.text_edit_singleline(&mut state.parent);
    });
    if ui
        .add_enabled(
            state.acknowledged && !state.query.is_empty() && !state.parent.is_empty(),
            egui::Button::new(crate::i18n::text(
                "repo-workflows-repository_tools-replacements-view-7916bff6",
            )),
        )
        .clicked()
    {
        acts.push(Action::RepositoryTools(Act::Graft(
            Box::new(data.clone()),
            state.query.trim().into(),
            state.parent.trim().into(),
        )));
    }
}

impl RepoTab {
    fn remote_config_preview(&mut self, editor: remote_config::Editor, ctx: &egui::Context) {
        let Some(Dialog::RepositoryTools(state)) = &mut self.dialog else {
            return;
        };
        let id = REQUESTS.fetch_add(1, Ordering::Relaxed);
        state.id = id;
        let repo = self.path.clone();
        let path = repo.clone();
        self.then_or(
            ctx,
            false,
            Err,
            move || remote_config::preview(&repo, editor),
            move |tab, result, _| {
                if tab.path != path {
                    return;
                }
                if let Some(Dialog::RepositoryTools(state)) = &mut tab.dialog
                    && state.id == id
                {
                    match result {
                        Ok(editor) => state.result = Some(Ok(Data::Remote(editor))),
                        Err(e) => {
                            tab.status = Some(crate::repo::Notice::plain((
                                format!("Remote preview: {e}"),
                                true,
                            )))
                        }
                    }
                }
            },
        );
    }
}

impl RepoTab {
    fn prepare_publication(&mut self, manager: publishing::Manager, ctx: &egui::Context) {
        let Some(Dialog::RepositoryTools(state)) = &mut self.dialog else {
            return;
        };
        let id = REQUESTS.fetch_add(1, Ordering::Relaxed);
        state.id = id;
        let repo = self.path.clone();
        self.then_or(
            ctx,
            false,
            Err,
            move || publishing::prepare(&repo, manager),
            move |tab, result, _| {
                if let Some(Dialog::RepositoryTools(state)) = &mut tab.dialog
                    && state.id == id
                {
                    match result {
                        Ok(v) => {
                            state.result = Some(Ok(Data::Publication(v)));
                            state.acknowledged = false;
                        }
                        Err(e) => tab.status = Some(crate::repo::Notice::plain((e, true))),
                    }
                }
            },
        );
    }
    fn publish_group(&mut self, manager: publishing::Manager, retry: bool, ctx: &egui::Context) {
        if self.busy_op.is_some() {
            return;
        }
        if let Some(pause) = crate::network::refused() {
            self.status = Some(crate::repo::Notice::plain((pause.why().into(), true)));
            return;
        }
        let Some(Dialog::RepositoryTools(state)) = &self.dialog else {
            return;
        };
        let id = state.id;
        let control = self.follow_op(&super::ops(&[&["push"]]), false, ctx);
        self.busy_op = Some("Publish group".into());
        self.op_started = Some(std::time::Instant::now());
        self.last_op = None;
        self.timeline_op_started(false, super::Refresh::All);
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
                            &[("failed", (failed).clone().into())],
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
                    super::Refresh::All,
                    super::OpKind::plain(),
                    false,
                    ctx,
                );
                if let Some(Dialog::RepositoryTools(state)) = &mut tab.dialog
                    && state.id == id
                {
                    match result {
                        Ok(m) => state.result = Some(Ok(Data::Publication(m))),
                        Err(e) => tab.status = Some(crate::repo::Notice::plain((e, true))),
                    }
                }
            },
        );
    }
}

impl RepoTab {
    fn dependency_job(&mut self, plan: dependency::Plan, publish: bool, ctx: &egui::Context) {
        if self.busy_op.is_some() {
            return;
        }
        if let Some(pause) = crate::network::refused() {
            self.status = Some(crate::repo::Notice::plain((pause.why().into(), true)));
            return;
        }
        let Some(Dialog::RepositoryTools(state)) = &self.dialog else {
            return;
        };
        let id = state.id;
        let label = if publish {
            "Publish dependencies before parent review"
        } else {
            "Check submodule publication dependencies"
        };
        let control = self.follow_op(&super::ops(&[&["push"]]), false, ctx);
        self.busy_op = Some(label.into());
        self.op_started = Some(std::time::Instant::now());
        self.last_op = None;
        self.timeline_op_started(false, super::Refresh::All);
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
                    super::Refresh::All,
                    super::OpKind::plain(),
                    false,
                    ctx,
                );
                if let Some(Dialog::RepositoryTools(state)) = &mut tab.dialog
                    && state.id == id
                {
                    match result {
                        Ok(p) => {
                            state.result = Some(Ok(Data::Dependencies(p)));
                            state.acknowledged = false;
                        }
                        Err(e) => tab.status = Some(crate::repo::Notice::plain((e, true))),
                    }
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

fn retire_origin_dialog(dialog: &mut Option<Dialog>, origin: Option<u64>) {
    if let Some(Dialog::RepositoryTools(state)) = dialog
        && Some(state.id) == origin
    {
        *dialog = None;
    }
}
fn refs_list(
    ui: &mut egui::Ui,
    refs: &[all_refs::Reference],
    filter: &str,
    acts: &mut Vec<Action>,
) -> usize {
    let filtered: Vec<_> = refs.iter().filter(|r| r.name.contains(filter)).collect();
    let mut rendered = 0;
    let height = ui
        .text_style_height(&egui::TextStyle::Body)
        .max(ui.spacing().interact_size.y);
    egui::ScrollArea::vertical()
        .id_salt("all_refs")
        .max_height(270.0)
        .show_rows(ui, height, filtered.len(), |ui, rows| {
            for index in rows {
                rendered += 1;
                let r = filtered[index];
                ui.horizontal(|ui| {
                    if ui.selectable_label(false, &r.name).clicked() {
                        acts.push(Action::RepositoryTools(Act::AllRefs(r.name.clone())));
                    }
                    ui.monospace(&r.oid);
                    ui.label(&r.kind);
                    if !r.symbolic.is_empty() && ui.link(format!("→ {}", r.symbolic)).clicked() {
                        acts.push(Action::RepositoryTools(Act::AllRefs(r.symbolic.clone())));
                    }
                    if ui
                        .small_button(crate::i18n::text(
                            "repo-workflows-repository_tools-refs-list-edec79bb",
                        ))
                        .clicked()
                    {
                        acts.push(Action::RepositoryTools(Act::Object(r.oid.clone())));
                    }
                    if ui
                        .small_button(crate::i18n::text(
                            "repo-workflows-repository_tools-refs-list-c2a1a3d6",
                        ))
                        .clicked()
                    {
                        ui.ctx().copy_text(r.oid.clone());
                    }
                });
            }
        });
    rendered
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
