//! Help → Copy diagnostics: collected in the background, then put on the clipboard.

use std::sync::mpsc::{Receiver, channel};

use super::panels::Toast;
use super::{App, Tab};
use crate::diagnostics::{self, Facts};

/// The graphics adapter and backend eframe draws with, as diagnostics say it.
pub(super) fn adapter(cc: &eframe::CreationContext<'_>) -> Option<String> {
    let info = cc.wgpu_render_state.as_ref()?.adapter.get_info();
    let driver = [info.driver.as_str(), info.driver_info.as_str()]
        .into_iter()
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    Some(if driver.is_empty() {
        format!("{} ({:?}, {:?})", info.name, info.backend, info.device_type)
    } else {
        format!(
            "{} ({:?}, {:?}, driver {driver})",
            info.name, info.backend, info.device_type
        )
    })
}

/// Diagnostics being collected.
pub(super) type Collecting = Receiver<String>;

impl App {
    /// Starts collecting diagnostics in the background (asking git and the forges' tools takes a
    /// moment); they go to the clipboard once collected.
    pub(super) fn copy_diagnostics(&mut self, ctx: &egui::Context) {
        if self.diagnostics.is_some() {
            self.toasts.push(Toast::new(
                "Diagnostics are still being collected".to_owned(),
                false,
            ));
            return;
        }
        let repo = match self.tabs.get(self.active) {
            Some(Tab::Repo(repo)) => Some(repo.path.clone()),
            _ => None,
        };
        let facts = Facts {
            channel: self.settings.update_channel.label().to_owned(),
            adapter: self.adapter.clone(),
            repo,
        };
        let (tx, rx) = channel();
        let ctx = ctx.clone();
        let result = crate::tasks::pool().and_then(|pool| {
            pool.submit(
                crate::tasks::Cancellation::default(),
                crate::tasks::Priority::Interactive,
                None,
                Box::new(move || {
                    let _ = tx.send(diagnostics::collect(&facts));
                    ctx.request_repaint();
                }),
            )
            .map_err(str::to_owned)
        });
        if let Err(error) = result {
            self.report(error);
            return;
        }
        self.diagnostics = Some(rx);
        self.toasts.push(Toast::new(
            "Collecting diagnostics (asking git, gh, glab, tea)…".to_owned(),
            false,
        ));
    }

    /// Collection remains local. Opening an external form requires a reviewed action.
    pub(super) fn poll_diagnostics(&mut self, ctx: &egui::Context) {
        if let Some(rx) = &self.diagnostics {
            match rx.try_recv() {
                Ok(text) => {
                    stage_report(ctx, &text);
                    self.diagnostics = None;
                    self.toasts.push(Toast::new("Diagnostics copied locally. Review and redact the report before opening an issue.".into(), false));
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {}
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    self.diagnostics = None;
                    self.toasts
                        .push(Toast::new("Collecting diagnostics failed".into(), true));
                }
            }
        }
        review_ui(ctx);
    }
}

#[derive(Clone)]
struct Review {
    text: String,
}
fn review_id() -> egui::Id {
    egui::Id::new("local-diagnostic-review")
}
fn stage_report(ctx: &egui::Context, text: &str) {
    let text = diagnostics::sanitize_report(text);
    ctx.copy_text(text.clone());
    ctx.data_mut(|data| data.insert_temp(review_id(), Review { text }));
}
fn review_ui(ctx: &egui::Context) {
    let Some(mut review) = ctx.data_mut(|data| data.remove_temp::<Review>(review_id())) else {
        return;
    };
    let mut open = true;
    let mut close = false;
    egui::Window::new("Review diagnostics locally").id(review_id()).open(&mut open)
        .default_size([720.0, 540.0]).show(ctx, |ui| {
            ui.label("Nothing has been sent. Review and remove private content, including panic messages. Opening the issue sends this edited report to GitHub in the URL; you can also keep only the local clipboard copy.");
            egui::ScrollArea::vertical().max_height(380.0).show(ui, |ui| {
                ui.add(egui::TextEdit::multiline(&mut review.text).desired_width(f32::INFINITY).desired_rows(18));
            });
            ui.horizontal(|ui| {
                if ui.button("Copy reviewed diagnostics").clicked() { ctx.copy_text(diagnostics::sanitize_report(&review.text)); }
                if ui.button("Open issue with reviewed diagnostics").clicked() {
                    let report = diagnostics::sanitize_report(&review.text);
                    ctx.copy_text(report.clone());
                    ctx.open_url(egui::OpenUrl::new_tab(diagnostics::issue_url(&report)));
                    close = true;
                }
                if ui.button("Keep local and close").clicked() { close = true; }
            });
        });
    if open && !close {
        ctx.data_mut(|data| data.insert_temp(review_id(), review));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn collecting_diagnostics_cannot_open_a_browser_before_local_review() {
        let ctx = egui::Context::default();
        let output = ctx.run_ui(Default::default(), |_| {
            stage_report(
                &ctx,
                "core.editor = env API_TOKEN=FIXTURE_SECRET nvim\nUNLABELED_DUMMY_PANIC",
            );
            review_ui(&ctx);
        });
        assert!(
            !output
                .platform_output
                .commands
                .iter()
                .any(|command| matches!(command, egui::OutputCommand::OpenUrl(_)))
        );
        let text = ctx
            .data(|data| data.get_temp::<Review>(review_id()))
            .map(|r| r.text)
            .unwrap_or_default();
        assert!(!text.contains("FIXTURE_SECRET"));
        assert!(text.contains("UNLABELED_DUMMY_PANIC"));
    }
}
