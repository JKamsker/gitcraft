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
                        eprintln!("Previous MERGED preserved at {}", label(&path));
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
                "Merge files"
            } else {
                "Compare files"
            });
            let can_save = idle && matches!(&self.loaded,Some(Loaded::Merge(m)) if m.can_save());
            if matches!(&self.loaded, Some(Loaded::Merge(_)))
                && ui
                    .add_enabled(can_save, egui::Button::new("Save and finish"))
                    .clicked()
            {
                save = true;
            }
            if matches!(&self.loaded, Some(Loaded::Diff(_))) && ui.button("Done").clicked() {
                finish = Some(0);
            }
            if ui
                .add_enabled(idle, egui::Button::new("Cancel file tool"))
                .clicked()
            {
                finish = Some(1);
            }
        });
        ui.label("Read-only input snapshots · 2 MiB per file · no repository actions");
        if self.loading.is_some() {
            ui.add(crate::spinner::Spinner::new());
            ui.label("Reading file snapshots…");
        }
        if self.saving.is_some() {
            ui.add(crate::spinner::Spinner::new());
            ui.label("Finishing the guarded save. Closing waits for its result.");
        }
        if let Some(error) = &self.error {
            ui.colored_label(crate::theme::Tokens::get(&ctx).danger, error);
            if ui
                .add_enabled(idle, egui::Button::new("Close with error"))
                .clicked()
            {
                finish = Some(2);
            }
        }
        match &mut self.loaded {
            Some(Loaded::Diff(diff)) => diff.ui(ui),
            Some(Loaded::Merge(merge)) => {
                ui.label(format!("MERGED: {}", label(&merge.output.path)));
                ui.label("Existing MERGED starts unchanged. Saving retains the previous file in a sibling recovery folder.");
                if let Some(notice) = &merge.notice {
                    ui.colored_label(crate::theme::Tokens::get(&ctx).warning, notice);
                }
                ui.horizontal_wrapped(|ui| {
                    for (side, label) in [
                        (0, "Use Base file"),
                        (1, "Use Local file"),
                        (2, "Use Remote file"),
                        (3, "Keep current MERGED"),
                    ] {
                        if ui.add_enabled(idle, egui::Button::new(label)).clicked() {
                            merge.choose(side);
                        }
                    }
                });
                if merge.computed.is_some() {
                    egui::CollapsingHeader::new("Replace working result with computed merge").show(ui,|ui|{
                        ui.label("Replaces unsaved edits in this window. MERGED on disk stays unchanged until Save and finish.");
                        if ui.add_enabled(idle,egui::Button::new("Use computed merge")).clicked()
                            && let Err(e)=merge.use_computed() {self.error=Some(e);}
                    });
                }
                egui::CollapsingHeader::new("Show Base / Local / Remote inputs")
                    .default_open(merge.editor.is_none())
                    .show(ui, |ui| {
                        ui.columns(3, |cols| {
                            for (col, (name, input)) in cols
                                .iter_mut()
                                .zip(["Base", "Local", "Remote"].into_iter().zip(&merge.inputs))
                            {
                                col.strong(name);
                                col.label(label(&input.path));
                                preview(col, input);
                            }
                        });
                    });
                if let Some(editor) = &mut merge.editor {
                    editor.standalone_ui(ui, idle);
                } else {
                    ui.label("Choose a whole file to produce an exact byte result.");
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
        Err(TryRecvError::Disconnected) => Some(Err("The file-tool worker stopped".into())),
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
            value.push_str("\n… input preview truncated");
        }
        value
    } else {
        format!(
            "Binary/non-UTF8 · {} bytes\n{}",
            input.bytes.len(),
            input
                .bytes
                .iter()
                .take(512)
                .map(|b| format!("{b:02x} "))
                .collect::<String>()
        )
    };
    egui::ScrollArea::both()
        .id_salt((&input.path, "source"))
        .max_height(160.)
        .show(ui, |ui| ui.monospace(content));
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui_kittest::{Harness, kittest::Queryable};
    use std::path::{Path, PathBuf};
    fn fixture() -> Result<(crate::git::file_comparison::Scratch, [PathBuf; 4]), String> {
        let temp = crate::git::file_comparison::Scratch::new()?;
        let paths = ["base", "local", "remote", "merged"].map(|n| temp.0.join(n));
        for (path, bytes) in paths.iter().zip([
            b"base\n".as_slice(),
            b"local\n",
            b"remote\n",
            b"existing result\n",
        ]) {
            std::fs::write(path, bytes).map_err(|e| e.to_string())?;
        }
        Ok((temp, paths))
    }
    fn harness(invocation: Invocation) -> (Harness<'static, ToolApp>, Arc<AtomicU8>) {
        let code = Arc::new(AtomicU8::new(1));
        let result = Arc::clone(&code);
        let h = Harness::builder()
            .with_size([1400., 950.])
            .build_eframe(move |cc| ToolApp::new(invocation, result, &cc.egui_ctx));
        (h, code)
    }
    fn settle(h: &mut Harness<'_, ToolApp>) -> Result<(), String> {
        let until = std::time::Instant::now() + std::time::Duration::from_secs(10);
        loop {
            h.run_steps(1);
            if h.state().loading.is_none() && h.state().saving.is_none() {
                return Ok(());
            }
            if std::time::Instant::now() > until {
                return Err("Standalone UI did not settle".into());
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    }
    fn close_window(h: &mut Harness<'_, ToolApp>) {
        h.input_mut()
            .viewports
            .entry(egui::ViewportId::ROOT)
            .or_default()
            .events
            .push(egui::ViewportEvent::Close);
        h.run_steps(1);
    }
    #[test]
    fn actual_diff_close_succeeds_but_unsaved_merge_close_cancels_and_load_error_is_distinct()
    -> Result<(), String> {
        let (_temp, paths) = fixture()?;
        let before = std::fs::read(&paths[3]).map_err(|e| e.to_string())?;
        let (mut diff, diff_code) = harness(Invocation::Diff([paths[0].clone(), paths[1].clone()]));
        settle(&mut diff)?;
        assert!(diff.query_by_label_contains("-base").is_some());
        assert!(diff.query_by_label_contains("+local").is_some());
        close_window(&mut diff);
        assert_eq!(diff_code.load(Ordering::Acquire), 0);
        let (mut merge, merge_code) = harness(Invocation::Merge(paths.clone()));
        settle(&mut merge)?;
        merge.get_by_label("Use Local file").click();
        merge.run_steps(1);
        close_window(&mut merge);
        assert_eq!(merge_code.load(Ordering::Acquire), 1);
        assert_eq!(std::fs::read(&paths[3]).map_err(|e| e.to_string())?, before);
        let missing = paths[0].with_file_name("does-not-exist");
        let (mut failed, failed_code) = harness(Invocation::Diff([missing, paths[1].clone()]));
        settle(&mut failed)?;
        assert!(failed.state().error.is_some());
        close_window(&mut failed);
        assert_eq!(failed_code.load(Ordering::Acquire), 2);
        Ok(())
    }
    #[test]
    fn actual_result_editor_and_conflict_choice_save_exact_bytes_with_success() -> Result<(), String>
    {
        let (_temp, paths) = fixture()?;
        let (mut h, code) = harness(Invocation::Merge(paths.clone()));
        settle(&mut h)?;
        h.get_by_role(egui::accesskit::Role::MultilineTextInput)
            .focus();
        h.run_steps(1);
        h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::A);
        h.run_steps(1);
        h.get_by_role(egui::accesskit::Role::MultilineTextInput)
            .type_text("manual result\nlast");
        h.run_steps(1);
        h.get_by_label("Save and finish").click();
        settle(&mut h)?;
        assert_eq!(code.load(Ordering::Acquire), 0);
        assert_eq!(
            std::fs::read(&paths[3]).map_err(|e| e.to_string())?,
            b"manual result\nlast"
        );
        assert_eq!(
            std::fs::read(&paths[1]).map_err(|e| e.to_string())?,
            b"local\n"
        );
        drop(h);
        std::fs::write(
            &paths[3],
            b"<<<<<<< LOCAL\nlocal\n||||||| BASE\nbase\n=======\nremote\n>>>>>>> REMOTE\n",
        )
        .map_err(|e| e.to_string())?;
        let (mut h, code) = harness(Invocation::Merge(paths.clone()));
        settle(&mut h)?;
        h.get_by_role_and_label(egui::accesskit::Role::Button, "Local")
            .click();
        h.run_steps(1);
        h.get_by_label("Save and finish").click();
        settle(&mut h)?;
        assert_eq!(code.load(Ordering::Acquire), 0);
        assert_eq!(
            std::fs::read(&paths[3]).map_err(|e| e.to_string())?,
            b"local\n"
        );
        Ok(())
    }
    #[test]
    fn actual_save_refuses_changed_destination_and_reports_nonzero_without_overwrite()
    -> Result<(), String> {
        let (_temp, paths) = fixture()?;
        let (mut h, code) = harness(Invocation::Merge(paths.clone()));
        settle(&mut h)?;
        h.get_by_label("Use Local file").click();
        h.run_steps(1);
        std::fs::write(&paths[3], b"concurrent owner bytes").map_err(|e| e.to_string())?;
        h.get_by_label("Save and finish").click();
        settle(&mut h)?;
        assert_eq!(code.load(Ordering::Acquire), 2);
        assert!(!h.state().finished);
        assert_eq!(
            std::fs::read(&paths[3]).map_err(|e| e.to_string())?,
            b"concurrent owner bytes"
        );
        h.get_by_label("Close with error").click();
        h.run_steps(1);
        assert_eq!(code.load(Ordering::Acquire), 2);
        Ok(())
    }
    #[test]
    fn hostile_profile_cannot_redirect_the_standalone_background_ui() -> Result<(), String> {
        const CHILD: &str = "GITCRAFT_STANDALONE_PROFILE_FIXTURE";
        const NAME: &str =
            "standalone::ui::tests::hostile_profile_cannot_redirect_the_standalone_background_ui";
        if std::env::var_os(CHILD).is_some() {
            crate::git::set_profile_config(Some("nonexistent-profile-config".into()));
            let (_temp, paths) = fixture()?;
            let (mut h, code) = harness(Invocation::Merge(paths.clone()));
            settle(&mut h)?;
            assert!(h.state().error.is_none());
            h.get_by_label("Use Remote file").click();
            h.run_steps(1);
            h.get_by_label("Save and finish").click();
            settle(&mut h)?;
            assert_eq!(code.load(Ordering::Acquire), 0);
            assert_eq!(
                std::fs::read(&paths[3]).map_err(|e| e.to_string())?,
                b"remote\n"
            );
            return Ok(());
        }
        let out = std::process::Command::new(std::env::current_exe().map_err(|e| e.to_string())?)
            .args(["--exact", NAME, "--nocapture"])
            .env(CHILD, "1")
            .env("GIT_DIR", Path::new("nonexistent-caller-repository"))
            .output()
            .map_err(|e| e.to_string())?;
        assert!(
            out.status.success(),
            "{} {}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        Ok(())
    }
}
