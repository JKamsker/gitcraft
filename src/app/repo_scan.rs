//! _Scan a folder for repositories_: the folders under one picked, walked in the background a
//! few levels deep, and the repositories found there listed to open or add to the recent list.

mod view;
use view::*;

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, channel};
use std::time::{Duration, Instant};

use egui::{RichText, Stroke, vec2};

use super::panels::Toast;
use super::{App, MAX_RECENT};
use crate::theme::{self, Tokens};

/// Folders never looked in: what tools fill with thousands of folders of their own. One of
/// them that is itself a repository is still found (a project called `build`).
const SKIPPED: [&str; 10] = [
    "node_modules",
    "target",
    "vendor",
    "bower_components",
    "__pycache__",
    "venv",
    "dist",
    "build",
    "obj",
    "Pods",
];

/// How far a scan goes.
#[derive(Clone, Copy, Debug)]
pub(super) struct ScanLimits {
    /// How many folders down from the one picked (its children are 1).
    pub depth: usize,
    /// The most repositories listed.
    pub max_repos: usize,
    /// The most folders looked in (and kept in memory to look in).
    pub max_dirs: usize,
}

/// The limits the start page scans with.
pub(super) const LIMITS: ScanLimits = ScanLimits {
    depth: 4,
    max_repos: 200,
    max_dirs: 50_000,
};

/// Why a scan ended before it looked everywhere.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Stopped {
    /// It found [`ScanLimits::max_repos`].
    Repos,
    /// It looked in [`ScanLimits::max_dirs`] folders.
    Folders,
    /// It was asked to stop.
    Asked,
}

/// What a scan found.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct Found {
    /// The repositories' folders, sorted.
    pub repos: Vec<PathBuf>,
    /// It stopped before it looked everywhere: there may be more.
    pub stopped: Option<Stopped>,
}

/// How a scan is going, now and then.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct Progress {
    pub folders: usize,
    pub repos: usize,
}

/// Whether the folder `name` is looked in.
fn walked(name: &str) -> bool {
    !name.starts_with('.') && !SKIPPED.contains(&name)
}

/// Whether `dir` is a repository's top folder (it has a `.git` folder, or the `.git` file of a
/// worktree or submodule).
fn is_repo(dir: &Path) -> bool {
    dir.join(".git").exists()
}

/// Why `dir` couldn't be looked in, in words.
fn unreadable(dir: &Path, e: &std::io::Error) -> String {
    let dir = crate::remote::display(dir);
    match e.kind() {
        std::io::ErrorKind::PermissionDenied => {
            format!("gitcraft isn't allowed to look in {dir}.")
        }
        std::io::ErrorKind::NotFound => format!("{dir} isn't there any more."),
        _ => format!("Could not look in {dir}: {e}"),
    }
}

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

/// The repositories in `root` and the folders under it, as far as `limits` go. Hidden folders,
/// the [`SKIPPED`] ones and links (which could go round in circles) aren't looked in, though one
/// of the first two that is a repository is found. Repositories inside repositories are found
/// too. `progress` is told how it goes, now and then; `cancel` stops it, with what it found.
pub(super) fn scan(
    root: &Path,
    limits: ScanLimits,
    cancel: &AtomicBool,
    progress: &mut dyn FnMut(Progress),
) -> Result<Found, String> {
    std::fs::read_dir(root).map_err(|e| unreadable(root, &e))?;
    let mut found = Found::default();
    // Each folder, and whether to look in it (not when it is only checked for being one).
    let mut level = vec![(root.to_owned(), true)];
    let mut seen = 0usize;
    let mut told = Instant::now();
    'walk: for depth in 0..=limits.depth {
        let mut next = Vec::new();
        for (dir, look_in) in level {
            if cancel.load(Ordering::Relaxed) {
                found.stopped = Some(Stopped::Asked);
                break 'walk;
            }
            seen += 1;
            if told.elapsed() >= Duration::from_millis(100) {
                told = Instant::now();
                progress(Progress {
                    folders: seen,
                    repos: found.repos.len(),
                });
            }
            if is_repo(&dir) {
                found.repos.push(dir.clone());
                if found.repos.len() >= limits.max_repos {
                    found.stopped = Some(Stopped::Repos);
                    break 'walk;
                }
            }
            if seen >= limits.max_dirs {
                found.stopped = Some(Stopped::Folders);
                break 'walk;
            }
            if depth == limits.depth || !look_in {
                continue;
            }
            let Ok(entries) = std::fs::read_dir(&dir) else {
                continue;
            };
            // In name order, whatever order the file system lists them in: which ones are kept
            // when the limit cuts in is the same everywhere.
            let mut children: Vec<_> = entries
                .flatten()
                // A link's own type: never followed.
                .filter(|entry| entry.file_type().is_ok_and(|t| t.is_dir()))
                .filter(|entry| entry.file_name() != ".git")
                .map(|entry| {
                    let walk = walked(&entry.file_name().to_string_lossy());
                    (entry.path(), walk)
                })
                .collect();
            children.sort();
            // Never more kept than will be looked in.
            let room = limits.max_dirs.saturating_sub(seen + next.len());
            if children.len() > room {
                found.stopped = Some(Stopped::Folders);
                children.truncate(room);
            }
            next.extend(children);
        }
        level = next;
    }
    progress(Progress {
        folders: seen,
        repos: found.repos.len(),
    });
    found.repos.sort();
    Ok(found)
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
    Done(Result<Found, String>),
}

/// A scan under way or done, and its dialog.
pub(super) struct RepoScan {
    root: PathBuf,
    rx: Receiver<ScanMsg>,
    progress: Progress,
    result: Option<Result<Found, String>>,
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
            let result = scan(&dir, LIMITS, &stop, &mut progress).map(|mut found| {
                found.repos = found.repos.into_iter().map(top_folder).collect();
                found
            });
            let _ = tx.send(ScanMsg::Done(result));
            ctx.request_repaint();
        });
        Self {
            root,
            rx,
            progress: Progress::default(),
            result: None,
            cancel,
        }
    }

    fn poll(&mut self) {
        while let Ok(msg) = self.rx.try_recv() {
            match msg {
                ScanMsg::Progress(p) => self.progress = p,
                ScanMsg::Done(result) => self.result = Some(result),
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
fn top_folder(dir: PathBuf) -> PathBuf {
    if crate::remote::is_remote(&dir) {
        return dir;
    }
    super::strip_verbatim(std::fs::canonicalize(&dir).unwrap_or(dir))
}

/// What the dialog was asked to do.
enum ScanClick {
    Open(PathBuf),
    AddAll,
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
#[path = "repo_scan/localization_tests.rs"]
mod localization_tests;
#[cfg(test)]
#[path = "repo_scan/tests.rs"]
mod tests;
