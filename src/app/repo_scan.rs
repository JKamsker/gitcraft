//! _Scan a folder for repositories_: the folders under one picked, walked in the background a
//! few levels deep, and the repositories found there listed to open or add to the recent list.

pub(super) mod engine;
mod view;
pub(super) use engine::*;
use view::*;

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, channel};

use egui::{RichText, Stroke, vec2};

use super::panels::Toast;
use super::{App, MAX_RECENT};
use crate::theme::{self, Tokens};

pub(super) fn display_error(raw: &str) -> Option<String> {
    if let Some(path) = raw
        .strip_prefix("gitcraft isn't allowed to look in ")
        .and_then(|s| s.strip_suffix("."))
    {
        return Some(crate::i18n::message(
            "interaction-scan-permission",
            &[("path", path.into())],
        ));
    }
    if let Some(path) = raw.strip_suffix(" isn't there any more.") {
        return Some(crate::i18n::message(
            "interaction-scan-gone",
            &[("path", path.into())],
        ));
    }
    raw.strip_prefix("Could not look in ").map(|detail| {
        crate::i18n::message("interaction-scan-read-error", &[("detail", detail.into())])
    })
}

/// Adds the `found` repositories missing from `recent` at its end, as many as there is room
/// for (`max` in all): never pushing out those it has. Returns how many were added, how many
/// it had already, and how many there was no room for.
pub(super) fn add_to_recent(
    recent: &mut Vec<PathBuf>,
    found: &[PathBuf],
    max: usize,
) -> (usize, usize, usize) {
    let (mut added, mut had, mut no_room) = (0, 0, 0);
    for repo in found {
        if recent.contains(repo) {
            had += 1;
        } else if recent.len() < max {
            recent.push(repo.clone());
            added += 1;
        } else {
            no_room += 1;
        }
    }
    (added, had, no_room)
}

/// What [`add_to_recent`] did, in words.
fn added_message(added: usize, had: usize, no_room: usize) -> String {
    let id = match (had > 0, no_room > 0) {
        (false, false) => "interaction-scan-added",
        (true, false) => "interaction-scan-added-existing",
        (false, true) => "interaction-scan-added-full",
        (true, true) => "interaction-scan-added-both",
    };
    crate::i18n::message(
        id,
        &[
            ("added", added.into()),
            ("had", had.into()),
            ("room", no_room.into()),
            ("max", MAX_RECENT.into()),
        ],
    )
}

/// What the scan's thread sends.
enum ScanMsg {
    Progress(Progress),
    /// How it ended, and how many folders down the deepest repository found was.
    Done(Result<Found, String>, usize),
}

/// A scan under way or done, and its dialog.
pub(super) struct RepoScan {
    root: PathBuf,
    rx: Receiver<ScanMsg>,
    progress: Progress,
    result: Option<Result<Found, String>>,
    /// How many folders down the deepest repository found is (what _Save as root_ keeps).
    deepest: usize,
    cancel: Arc<AtomicBool>,
}

impl RepoScan {
    fn start(root: PathBuf, ctx: &egui::Context) -> Self {
        let (tx, rx) = channel();
        let cancel = Arc::new(AtomicBool::new(false));
        let (dir, stop, ctx) = (root.clone(), cancel.clone(), ctx.clone());
        crate::connections::spawn(move || {
            let mut progress = |p| {
                let _ = tx.send(ScanMsg::Progress(p));
                ctx.request_repaint();
            };
            let mut deepest = 0;
            let result = scan(&dir, LIMITS, &stop, &mut progress).map(|mut found| {
                deepest = found
                    .repos
                    .iter()
                    .filter_map(|repo| repo.strip_prefix(&dir).ok())
                    .map(|rel| rel.components().count())
                    .max()
                    .unwrap_or(0);
                found.repos = found.repos.into_iter().map(top_folder).collect();
                found
            });
            let _ = tx.send(ScanMsg::Done(result, deepest));
            ctx.request_repaint();
        });
        Self {
            root,
            rx,
            progress: Progress::default(),
            result: None,
            deepest: 0,
            cancel,
        }
    }

    fn poll(&mut self) {
        while let Ok(msg) = self.rx.try_recv() {
            match msg {
                ScanMsg::Progress(p) => self.progress = p,
                ScanMsg::Done(result, deepest) => {
                    self.result = Some(result);
                    self.deepest = deepest;
                }
            }
        }
    }

