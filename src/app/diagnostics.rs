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
        std::thread::spawn(move || {
            let _ = tx.send(diagnostics::collect(&facts));
            ctx.request_repaint();
        });
        self.diagnostics = Some(rx);
        self.toasts.push(Toast::new(
            "Collecting diagnostics (asking git, gh, glab, tea)…".to_owned(),
            false,
        ));
    }

    /// Puts the diagnostics on the clipboard once they are collected.
    pub(super) fn poll_diagnostics(&mut self, ctx: &egui::Context) {
        let Some(rx) = &self.diagnostics else {
            return;
        };
        match rx.try_recv() {
            Ok(text) => {
                ctx.copy_text(text);
                self.diagnostics = None;
                self.toasts.push(Toast::new(
                    "Diagnostics copied: check them, then paste them into the issue you report."
                        .to_owned(),
                    false,
                ));
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {}
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                self.diagnostics = None;
                self.toasts
                    .push(Toast::new("Collecting diagnostics failed".to_owned(), true));
            }
        }
    }
}
