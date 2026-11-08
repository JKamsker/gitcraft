use super::Outcome;
use crate::git::{BundlePreview, BundleRef};
use crate::repo::Action;
use crate::theme::Tokens;
use egui::Ui;
use std::collections::BTreeSet;

pub(super) fn export(
    ui: &mut Ui,
    refs: &[BundleRef],
    selected: &mut BTreeSet<String>,
    base: &mut String,
    idle: bool,
) -> Outcome {
    ui.set_width(650.0);
    ui.heading("Export Git bundle");
    if refs.is_empty() {
        ui.label("This repository has no committed refs to export.");
    }
    ui.label("Choose refs from this snapshot. The file is saved on this machine, including when the repository is on SSH / WSL.");
    egui::ScrollArea::vertical()
        .max_height(280.0)
        .id_salt("bundle_export_refs")
        .show(ui, |ui| {
            for r in refs {
                let mut on = selected.contains(&r.name);
                if ui
                    .checkbox(
                        &mut on,
                        format!("{} · {}", r.name, &r.oid[..8.min(r.oid.len())]),
                    )
                    .changed()
                {
                    if on {
                        selected.insert(r.name.clone());
                    } else {
                        selected.remove(&r.name);
                    }
                }
            }
        });
    ui.separator();
    ui.label("Optional range: exclude history reachable from this base commit");
    ui.text_edit_singleline(base);
    ui.label("Blank includes full reachable history. An incremental bundle requires the recipient to already have its prerequisite commits. Limit: 128 MiB; 2 minute Git deadline.");
    let mut outcome = Outcome::Open;
    ui.horizontal(|ui| {
        if ui
            .add_enabled(
                !selected.is_empty() && idle,
                egui::Button::new("Choose new output file…"),
            )
            .clicked()
        {
            outcome = Outcome::Done(Box::new(Action::ExportBundle {
                refs: refs
                    .iter()
                    .filter(|r| selected.contains(&r.name))
                    .cloned()
                    .collect(),
                base: base.clone(),
            }));
        }
        if ui.button("Cancel").clicked() {
            outcome = Outcome::Cancel;
        }
    });
    outcome
}

