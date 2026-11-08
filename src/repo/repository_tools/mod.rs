mod lists;
mod outbox;
mod sharing;
mod turns;
use crate::git::repository_tools::sharing as borrowing;
mod dependencies;
use crate::git::repository_tools::dependencies as dependency;
mod submodule_conflict;
use crate::git::repository_tools::submodule_conflict as subconflict;
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
    Lists,
}
static REQUESTS: AtomicU64 = AtomicU64::new(1);

pub(crate) enum Act {
    Lists,
    EditList(
        Box<crate::git::drafts::lists::View>,
        crate::git::drafts::lists::Edit,
    ),
    CommitList(Box<crate::git::drafts::lists::View>, String),
    ShelveList(Box<crate::git::drafts::lists::View>, String, bool),
    Turn(String, String, String),
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
                self.repository_tool_mutation(
                    "Force push to confirmed push route",
                    true,
                    ctx,
                    move |runner| routing::force(runner, &plan),
                );
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
                self.repository_tool_mutation(
                    "Push branch via its push route",
                    true,
                    ctx,
                    move |runner| routing::push_branch(runner, &name, &oid),
                );
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
            Act::Lists => (String::new(), Tool::Lists),
            Act::EditList(..) | Act::CommitList(..) | Act::ShelveList(..) => return,
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
                Tool::Object => object::inspect(&path, &query, &|| {
                    !crate::tasks::cancellation().is_cancelled()
                })
                .map(Data::Object),
                Tool::Lists => crate::git::drafts::lists::read(&path, &|| {
                    !crate::tasks::cancellation().is_cancelled()
                })
                .map(Data::Lists),
                Tool::Turn => {
                    let parts = query.splitn(3, '\0').collect::<Vec<_>>();
                    if parts.len() != 3 {
                        return Err("Missing saved turn".into());
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
                    let (name, oid) = query.split_once('\0').ok_or("Missing branch snapshot")?;
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
            self.status = Some((format!("{busy} is still running"), true));
            return;
        }
        if network && let Some(why) = crate::network::refused() {
            self.status = Some((why.why().to_owned(), true));
            return;
        }
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
                work(&guard::Runner {
                    repo: &repo,
                    control: &control,
                    credentials: credentials.as_ref(),
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
                if matches!(tab.dialog, Some(Dialog::RepositoryTools(_))) {
                    tab.dialog = None;
                }
            },
        );
    }
}

