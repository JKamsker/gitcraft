//! The inspector owns a pinned request; old tabs, profile edits and reloads cannot publish it.
use super::{Action, Dialog, RepoTab};
use crate::{connections, git::object_storage as storage};
use std::path::PathBuf;
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};

static REQUESTS: AtomicU64 = AtomicU64::new(1);
pub struct State {
    request: u64,
    generation: Arc<AtomicU64>,
    path: PathBuf,
    profile: connections::Context,
    read: u64,
    query: String,
    file: String,
    loading: bool,
    result: Option<Result<storage::Inspection, String>>,
    independence_ack: bool,
}
impl RepoTab {
    pub(super) fn open_object_storage(&mut self, query: Option<String>, ctx: &egui::Context) {
        let query = query.unwrap_or_else(|| {
            self.selected_hash()
                .filter(|s| s != super::WIP_HASH)
                .unwrap_or_else(|| "HEAD".into())
        });
        let request = REQUESTS.fetch_add(1, Ordering::Relaxed);
        let generation = Arc::new(AtomicU64::new(request));
        let weak = Arc::downgrade(&generation);
        let path = self.path.clone();
        let profile = connections::current();
        let read = self.reads;
        if let Some(Dialog::ObjectStorage(old)) = &self.dialog {
            old.generation.store(0, Ordering::Relaxed);
        }
        self.dialog = Some(Dialog::ObjectStorage(Box::new(State {
            request,
            generation,
            path: path.clone(),
            profile: profile.clone(),
            read,
            query: query.clone(),
            file: String::new(),
            loading: true,
            result: None,
            independence_ack: false,
        })));
        self.then(
            ctx,
            move || {
                storage::inspect(&path, &query, &|| {
                    weak.upgrade()
                        .is_some_and(|g| g.load(Ordering::Relaxed) == request)
                })
            },
            move |tab, result, _| {
                let Some(Dialog::ObjectStorage(state)) = &mut tab.dialog else {
                    return;
                };
                if state.request != request
                    || tab.path != state.path
                    || tab.reads != read
                    || connections::current() != profile
                {
                    return;
                }
                state.loading = false;
                state.result = Some(result);
            },
        );
    }
    pub(super) fn make_object_storage_independent(
        &mut self,
        snapshot: Box<storage::independent::Snapshot>,
        ctx: &egui::Context,
    ) {
        if self.busy_op.is_some() {
            return;
        }
        if snapshot.repo != self.path {
            self.status = Some((
                "Repository changed; prepare independence again.".into(),
                true,
            ));
            return;
        }
        let repo = self.path.clone();
        let profile = connections::current();
        self.last_op = None;
        self.busy_op = Some("Copying and verifying borrowed objects".into());
        self.op_started = Some(std::time::Instant::now());
        self.then(
            ctx,
            move || storage::independent::make_independent(&snapshot, &|| true),
            move |tab, result, ctx| {
                tab.finish_object_storage_independence(&repo, &profile, result, ctx);
            },
        );
    }
    fn finish_object_storage_independence(
        &mut self,
        repo: &std::path::Path,
        profile: &connections::Context,
        result: Result<String, String>,
        ctx: &egui::Context,
    ) {
        self.busy_op = None;
        self.op_started = None;
        if self.path != repo || connections::current() != *profile {
            return;
        }
        self.status = Some(match result {
            Ok(message) => (message, false),
            Err(error) => (format!("Make clone independent failed: {error}"), true),
        });
        self.reload(ctx);
    }
    pub(super) fn sync_object_storage(&mut self) {
        if let Some(Dialog::ObjectStorage(state)) = &mut self.dialog
            && (state.path != self.path
                || state.read != self.reads
                || state.profile != connections::current())
        {
            state.generation.store(0, Ordering::Relaxed);
            state.loading = false;
            state.result = Some(Err(
                "Repository or profile changed. Inspect again to read a fresh snapshot.".into(),
            ));
        }
    }
}