enum Event {
    Done(bool, Result<Vec<BundleRef>, String>),
}
pub struct Form {
    repo: std::path::PathBuf,
    label: String,
    preview: BundlePreview,
    selected: BTreeSet<String>,
    prefix: String,
    scope: crate::tasks::Scope,
    active: Option<crate::tasks::Cancellation>,
    tx: std::sync::mpsc::Sender<Event>,
    rx: std::sync::mpsc::Receiver<Event>,
    consent: bool,
    confirm: bool,
    verified: bool,
    imported: Vec<BundleRef>,
    error: Option<String>,
}
impl super::Dialog {
    pub(in crate::repo) fn bundle_incoming(
        repo: std::path::PathBuf,
        token: crate::tasks::Cancellation,
        label: String,
        preview: BundlePreview,
    ) -> Self {
        let (tx, rx) = std::sync::mpsc::channel();
        let selected = preview.refs.iter().map(|r| r.name.clone()).collect();
        Self::BundleIncoming(Box::new(Form {
            repo,
            label,
            preview,
            selected,
            prefix: "incoming".into(),
            scope: crate::tasks::Scope(token),
            active: None,
            tx,
            rx,
            consent: false,
            confirm: false,
            verified: false,
            imported: Vec::new(),
            error: None,
        }))
    }
}
impl Form {
    fn start(&mut self, import: bool, ctx: &egui::Context) {
        let pool = match crate::tasks::pool() {
            Ok(pool) => pool,
            Err(e) => {
                self.error = Some(e);
                return;
            }
        };
        let repo = self.repo.clone();
        let preview = self.preview.clone();
        let selected = self.selected.clone();
        let prefix = self.prefix.clone();
        let token = self.scope.0.child();
        let tx = self.tx.clone();
        let ctx = ctx.clone();
        let failed_tx = tx.clone();
        let failed_ctx = ctx.clone();
        let key = Some(crate::repo::messages::mutation_key(&repo));
        let result = pool.submit_cancellable(token.clone(), crate::tasks::Priority::Interactive, key,
            Some(Box::new(move || { let _ = failed_tx.send(Event::Done(import, Err("Cancelled before execution".into()))); failed_ctx.request_repaint(); })),
            Box::new(move || {
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(||
                    crate::git::check_bundle(&repo, &preview, &selected, &prefix, import)))
                    .unwrap_or_else(|_| Err("Bundle worker failed unexpectedly; inspect retained recovery evidence before retrying".into()));
                let _ = tx.send(Event::Done(import, result)); ctx.request_repaint();
            }));
        match result {
            Ok(()) => {
                self.active = Some(token);
                self.error = None;
                self.confirm = false;
            }
            Err(e) => self.error = Some(e.into()),
        }
    }
    fn receive(&mut self) {
        while let Ok(Event::Done(import, result)) = self.rx.try_recv() {
            self.active = None;
            // Preserve ownership/cleanup failures even after the user pressed Stop.
            match result {
                Ok(refs) if import => {
                    self.imported = refs;
                    self.error = None;
                }
                Ok(_) => {
                    self.verified = true;
                    self.error = None;
                }
                Err(e) => {
                    self.error = Some(e);
                    self.confirm = false;
                }
            }
        }
    }
    pub(super) fn show(&mut self, ui: &mut Ui) -> Outcome {
        self.receive();
        ui.set_width(740.0);
        let mut outcome = Outcome::Open;
        let height = (ui.ctx().content_rect().height() - 100.0).max(180.0);
        egui::ScrollArea::vertical().max_height(height).show(ui, |ui| {
            let t = Tokens::get(ui.ctx());
            ui.heading("Inspect and import Git bundle");
            ui.label(format!("Local file snapshot: {}", self.label));
            ui.monospace(format!("SHA-256 {}", self.preview.digest));
            ui.label("Inspection checks the header/prerequisites only. Pack contents have not been validated until Verify succeeds.");
            if let Some(error) = &self.preview.verification_error { ui.colored_label(t.danger, format!("Header/prerequisite check: {error}")); }
            ui.label("Verify creates a private quarantine on the repository machine and strictly checks the pack, objects and connectivity. Import repeats verification and creates only the new refs listed below. It never checks out files or overwrites existing refs. Failed/cancelled imports may leave unreferenced objects. A lost completion acknowledgement can also leave the new refs already created: inspect this group before retrying. Uncertain cleanup retains recovery evidence.");
            ui.label("Limits: input 128 MiB; enforced memory ceiling 1 GiB per process on Linux / 1 GiB aggregate committed memory and 8 processes on Windows; 120 seconds per Git operation; one pack thread. Linux also enforces a 512 MiB file limit. These are resource limits for Git data parsing, not a sandbox for arbitrary programs.");
            let supported = crate::remote::is_remote(&self.repo) || cfg!(any(target_os = "linux", windows));
            if !supported { ui.colored_label(t.danger, "This platform cannot establish the required memory containment. Unpacking/import is available on Linux, Windows, and Linux SSH/WSL hosts. Inspection and export remain available here."); }
            ui.label("SSH/WSL import requires Python 3 and Linux resource limits. Unsupported hosts refuse before opening pack data. macOS unpacking is refused because a pre-exec address-space limit alone cannot prove enforcement after exec.");
            ui.collapsing(format!("Prerequisites ({})", self.preview.prerequisites.len()), |ui| {
                if self.preview.prerequisites.is_empty() { ui.label("None: full bundle"); }
                for r in &self.preview.prerequisites { ui.monospace(format!("{} {}", r.oid, r.name)); }
            });
            let editable = self.active.is_none() && self.imported.is_empty();
            let before = (self.prefix.clone(), self.selected.clone());
            ui.add_enabled_ui(editable, |ui| {
                ui.label("New ref group (lowercase letters, digits, underscores, hyphens):");
                ui.text_edit_singleline(&mut self.prefix);
                ui.label("Incoming refs to import:");
                egui::ScrollArea::vertical().max_height(190.0).id_salt("bundle_ref_selection").show(ui, |ui| {
                    for r in &self.preview.refs {
                        let mut selected = self.selected.contains(&r.name);
                        if ui.checkbox(&mut selected, format!("{} · {}", r.name, r.oid)).changed() {
                            if selected { self.selected.insert(r.name.clone()); } else { self.selected.remove(&r.name); }
                        }
                    }
                });
            });
            if before != (self.prefix.clone(), self.selected.clone()) { self.confirm = false; }
            let mapping = crate::git::import_mapping(&self.preview.refs, &self.selected, &self.prefix);
            egui::CollapsingHeader::new("Exact new refs (existing names refuse the transaction)").default_open(true).show(ui, |ui| {
                egui::ScrollArea::vertical().max_height(160.0).id_salt("bundle_exact_refs").show(ui, |ui| {
                    match &mapping { Ok(refs) => { for r in refs { ui.monospace(format!("{} {}", r.oid, r.name)); } },
                        Err(e) => { ui.colored_label(t.danger, e); } }
                });
            });
            if self.verified { ui.label("The held snapshot passed strict quarantine verification. Import will verify it again against this destination."); }
            if !self.imported.is_empty() {
                ui.label("Import completed. Created refs:");
                for r in &self.imported { ui.horizontal(|ui| { ui.monospace(&r.name); if ui.small_button("Copy ref").clicked() { ui.ctx().copy_text(r.name.clone()); } }); }
            }
            if let Some(error) = &self.error { ui.colored_label(t.danger, error); }
            if self.active.is_some() {
                ui.add(crate::spinner::Spinner::new());
                if ui.button("Stop").clicked() && let Some(token) = &self.active { token.cancel(); }
            } else if self.imported.is_empty() {
                ui.add_enabled_ui(supported && mapping.is_ok(), |ui| {
                    ui.checkbox(&mut self.consent, "Allow private quarantine creation and contained pack verification on the repository machine");
                    if ui.add_enabled(self.consent, egui::Button::new("Verify complete pack")).clicked() { self.start(false, ui.ctx()); }
                    ui.add_enabled_ui(self.verified, |ui| {
                        ui.checkbox(&mut self.confirm, format!("Create {} new refs under refs/bundle-import/{}/", self.selected.len(), self.prefix));
                        if ui.add_enabled(self.confirm, egui::Button::new("Verify again and import selected refs")).clicked() { self.start(true, ui.ctx()); }
                    });
                });
            }
            if ui.button(if self.imported.is_empty() { "Close" } else { "Close and refresh repository" }).clicked() {
                outcome = if self.imported.is_empty() { Outcome::Cancel } else { Outcome::Done(Box::new(Action::Reload)) };
            }
        });
        outcome
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn incoming_dialog_is_read_only_and_escape_cancels_without_ref_changes() -> Result<(), String> {
        use crate::git;
        use crate::repo::{Dialog, RepoTab};
        let root = std::env::temp_dir().join(format!("gitcraft-bundle-ui-{}", std::process::id()));
        std::fs::create_dir(&root).map_err(|e| e.to_string())?;
        struct Cleanup(std::path::PathBuf);
        impl Drop for Cleanup {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
        let _cleanup = Cleanup(root.clone());
        for args in [
            vec!["init", "-q"],
            vec!["config", "user.name", "Test"],
            vec!["config", "user.email", "test@example.com"],
            vec![
                "-c",
                "commit.gpgSign=false",
                "commit",
                "--allow-empty",
                "-qm",
                "one",
            ],
        ] {
            git::run(Some(&root), &args)?;
        }
        let before = git::run(
            Some(&root),
            &["for-each-ref", "--format=%(refname) %(objectname)"],
        )?;
        let refs = git::bundle_refs(&root)?;
        let head = refs.iter().find(|r| r.name == "HEAD").ok_or("no HEAD")?;
        let preview = git::inspect_bundle(
            &root,
            git::export_bundle(&root, std::slice::from_ref(head), None)?,
        )?;
        let ctx = egui::Context::default();
        crate::theme::install_fonts(&ctx);
        let mut tab = RepoTab::new_deferred(root.clone(), &ctx);
        tab.dialog = Some(Dialog::bundle_incoming(
            root.clone(),
            tab._jobs.0.child(),
            "selected.bundle".into(),
            preview,
        ));
        let mut actions = Vec::new();
        let mut text = String::new();
        for frame in 0..3 {
            let mut input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1100.0, 900.0),
                )),
                ..Default::default()
            };
            if frame == 2 {
                input.events.push(egui::Event::Key {
                    key: egui::Key::Escape,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                });
            }
            let mut output = ctx.run_ui(input, |ui| tab.dialogs(ui.ctx(), &mut actions));
            text.push_str(
                &output
                    .shapes
                    .iter()
                    .filter_map(|s| match &s.shape {
                        egui::Shape::Text(t) => Some(t.galley.text()),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
                    .join("\n"),
            );
            output.textures_delta.clear();
        }
        assert!(text.contains("Inspection checks the header"), "{text}");
        assert!(text.contains("SHA-256"), "{text}");
        assert!(tab.dialog.is_none());
        assert!(actions.is_empty());
        assert_eq!(
            git::run(
                Some(&root),
                &["for-each-ref", "--format=%(refname) %(objectname)"]
            )?,
            before
        );
        tab.bundle_done(
            "Import bundle refs".into(),
            Err("fatal: Authentication failed for untrusted prerequisite text".into()),
            crate::repo::Refresh::Status,
            &ctx,
        );
        assert!(tab.dialog.is_none());
        assert!(
            tab.status
                .as_ref()
                .is_some_and(|(text, error)| *error && text.contains("Authentication failed"))
        );
        Ok(())
    }
}