pub(super) fn show(ui: &mut egui::Ui, state: &mut State, acts: &mut Vec<Action>) {
    ui.set_width(850.0);
    if state.tool == Tool::Lists {
        lists::show(ui, state, acts);
        return;
    }
    if state.tool == Tool::Turn {
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
    ui.heading("All refs and namespace history");
    ui.label("Includes application, bisect, backup and namespace refs. Object IDs name the exact snapshot below.");
    ui.horizontal(|ui| {
        ui.label("History of ref / prefix (blank = all):");
        ui.text_edit_singleline(&mut state.query);
        if ui.button("Read").clicked() {
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
        ui.label("This Git cannot enumerate arbitrary root refs; established pseudorefs are included when present.");
    }
    ui.horizontal(|ui| {
        ui.label("Filter refs:");
        ui.text_edit_singleline(&mut state.filter);
    });
    egui::ScrollArea::vertical()
        .id_salt("all_refs")
        .max_height(270.0)
        .show(ui, |ui| {
            for r in browser
                .refs
                .iter()
                .filter(|r| r.name.contains(&state.filter))
            {
                ui.horizontal(|ui| {
                    if ui.selectable_label(false, &r.name).clicked() {
                        acts.push(Action::RepositoryTools(Act::AllRefs(r.name.clone())));
                    }
                    ui.monospace(&r.oid);
                    ui.label(&r.kind);
                    if !r.symbolic.is_empty() && ui.link(format!("→ {}", r.symbolic)).clicked() {
                        acts.push(Action::RepositoryTools(Act::AllRefs(r.symbolic.clone())));
                    }
                    if ui.small_button("Inspect object").clicked() {
                        acts.push(Action::RepositoryTools(Act::Object(r.oid.clone())));
                    }
                    if ui.small_button("Copy object").clicked() {
                        ui.ctx().copy_text(r.oid.clone());
                    }
                });
            }
        });
    ui.separator();
    ui.label("History (up to 200 commits)");
    egui::ScrollArea::both()
        .id_salt("namespace_history")
        .max_height(260.0)
        .show(ui, |ui| {
            ui.monospace(&browser.history);
        });
}

fn object_view(ui: &mut egui::Ui, state: &mut State, acts: &mut Vec<Action>) {
    ui.heading("How an object is stored");
    ui.label("Object ID or file version, for example HEAD:src/main.rs. Inspection never downloads missing objects. Git 2.45+ is required for this guarantee.");
    ui.horizontal(|ui| {
        ui.text_edit_singleline(&mut state.query);
        if ui.button("Inspect").clicked() {
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
    egui::ScrollArea::both().id_salt("object_inspector").max_height(600.0).show(ui, |ui| {
        ui.label(format!("{} · {} · resolved from {}", data.format, data.objects.display(), data.query));
        ui.label(format!("Git directory: {}\nShared directory: {}", data.git_dir, data.common_dir));
        if let Some(o) = &data.object {
            ui.monospace(&o.oid); ui.label(format!("{} · {} bytes expanded · {} bytes on disk", o.kind, o.size, o.disk));
            if let Some(base) = &o.base && ui.link(format!("Delta base: {base}")).clicked() { acts.push(Action::RepositoryTools(Act::Object(base.clone()))); }
        }
        if let Some(missing) = &data.missing { ui.label(missing); }
        for note in &data.notes { ui.label(note); }
        for representation in &data.representations {
            ui.separator(); ui.monospace(representation.path.display().to_string());
            if let Some(n) = representation.loose_bytes { ui.label(format!("Loose object: {n} compressed bytes")); }
            for (i, row) in representation.chain.iter().enumerate() {
                if ui.link(format!("{}: {} · {} · packed {} B · representation {} B · offset {} · depth {}", if i == 0 { "Object" } else { "Base" }, row.oid, row.kind, row.disk, row.size, row.offset, row.depth)).clicked() { acts.push(Action::RepositoryTools(Act::Object(row.oid.clone()))); }
            }
            if representation.chain.iter().any(|r| r.base.is_some()) { ui.label("For deltas, representation size describes the delta instruction stream; expanded object size is above."); }
        }
        ui.separator(); ui.label("Tree entries / linked objects");
        for entry in &data.entries {
            if ui.link(format!("{} · {} · {}", entry.name, entry.kind, entry.oid)).clicked() { acts.push(Action::RepositoryTools(Act::Object(entry.oid.clone()))); }
        }
    });
}

fn reftable_view(ui: &mut egui::Ui, state: &mut State, acts: &mut Vec<Action>) {
    ui.heading("Ref storage and guided migration");
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
    ui.label(format!(
        "{} · current storage: {}",
        storage.git, storage.format
    ));
    ui.label("Reftable requires Git 2.45+ and compatible third-party tools. Some IDEs, embedded Git libraries, backup tools and older clients only support files. Check every tool that accesses this repository before migration. Files is the broadest compatibility choice.");
    ui.label("Migration preserves refs and reflogs, refuses linked worktrees, and verifies both afterwards. Stop all other programs that write to this repository. Keep a repository backup before proceeding.");
    if let Some(why) = &storage.blocked {
        ui.label(why);
    }
    ui.checkbox(
        &mut state.acknowledged,
        "All other writers are stopped and I have a backup",
    );
    ui.checkbox(
        &mut state.compatible,
        "I checked compatibility of my installed tools with the target format",
    );
    let target = if storage.format == "files" {
        "reftable"
    } else {
        "files"
    };
    if ui
        .add_enabled(
            storage.migrates && storage.blocked.is_none() && state.acknowledged && state.compatible,
            egui::Button::new(format!("Migrate to {target} and verify")),
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
    ui.heading("Replace refs and temporary history joins");
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
    ui.label("Replace refs affect these commits and their descendants. Joins below are local and temporary; removing the replace ref restores the original history.");
    ui.checkbox(
        &mut state.acknowledged,
        "I reviewed these local history changes",
    );
    for r in &data.replacements {
        ui.horizontal(|ui| {
            if ui
                .link(format!("Affected commit {} → {}", r.original, r.effective))
                .clicked()
            {
                acts.push(Action::RepositoryTools(Act::Object(r.effective.clone())));
            }
            if ui
                .add_enabled(state.acknowledged, egui::Button::new("Remove link"))
                .clicked()
            {
                acts.push(Action::RepositoryTools(Act::RemoveReplacement(
                    Box::new(data.clone()),
                    r.original.clone(),
                    r.effective.clone(),
                )));
            }
        });
        ui.label(format!("Effective parents: {}", r.parents));
    }
    if !data.legacy.is_empty() {
        ui.label("Legacy info/grafts file (converted links remain visible/removable here):");
        ui.monospace(&data.legacy);
        if ui
            .add_enabled(
                state.acknowledged,
                egui::Button::new("Convert legacy grafts to replace refs"),
            )
            .clicked()
        {
            acts.push(Action::RepositoryTools(Act::ConvertGrafts(Box::new(
                data.clone(),
            ))));
        }
    }
    ui.horizontal(|ui| {
        ui.selectable_value(&mut state.effective, true, "Effective history");
        ui.selectable_value(
            &mut state.effective,
            false,
            "Original history (--no-replace-objects)",
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
    ui.label("Join imported histories: replace this commit's parents with a commit from the other history. Paste full commit IDs. Cyclic joins and existing replacements are refused.");
    ui.horizontal(|ui| {
        ui.label("Commit:");
        ui.text_edit_singleline(&mut state.query);
    });
    ui.horizontal(|ui| {
        ui.label("New parent:");
        ui.text_edit_singleline(&mut state.parent);
    });
    if ui
        .add_enabled(
            state.acknowledged && !state.query.is_empty() && !state.parent.is_empty(),
            egui::Button::new("Create temporary graft"),
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
                        Err(e) => tab.status = Some((format!("Remote preview: {e}"), true)),
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
                        Err(e) => tab.status = Some((e, true)),
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
            self.status = Some((pause.why().into(), true));
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
        self.then_or(ctx, true, Err, move || publishing::publish(&guard::Runner { repo: &repo, control: &control, credentials: credentials.as_ref() }, manager, retry), move |tab, result, ctx| {
            let outcome = result.as_ref().map_err(Clone::clone).and_then(|m| {
                let failed = m.plan.as_ref().map_or(0, |p| p.destinations.iter().filter(|d| matches!(d.result, Some(Err(_)))).count());
                if failed > 0 { Err(format!("Publication incomplete: {failed} destinations failed. Inspect per-destination results.")) } else { Ok("Every destination published the prepared commit".into()) }
            });
            tab.op_done("Publish group".into(), outcome, super::Refresh::All, super::OpKind::plain(), false, ctx);
            if let Some(Dialog::RepositoryTools(state)) = &mut tab.dialog && state.id == id {
                match result { Ok(m) => state.result = Some(Ok(Data::Publication(m))), Err(e) => tab.status = Some((e, true)) }
            }
        });
    }
}

impl RepoTab {
    fn dependency_job(&mut self, plan: dependency::Plan, publish: bool, ctx: &egui::Context) {
        if self.busy_op.is_some() {
            return;
        }
        if let Some(pause) = crate::network::refused() {
            self.status = Some((pause.why().into(), true));
            return;
        }
        let Some(Dialog::RepositoryTools(state)) = &self.dialog else {
            return;
        };
        let id = state.id;
        let label = if publish {
            "Publish dependencies, then parent"
        } else {
            "Check submodule publication dependencies"
        };
        let control = self.follow_op(&super::ops(&[&["push"]]), false, ctx);
        self.busy_op = Some(label.into());
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
                let runner = guard::Runner {
                    repo: &repo,
                    control: &control,
                    credentials: credentials.as_ref(),
                };
                if publish {
                    dependency::publish(&runner, plan)
                } else {
                    dependency::check(&runner, plan)
                }
            },
            move |tab, result, ctx| {
                let outcome = result.as_ref().map_err(Clone::clone).and_then(|p| {
                    if p.parent_result.as_ref().is_some_and(Result::is_err) {
                        Err("Parent publication stopped; inspect per-repository outcomes".into())
                    } else if p.dependencies.iter().any(|n| n.problem.is_some()) {
                        Err("Dependency check found blockers; inspect each submodule".into())
                    } else {
                        Ok("Dependency operation completed".into())
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
                        Err(e) => tab.status = Some((e, true)),
                    }
                }
            },
        );
    }
}