pub(super) fn dialog(ui: &mut egui::Ui, state: &mut State) -> super::dialogs::Outcome {
    use super::dialogs::Outcome;
    ui.set_width(720.0);
    ui.heading("How an object is stored");
    ui.label("Read-only inspection on the repository's machine. Objects are never downloaded.");
    let mut outcome = Outcome::Open;
    ui.horizontal(|ui| {
        ui.label("Object / revision");
        ui.text_edit_singleline(&mut state.query);
        if ui.button("Inspect").clicked() {
            outcome = Outcome::Done(Action::InspectObject(state.query.clone()));
        }
    });
    ui.horizontal(|ui| {
        ui.label("File at revision");
        ui.text_edit_singleline(&mut state.file);
        if ui
            .add_enabled(
                !state.file.is_empty(),
                egui::Button::new("Inspect file version"),
            )
            .clicked()
        {
            outcome = Outcome::Done(Action::InspectObject(format!(
                "{}:{}",
                state.query, state.file
            )));
        }
    });
    if state.loading {
        ui.add(crate::spinner::Spinner::new());
        ui.label("Reading object metadata and checking packs…");
    }
    egui::ScrollArea::vertical().max_height(500.0).show(ui, |ui| {
        if let Some(result) = &state.result { match result {
            Err(e) => { ui.colored_label(ui.visuals().error_fg_color, e); },
            Ok(data) => {
                ui.label(format!("Resolved from {} · {}", data.query, data.format));
                ui.label(format!("Git directory: {}", data.git_dir));
                ui.label(format!("Common directory: {}", data.common_dir));
                ui.label(format!("Primary objects: {}", crate::remote::display(&data.objects)));
                ui.separator();
                ui.strong("Shared and borrowed objects");
                ui.label("Linked worktrees below share the common Git directory and its object ownership:");
                ui.monospace(&data.topology.worktrees);
                for store in &data.topology.stores {
                    let owner = store.donor_of.as_ref().map(|p| format!("Donor for {}", crate::remote::display(p))).unwrap_or_else(|| "Primary object store".into());
                    ui.label(format!("{owner}: {} · {}", crate::remote::display(&store.path), if store.available { "available" } else { "MISSING / unavailable" }));
                    if let Some(error) = &store.error { ui.colored_label(ui.visuals().warn_fg_color, error); }
                }
                ui.label("Objects shown in a donor's loose file or pack are borrowed: deleting or pruning that donor may make this repository unreadable. A linked worktree shares objects rather than owning a separate clone.");
                if let Some(snapshot) = &data.independence {
                    ui.label("Make independent copies every object needed by refs, reflogs and the index into a self-contained local pack. Only after coverage and connectivity proofs succeed is the primary alternate link removed. Donors stay untouched; an alternate backup is retained. Copies are limited to 128 MiB per pack.");
                    if crate::remote::is_remote(&state.path) { ui.label("If the connection is interrupted, use Inspect to check the retained terminal receipt before another action. An unconfirmed attempt blocks replacement; its recovery files are kept."); }
                    ui.checkbox(&mut state.independence_ack, "I have stopped other Git writers for this clone and want to remove its borrowing link.");
                    if ui.add_enabled(state.independence_ack && !state.loading, egui::Button::new("Make this clone independent")).clicked() { outcome = Outcome::Done(Action::MakeObjectStorageIndependent(Box::new(snapshot.clone()))); }
                }
                ui.separator();
                if let Some(object) = &data.object {
                    ui.monospace(&object.oid);
                    ui.label(format!("{} · {} logical bytes · {} bytes in Git's selected representation", object.kind, object.size, object.disk));
                    if let Some(base) = &object.base && ui.button(format!("Inspect selected delta base {base}")).clicked() { outcome = Outcome::Done(Action::InspectObject(base.clone())); }
                }
                if let Some(missing) = &data.missing { ui.colored_label(ui.visuals().warn_fg_color, missing); }
                for note in &data.notes { ui.label(note); }
                for representation in &data.representations {
                    ui.separator();
                    ui.monospace(crate::remote::display(&representation.path));
                    if let Some(size) = representation.loose_bytes { ui.label(format!("Loose object · {size} compressed bytes (including its header)")); }
                    for (i, row) in representation.chain.iter().enumerate() {
                        ui.horizontal_wrapped(|ui| {
                            if ui.link(format!("{}{}", if i == 0 { "Object " } else { "Base " }, row.oid)).clicked() { outcome = Outcome::Done(Action::InspectObject(row.oid.clone())); }
                            ui.label(format!("{} · {} inflated entry / {} packed bytes · offset {} · depth {}", row.kind, row.size, row.disk, row.offset, row.depth));
                        });
                    }
                }
                if !data.entries.is_empty() { ui.separator(); ui.strong("Tree entries and object links"); }
                for entry in &data.entries {
                    if ui.link(format!("{} · {} · {}", entry.name, entry.kind, entry.oid)).clicked() { outcome = Outcome::Done(Action::InspectObject(entry.oid.clone())); }
                }
            }
        }}
    });
    ui.label("For delta rows, inflated entry size is the delta program size; the object header above gives the logical content size.");
    ui.label("Limits: 64 packs across all stores, 16 MiB per Git response, 2,000 tree entries and 128 delta levels; each Git analysis has a deadline.");
    if ui.button("Close").clicked() {
        outcome = Outcome::Cancel;
    }
    outcome
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn stale_paths_profiles_and_ref_reload_epochs_invalidate_inspector() {
        let ctx = egui::Context::default();
        let mut tab = RepoTab::new_deferred(PathBuf::from("/fixture"), &ctx);
        let make = |path: PathBuf, profile, read| State {
            request: 1,
            generation: Arc::new(AtomicU64::new(1)),
            path,
            profile,
            read,
            query: "HEAD".into(),
            file: String::new(),
            loading: true,
            result: None,
            independence_ack: false,
        };
        tab.dialog = Some(Dialog::ObjectStorage(Box::new(make(
            tab.path.clone(),
            connections::current(),
            tab.reads,
        ))));
        tab.path = PathBuf::from("/other");
        tab.sync_object_storage();
        assert!(
            matches!(&tab.dialog, Some(Dialog::ObjectStorage(s)) if !s.loading && s.generation.load(Ordering::Relaxed)==0 && matches!(s.result, Some(Err(_))))
        );
        tab.dialog = Some(Dialog::ObjectStorage(Box::new(make(
            tab.path.clone(),
            connections::Context {
                error: Some("old profile".into()),
                ..Default::default()
            },
            tab.reads,
        ))));
        tab.sync_object_storage();
        assert!(matches!(&tab.dialog, Some(Dialog::ObjectStorage(s)) if !s.loading));
        tab.dialog = Some(Dialog::ObjectStorage(Box::new(make(
            tab.path.clone(),
            connections::current(),
            tab.reads.wrapping_sub(1),
        ))));
        tab.sync_object_storage();
        assert!(matches!(&tab.dialog, Some(Dialog::ObjectStorage(s)) if !s.loading));
        assert!(
            tab.palette_items()
                .iter()
                .any(|(label, _, _)| label.contains("How an object"))
        );
        assert!(
            tab.palette_items()
                .iter()
                .any(|(label, _, _)| label.contains("Shared and borrowed"))
        );
    }
    #[test]
    fn operation_completions_do_not_publish_into_another_tab_path_or_profile() {
        let ctx = egui::Context::default();
        let mut tab = RepoTab::new_deferred(PathBuf::from("/current"), &ctx);
        tab.busy_op = Some("independence".into());
        tab.finish_object_storage_independence(
            std::path::Path::new("/previous"),
            &connections::current(),
            Err("private result".into()),
            &ctx,
        );
        assert!(tab.status.is_none());
        assert!(tab.busy_op.is_none());
        tab.finish_object_storage_independence(
            std::path::Path::new("/current"),
            &connections::Context {
                error: Some("previous profile".into()),
                ..Default::default()
            },
            Err("private result".into()),
            &ctx,
        );
        assert!(tab.status.is_none());
    }
}