#[cfg(test)]
mod recording_import_ui_tests {
    use super::*;
    #[test]
    fn confirmation_is_explicit_and_late_owned_cleanup_errors_survive_stop() -> Result<(), String> {
        let oid = "a".repeat(40);
        let bytes = format!("# v2 git bundle\n{oid} refs/heads/main\n\nPACK").into_bytes();
        let preview = BundlePreview {
            refs: vec![BundleRef {
                name: "refs/heads/main".into(),
                oid,
            }],
            prerequisites: vec![],
            verification_error: None,
            digest: crate::git::bundle_digest(&bytes),
            bytes: std::sync::Arc::new(bytes),
            repository: "/recording/common".into(),
        };
        let dialog = super::super::Dialog::bundle_incoming(
            "/recording/repo".into(),
            crate::tasks::Cancellation::default(),
            "held.bundle".into(),
            preview,
        );
        let super::super::Dialog::BundleIncoming(mut form) = dialog else {
            return Err("wrong dialog".into());
        };
        assert!(!form.consent && !form.confirm && !form.verified && form.active.is_none());
        let token = form.scope.0.child();
        token.cancel();
        form.active = Some(token);
        form.tx
            .send(Event::Done(
                true,
                Err("cancelled; owned cleanup unconfirmed; ledger retained".into()),
            ))
            .map_err(|error| error.to_string())?;
        form.receive();
        assert!(
            form.error
                .as_deref()
                .is_some_and(|error| error.contains("owned cleanup unconfirmed"))
        );
        assert!(form.active.is_none() && form.imported.is_empty() && !form.confirm);
        let ctx = egui::Context::default();
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1000.0, 1400.0),
            )),
            ..Default::default()
        };
        let mut output = ctx.run_ui(input, |ui| {
            assert!(matches!(form.show(ui), Outcome::Open));
        });
        output.textures_delta.clear();
        assert!(form.active.is_none(), "rendering must not submit a job");
        assert!(!form.consent && !form.confirm && !form.verified);
        Ok(())
    }
}
