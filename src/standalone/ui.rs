use super::{
    Invocation,
    files::{Snapshot, label},
    model::{self, Loaded},
    save::Saved,
};
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU8, Ordering},
    mpsc::{self, Receiver, TryRecvError},
};

pub(super) struct ToolApp {
    active: Arc<AtomicBool>,
    outcome: Arc<AtomicU8>,
    loading: Option<Receiver<Result<Loaded, String>>>,
    loaded: Option<Loaded>,
    saving: Option<Receiver<Result<Saved, String>>>,
    error: Option<String>,
    finished: bool,
}
impl ToolApp {
    pub fn new(invocation: Invocation, outcome: Arc<AtomicU8>, ctx: &egui::Context) -> Self {
        crate::theme::install_fonts(ctx);
        crate::theme::apply(
            ctx,
            &crate::theme::ThemeId::default(),
            &crate::theme::Tweaks::default(),
        );
        // Do not construct App, load Settings/profiles, or inherit a connection context.
        let active = Arc::new(AtomicBool::new(true));
        let weak = Arc::downgrade(&active);
        let (tx, rx) = mpsc::channel();
        let repaint = ctx.clone();
        let _ = super::submit(move || {
            let current = || weak.upgrade().is_some_and(|a| a.load(Ordering::Acquire));
            let result = model::load(&invocation, &current);
            let _ = tx.send(result);
            repaint.request_repaint();
        });
        Self {
            active,
            outcome,
            loading: Some(rx),
            loaded: None,
            saving: None,
            error: None,
            finished: false,
        }
    }
    fn poll(&mut self, ctx: &egui::Context) {
        if self.finished {
            return;
        }
        if let Some(result) = self.loading.as_ref().and_then(receive) {
            self.loading = None;
            match result {
                Ok(loaded) => {
                    if matches!(&loaded, Loaded::Diff(_)) {
                        self.outcome.store(0, Ordering::Release);
                    }
                    self.loaded = Some(loaded);
                }
                Err(e) => {
                    self.error = Some(e);
                    self.outcome.store(2, Ordering::Release);
                }
            }
        }
        if let Some(result) = self.saving.as_ref().and_then(receive) {
            self.saving = None;
            match result {
                Ok(saved) => {
                    if let Some(path) = saved.previous {
                        eprintln!(
                            "{}",
                            crate::i18n::message(
                                "interaction-standalone-previous-location",
                                &[("path", (label(&path)).into())]
                            )
                        );
                    }
                    self.finish(0, ctx);
                }
                Err(e) => {
                    self.error = Some(e);
                    self.outcome.store(2, Ordering::Release);
                }
            }
        }
    }
    fn finish(&mut self, code: u8, ctx: &egui::Context) {
        if self.saving.is_some() {
            return;
        }
        self.finished = true;
        self.outcome.store(code, Ordering::Release);
        self.active.store(false, Ordering::Release);
        self.loading = None;
        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
    }
    fn save(&mut self, ctx: &egui::Context) {
        if self.saving.is_some() {
            return;
        }
        let Some(Loaded::Merge(merge)) = &self.loaded else {
            return;
        };
        let bytes = match merge.candidate() {
            Ok(bytes) => bytes,
            Err(e) => {
                self.error = Some(e);
                return;
            }
        };
        let output = merge.output.clone();
        let inputs = merge.inputs.clone();
        let repaint = ctx.clone();
        let (tx, rx) = mpsc::channel();
        self.saving = Some(rx);
        self.error = None;
        // A deliberate save owns its transaction until completion, even if close is requested.
        let _ = super::submit(move || {
            let _ = tx.send(output.save(&bytes, &inputs));
            repaint.request_repaint();
        });
    }
    pub fn paint(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        self.poll(&ctx);
        if !self.finished && ctx.input(|i| i.viewport().close_requested()) {
            if self.saving.is_some() {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            } else {
                self.finish(self.outcome.load(Ordering::Acquire), &ctx);
            }
        }
        if crate::app::MAXIMIZE_WHEN_SHOWN.load(Ordering::Relaxed) && ctx.cumulative_frame_nr() >= 1
        {
            crate::app::MAXIMIZE_WHEN_SHOWN.store(false, Ordering::Relaxed);
            ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(true));
        }
        let idle = self.saving.is_none();
        if idle && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) {
            self.finish(1, &ctx);
        }
        let mut save = false;
        let mut finish = None;
        ui.horizontal_wrapped(|ui| {
            ui.heading(if matches!(self.loaded, Some(Loaded::Merge(_))) {
                crate::i18n::text("interaction-standalone-merge-title")
            } else {
                crate::i18n::text("interaction-standalone-compare-title")
            });
            let can_save = idle && matches!(&self.loaded,Some(Loaded::Merge(m)) if m.can_save());
            if matches!(&self.loaded, Some(Loaded::Merge(_)))
                && ui
                    .add_enabled(
                        can_save,
                        egui::Button::new(crate::i18n::text("interaction-standalone-save-finish")),
                    )
                    .clicked()
            {
                save = true;
            }
            if matches!(&self.loaded, Some(Loaded::Diff(_)))
                && ui
                    .button(crate::i18n::text("interaction-standalone-done"))
                    .clicked()
            {
                finish = Some(0);
            }
            if ui
                .add_enabled(
                    idle,
                    egui::Button::new(crate::i18n::text("interaction-standalone-cancel-tool")),
                )
                .clicked()
            {
                finish = Some(1);
            }
        });
        ui.label(crate::i18n::text("interaction-standalone-snapshot-summary"));
        if self.loading.is_some() {
            ui.add(crate::spinner::Spinner::new());
            ui.label(crate::i18n::text("interaction-standalone-reading"));
        }
        if self.saving.is_some() {
            ui.add(crate::spinner::Spinner::new());
            ui.label(crate::i18n::text("interaction-standalone-saving"));
        }
        if let Some(error) = &self.error {
            ui.colored_label(crate::theme::Tokens::get(&ctx).danger, error);
            if ui
                .add_enabled(
                    idle,
                    egui::Button::new(crate::i18n::text("interaction-standalone-close-error")),
                )
                .clicked()
            {
                finish = Some(2);
            }
        }
        match &mut self.loaded {
            Some(Loaded::Diff(diff)) => diff.ui(ui),
            Some(Loaded::Merge(merge)) => {
                ui.label(crate::i18n::message(
                    "interaction-standalone-merged-path",
                    &[("path", (label(&merge.output.path)).into())],
                ));
                ui.label(crate::i18n::text(
                    "interaction-standalone-merged-explanation",
                ));
                if let Some(notice) = &merge.notice {
                    ui.colored_label(crate::theme::Tokens::get(&ctx).warning, notice);
                }
                ui.horizontal_wrapped(|ui| {
                    for (side, label) in [
                        (0, crate::i18n::text("interaction-standalone-use-base")),
                        (1, crate::i18n::text("interaction-standalone-use-local")),
                        (2, crate::i18n::text("interaction-standalone-use-remote")),
                        (3, crate::i18n::text("interaction-standalone-keep-merged")),
                    ] {
                        if ui.add_enabled(idle, egui::Button::new(label)).clicked() {
                            merge.choose(side);
                        }
                    }
                });
                if merge.computed.is_some() {
                    egui::CollapsingHeader::new(crate::i18n::text(
                        "interaction-standalone-computed-heading",
                    ))
                    .id_salt("Replace working result with computed merge")
                    .show(ui, |ui| {
                        ui.label(crate::i18n::text(
                            "interaction-standalone-computed-explanation",
                        ));
                        if ui
                            .add_enabled(
                                idle,
                                egui::Button::new(crate::i18n::text(
                                    "interaction-standalone-use-computed",
                                )),
                            )
                            .clicked()
                            && let Err(e) = merge.use_computed()
                        {
                            self.error = Some(e);
                        }
                    });
                }
                egui::CollapsingHeader::new(crate::i18n::text(
                    "interaction-standalone-inputs-heading",
                ))
                .id_salt("Show Base / Local / Remote inputs")
                .default_open(merge.editor.is_none())
                .show(ui, |ui| {
                    ui.columns(3, |cols| {
                        for (col, (name, input)) in cols.iter_mut().zip(
                            [
                                crate::i18n::text("interaction-standalone-base"),
                                crate::i18n::text("interaction-standalone-local"),
                                crate::i18n::text("interaction-standalone-remote"),
                            ]
                            .into_iter()
                            .zip(&merge.inputs),
                        ) {
                            col.strong(name);
                            col.label(label(&input.path));
                            preview(col, input);
                        }
                    });
                });
                if let Some(editor) = &mut merge.editor {
                    editor.standalone_ui(ui, idle);
                } else {
                    ui.label(crate::i18n::text(
                        "interaction-standalone-choose-whole-file",
                    ));
                }
            }
            None => {}
        }
        if save {
            self.save(&ctx);
        }
        if let Some(code) = finish {
            self.finish(code, &ctx);
        }
        if self.loading.is_some() || self.saving.is_some() {
            ctx.request_repaint_after(std::time::Duration::from_millis(30));
        }
    }
}
impl eframe::App for ToolApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.paint(ui);
    }
    fn persist_egui_memory(&self) -> bool {
        false
    }
}
impl Drop for ToolApp {
    fn drop(&mut self) {
        self.active.store(false, Ordering::Release);
    }
}
fn receive<T>(rx: &Receiver<Result<T, String>>) -> Option<Result<T, String>> {
    match rx.try_recv() {
        Ok(result) => Some(result),
        Err(TryRecvError::Empty) => None,
        Err(TryRecvError::Disconnected) => Some(Err(crate::i18n::text(
            "interaction-standalone-error-worker-stopped",
        )
        .into())),
    }
}
fn preview(ui: &mut egui::Ui, input: &Snapshot) {
    let text = if let Ok(s) = std::str::from_utf8(&input.bytes) {
        if s.contains('\0') { None } else { Some(s) }
    } else {
        None
    };
    let content = if let Some(text) = text {
        let mut value = text.chars().take(16_000).collect::<String>();
        if value.len() < text.len() {
            value = crate::i18n::message(
                "interaction-standalone-truncated-preview",
                &[("preview", value.into())],
            );
        }
        value
    } else {
        crate::i18n::message(
            "interaction-standalone-binary-preview",
            &[
                ("count", (input.bytes.len()).into()),
                (
                    "preview",
                    (input
                        .bytes
                        .iter()
                        .take(512)
                        .map(|b| format!("{b:02x} "))
                        .collect::<String>())
                    .into(),
                ),
            ],
        )
    };
    egui::ScrollArea::both()
        .id_salt((&input.path, "source"))
        .max_height(160.)
        .show(ui, |ui| ui.monospace(content));
}

#[cfg(test)]
#[path = "ui/tests.rs"]
mod tests;

#[cfg(test)]
#[path = "ui/localization_tests.rs"]
mod localization_tests;