    /// Stops looking; what was found so far stays listed.
    fn stop(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

impl Drop for RepoScan {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

/// A found repository's folder in the form the recent list keeps (as opening it would give).
pub(super) fn top_folder(dir: PathBuf) -> PathBuf {
    if crate::remote::is_remote(&dir) {
        return dir;
    }
    super::strip_verbatim(std::fs::canonicalize(&dir).unwrap_or(dir))
}

/// What the dialog was asked to do.
enum ScanClick {
    Open(PathBuf),
    AddAll,
    SaveAsRoot,
    Stop,
    Close,
}

impl App {
    pub(super) fn poll_repo_scan(&mut self) {
        if let Some(scan) = &mut self.repo_scan {
            scan.poll();
        }
    }
    /// Asks for a folder and starts scanning it.
    pub(super) fn start_repo_scan(&mut self, ctx: &egui::Context) {
        if let Some(dir) = rfd::FileDialog::new()
            .set_title(crate::i18n::text("interaction-scan-picker"))
            .pick_folder()
        {
            self.repo_scan = Some(RepoScan::start(dir, ctx));
        }
    }

    /// The dialog of the scan, while there is one.
    pub(super) fn repo_scan_dialog(&mut self, ctx: &egui::Context) {
        let Some(scan) = &mut self.repo_scan else {
            return;
        };
        let t = Tokens::get(ctx);
        let frame = egui::Frame::new()
            .fill(t.card)
            .stroke(Stroke::new(1.0, t.field_border))
            .corner_radius(t.radius_lg)
            .inner_margin(egui::Margin::same(18))
            .shadow(ctx.global_style().visuals.window_shadow);
        let done = scan.result.is_some();
        let mut click = None;
        let resp = egui::Modal::new(egui::Id::new("repo-scan-dialog"))
            .frame(frame)
            .backdrop_color(egui::Color32::from_black_alpha(110))
            .show(ctx, |ui| {
                ui.set_width(520.0);
                let title = if done {
                    crate::i18n::text("interaction-scan-found")
                } else {
                    crate::i18n::text("interaction-scan-scanning")
                };
                ui.label(RichText::new(title).font(theme::semibold(15.0)));
                ui.label(
                    RichText::new(crate::remote::display(&scan.root))
                        .color(t.text_faint)
                        .small(),
                );
                ui.add_space(10.0);
                click = scan_body(ui, scan, &t);
                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    let count = match &scan.result {
                        Some(Ok(found)) => found.repos.len(),
                        _ => 0,
                    };
                    let add = crate::repo::primary_button(
                        ui,
                        crate::i18n::text("interaction-scan-add"),
                        count > 0,
                        200.0,
                    )
                    .on_hover_text(crate::i18n::message(
                        "interaction-scan-capacity",
                        &[("max", MAX_RECENT.into())],
                    ));
                    if add.clicked() {
                        click = Some(ScanClick::AddAll);
                    }
                    if count > 0
                        && ui
                            .button(crate::i18n::text("interaction-library-save-root"))
                            .on_hover_text(crate::i18n::text("interaction-library-save-root-tip"))
                            .clicked()
                    {
                        click = Some(ScanClick::SaveAsRoot);
                    }
                    let (label, what) = if done {
                        (
                            crate::i18n::text("interaction-scan-close"),
                            ScanClick::Close,
                        )
                    } else {
                        (crate::i18n::text("interaction-scan-stop"), ScanClick::Stop)
                    };
                    if ui
                        .add(egui::Button::new(label).min_size(vec2(80.0, 30.0)))
                        .clicked()
                    {
                        click = Some(what);
                    }
                });
            });
        if resp.should_close() && click.is_none() {
            click = Some(if done {
                ScanClick::Close
            } else {
                ScanClick::Stop
            });
        }
        match click {
            Some(ScanClick::Open(path)) => {
                self.repo_scan = None;
                self.open_repo(&path, ctx);
            }
            Some(ScanClick::AddAll) => {
                if let Some(Some(Ok(found))) = self.repo_scan.take().map(|mut s| s.result.take()) {
                    let (added, had, no_room) =
                        add_to_recent(&mut self.recent, &found.repos, MAX_RECENT);
                    self.toasts.push(Toast::new(
                        added_message(added, had, no_room),
                        added == 0 && no_room > 0,
                    ));
                }
            }
            Some(ScanClick::SaveAsRoot) => {
                if let Some(scan) = self.repo_scan.take() {
                    self.save_scan_as_root(scan.root.clone(), scan.deepest);
                }
            }
            Some(ScanClick::Stop) => {
                if let Some(scan) = &self.repo_scan {
                    scan.stop();
                }
            }
            Some(ScanClick::Close) => self.repo_scan = None,
            None => {}
        }
    }
}

#[cfg(test)]
#[path = "repo_scan/engine_tests.rs"]
mod engine_tests;
#[cfg(test)]
#[path = "repo_scan/localization_tests.rs"]
mod localization_tests;
#[cfg(test)]
#[path = "repo_scan/tests.rs"]
mod tests;
