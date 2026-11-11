use super::Outcome;
use crate::git::{BundlePreview, BundleRef};
use crate::parallel_bisect::presentation::{self, Presentation};
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
    ui.heading(crate::i18n::text("repo-core-1234"));
    if refs.is_empty() {
        ui.label(crate::i18n::text("repo-core-1235"));
    }
    ui.label(crate::i18n::text("repo-core-1236"));
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
    ui.label(crate::i18n::text("repo-core-1238"));
    ui.text_edit_singleline(base);
    ui.label(crate::i18n::text("repo-core-1239"));
    let mut outcome = Outcome::Open;
    ui.horizontal(|ui| {
        if ui
            .add_enabled(
                !selected.is_empty() && idle,
                egui::Button::new(crate::i18n::text("repo-core-1240")),
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
        if ui.button(crate::i18n::text("repo-core-0443")).clicked() {
            outcome = Outcome::Cancel;
        }
    });
    outcome
}

enum Event {
    Done(bool, Result<Vec<BundleRef>, String>),
    DoneWithPresentation(bool, Result<Vec<BundleRef>, String>, Option<Presentation>),
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
    error_presentation: Option<Presentation>,
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
            error_presentation: None,
        }))
    }
}
impl Form {
    fn start(&mut self, import: bool, ctx: &egui::Context) {
        self.error_presentation = None;
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
        let result = pool.submit_cancellable(
            token.clone(),
            crate::tasks::Priority::Interactive,
            key,
            Some(Box::new(move || {
                let _ = failed_tx.send(Event::Done(
                    import,
                    Err(crate::i18n::text("repo-core-1159").into()),
                ));
                failed_ctx.request_repaint();
            })),
            Box::new(move || {
                let mut presentation = None;
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    crate::git::check_bundle_with_presentation(
                        &repo,
                        &preview,
                        &selected,
                        &prefix,
                        import,
                        &mut presentation,
                    )
                }))
                .unwrap_or_else(|_| {
                    presentation = None;
                    Err(crate::i18n::text("repo-core-1242").into())
                });
                presentation::retain_error(&result, &mut presentation);
                let _ = tx.send(Event::DoneWithPresentation(import, result, presentation));
                ctx.request_repaint();
            }),
        );
        match result {
            Ok(()) => {
                self.active = Some(token);
                self.error = None;
                self.confirm = false;
            }
            Err(e) => self.error = Some(e.into()),
        }
    }
    fn painted_error(&self, error: &str) -> String {
        if self.error_presentation.is_some() {
            presentation::error(error, self.error_presentation.as_ref())
        } else {
            crate::git::display_error(&crate::forge::inbox::display_error(error))
        }
    }
    fn receive(&mut self) {
        while let Ok(event) = self.rx.try_recv() {
            let (import, result) = incoming(event, &mut self.error_presentation);
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
        egui::ScrollArea::vertical()
            .max_height(height)
            .show(ui, |ui| {
                let t = Tokens::get(ui.ctx());
                ui.heading(crate::i18n::text("repo-core-1243"));
                ui.label(crate::i18n::message(
                    "repo-core-1244",
                    &[(
                        "label",
                        crate::repo::messages::display_operation(&self.label).into(),
                    )],
                ));
                ui.monospace(format!("SHA-256 {}", self.preview.digest));
                ui.label(crate::i18n::text("repo-core-1246"));
                if let Some(error) = &self.preview.verification_error {
                    ui.colored_label(
                        t.danger,
                        crate::i18n::message(
                            "repo-core-1247",
                            &[("error", error.to_string().into())],
                        ),
                    );
                }
                ui.label(crate::i18n::text("repo-core-1248"));
                ui.label(crate::i18n::text("repo-core-1249"));
                let supported =
                    crate::remote::is_remote(&self.repo) || cfg!(any(target_os = "linux", windows));
                if !supported {
                    ui.colored_label(t.danger, crate::i18n::text("repo-core-1251"));
                }
                ui.label(crate::i18n::text("repo-core-1252"));
                ui.collapsing(
                    crate::i18n::message(
                        "repo-core-1253",
                        &[("count", (self.preview.prerequisites.len()).into())],
                    ),
                    |ui| {
                        if self.preview.prerequisites.is_empty() {
                            ui.label(crate::i18n::text("repo-core-1254"));
                        }
                        for r in &self.preview.prerequisites {
                            ui.monospace(format!("{} {}", r.oid, r.name));
                        }
                    },
                );
                let editable = self.active.is_none() && self.imported.is_empty();
                let before = (self.prefix.clone(), self.selected.clone());
                ui.add_enabled_ui(editable, |ui| {
                    ui.label(crate::i18n::text("repo-core-1255"));
                    ui.text_edit_singleline(&mut self.prefix);
                    ui.label(crate::i18n::text("repo-core-1256"));
                    egui::ScrollArea::vertical()
                        .max_height(190.0)
                        .id_salt("bundle_ref_selection")
                        .show(ui, |ui| {
                            for r in &self.preview.refs {
                                let mut selected = self.selected.contains(&r.name);
                                if ui
                                    .checkbox(&mut selected, format!("{} · {}", r.name, r.oid))
                                    .changed()
                                {
                                    if selected {
                                        self.selected.insert(r.name.clone());
                                    } else {
                                        self.selected.remove(&r.name);
                                    }
                                }
                            }
                        });
                });
                if before != (self.prefix.clone(), self.selected.clone()) {
                    self.confirm = false;
                }
                let mapping =
                    crate::git::import_mapping(&self.preview.refs, &self.selected, &self.prefix);
                egui::CollapsingHeader::new(crate::i18n::text("repo-core-1258"))
                    .id_salt("Exact new refs (existing names refuse the transaction)")
                    .default_open(true)
                    .show(ui, |ui| {
                        egui::ScrollArea::vertical()
                            .max_height(160.0)
                            .id_salt("bundle_exact_refs")
                            .show(ui, |ui| match &mapping {
                                Ok(refs) => {
                                    for r in refs {
                                        ui.monospace(format!("{} {}", r.oid, r.name));
                                    }
                                }
                                Err(e) => {
                                    ui.colored_label(
                                        t.danger,
                                        crate::git::display_error(
                                            &crate::forge::inbox::display_error(e),
                                        ),
                                    );
                                }
                            });
                    });
                if self.verified {
                    ui.label(crate::i18n::text("repo-core-1260"));
                }
                if !self.imported.is_empty() {
                    ui.label(crate::i18n::text("repo-core-1261"));
                    for r in &self.imported {
                        ui.horizontal(|ui| {
                            ui.monospace(&r.name);
                            if ui
                                .small_button(crate::i18n::text("repo-core-1262"))
                                .clicked()
                            {
                                ui.ctx().copy_text(r.name.clone());
                            }
                        });
                    }
                }
                if let Some(error) = &self.error {
                    ui.colored_label(t.danger, self.painted_error(error));
                }
                if self.active.is_some() {
                    ui.add(crate::spinner::Spinner::new());
                    if ui.button(crate::i18n::text("repo-core-1263")).clicked()
                        && let Some(token) = &self.active
                    {
                        token.cancel();
                    }
                } else if self.imported.is_empty() {
                    ui.add_enabled_ui(supported && mapping.is_ok(), |ui| {
                        ui.checkbox(&mut self.consent, crate::i18n::text("repo-core-1264"));
                        if ui
                            .add_enabled(
                                self.consent,
                                egui::Button::new(crate::i18n::text("repo-core-1265")),
                            )
                            .clicked()
                        {
                            self.start(false, ui.ctx());
                        }
                        ui.add_enabled_ui(self.verified, |ui| {
                            ui.checkbox(
                                &mut self.confirm,
                                crate::i18n::message(
                                    "repo-core-1266",
                                    &[
                                        ("count", (self.selected.len()).into()),
                                        ("prefix", self.prefix.to_string().into()),
                                    ],
                                ),
                            );
                            if ui
                                .add_enabled(
                                    self.confirm,
                                    egui::Button::new(crate::i18n::text("repo-core-1267")),
                                )
                                .clicked()
                            {
                                self.start(true, ui.ctx());
                            }
                        });
                    });
                }
                if ui
                    .button(if self.imported.is_empty() {
                        crate::i18n::text("repo-core-0595")
                    } else {
                        crate::i18n::text("repo-core-1268")
                    })
                    .clicked()
                {
                    outcome = if self.imported.is_empty() {
                        Outcome::Cancel
                    } else {
                        Outcome::Done(Box::new(Action::Reload))
                    };
                }
            });
        outcome
    }
}

#[cfg(test)]
#[path = "bundles/tests.rs"]
mod tests;

#[cfg(test)]
#[path = "bundles/recording_import_ui_tests.rs"]
mod recording_import_ui_tests;

// The actual event-to-form receipt seam; raw errors stay in Form.error.
fn incoming(
    event: Event,
    receipt: &mut Option<Presentation>,
) -> (bool, Result<Vec<BundleRef>, String>) {
    let (import, result, mut presentation) = match event {
        Event::Done(import, result) => (import, result, None),
        Event::DoneWithPresentation(import, result, presentation) => (import, result, presentation),
    };
    presentation::retain_error(&result, &mut presentation);
    *receipt = presentation;
    (import, result)
}

#[cfg(test)]
#[path = "bundles/execution_bridge_tests.rs"]
mod execution_bridge_tests;
