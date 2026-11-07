//! Reachable repository internals dialogs; stale background answers never replace a new view.
use super::{Action, Dialog, RepoTab};
use crate::git::repository_tools::{all_refs, guard, object, reftable};
use std::sync::atomic::{AtomicU64, Ordering};
#[derive(Clone, Copy, PartialEq, Eq)]
enum Tool {
    Refs,
    Object,
    Reftable,
}
static REQUESTS: AtomicU64 = AtomicU64::new(1);

pub(crate) enum Act {
    AllRefs(String),
    Object(String),
    Reftable,
    Migrate(Box<reftable::Storage>, String),
}
enum Data {
    Refs(all_refs::Browser),
    Object(object::Inspection),
    Reftable(reftable::Storage),
}
pub(crate) struct State {
    id: u64,
    query: String,
    result: Option<Result<Data, String>>,
    filter: String,
    tool: Tool,
    acknowledged: bool,
    compatible: bool,
}
impl RepoTab {
    pub(super) fn repository_tools_act(&mut self, act: Act, ctx: &egui::Context) {
        let act = match act {
            Act::Migrate(snapshot, target) => {
                self.repository_tool_mutation("Migrate ref storage", false, ctx, move |runner| {
                    reftable::migrate(runner, &snapshot, &target)
                });
                return;
            }
            act => act,
        };
        let (query, tool) = match act {
            Act::AllRefs(q) => (q, Tool::Refs),
            Act::Object(q) => (q, Tool::Object),
            Act::Reftable => (String::new(), Tool::Reftable),
            Act::Migrate(..) => return,
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
                Tool::Refs => all_refs::read(&path, &query).map(Data::Refs),
                Tool::Reftable => reftable::inspect(&path).map(Data::Reftable),
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
        ui.spinner();
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
        ui.spinner();
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
        ui.spinner();
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
