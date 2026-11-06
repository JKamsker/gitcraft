//! _Scan a folder for repositories_: the folders under one picked, walked in the background a
//! few levels deep, and the repositories found there listed to open or add to the recent list.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, channel};

use egui::{RichText, Stroke, vec2};

use super::panels::Toast;
use super::{App, MAX_RECENT};
use crate::theme::{self, Tokens};

/// Folders never looked in: what tools fill with thousands of folders of their own.
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
    /// The most folders looked in.
    pub max_dirs: usize,
}

/// The limits the start page scans with.
pub(super) const LIMITS: ScanLimits = ScanLimits {
    depth: 4,
    max_repos: 200,
    max_dirs: 50_000,
};

/// What a scan found.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct Found {
    /// The repositories' folders, sorted.
    pub repos: Vec<PathBuf>,
    /// It stopped at a limit: there may be more.
    pub cut: bool,
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

/// The repositories in `root` and the folders under it, as far as `limits` go, skipping hidden
/// folders, the [`SKIPPED`] ones and links (which could go round in circles). Repositories
/// inside repositories are found too. `visited` is told how many folders were looked in, now
/// and then; `cancel` stops it.
pub(super) fn scan(
    root: &Path,
    limits: ScanLimits,
    cancel: &AtomicBool,
    visited: &mut dyn FnMut(usize),
) -> Result<Found, String> {
    std::fs::read_dir(root).map_err(|e| format!("Could not look in {}: {e}", root.display()))?;
    let mut found = Found::default();
    let mut level = vec![root.to_owned()];
    let mut seen = 0usize;
    'walk: for depth in 0..=limits.depth {
        let mut next = Vec::new();
        for dir in level {
            if cancel.load(Ordering::Relaxed) {
                break 'walk;
            }
            seen += 1;
            if seen.is_multiple_of(200) {
                visited(seen);
            }
            if is_repo(&dir) {
                found.repos.push(dir.clone());
                if found.repos.len() >= limits.max_repos {
                    found.cut = true;
                    break 'walk;
                }
            }
            if seen >= limits.max_dirs {
                found.cut = true;
                break 'walk;
            }
            if depth == limits.depth {
                continue;
            }
            let Ok(entries) = std::fs::read_dir(&dir) else {
                continue;
            };
            for entry in entries.flatten() {
                // A link's own type: never followed.
                let is_dir = entry.file_type().is_ok_and(|t| t.is_dir());
                if is_dir && walked(&entry.file_name().to_string_lossy()) {
                    next.push(entry.path());
                }
            }
        }
        next.sort();
        level = next;
    }
    visited(seen);
    found.repos.sort();
    Ok(found)
}

/// What the scan's thread sends.
enum ScanMsg {
    Visited(usize),
    Done(Result<Found, String>),
}

/// A scan under way or done, and its dialog.
pub(super) struct RepoScan {
    root: PathBuf,
    rx: Receiver<ScanMsg>,
    visited: usize,
    result: Option<Result<Found, String>>,
    cancel: Arc<AtomicBool>,
}

impl RepoScan {
    fn start(root: PathBuf, ctx: &egui::Context) -> Self {
        let (tx, rx) = channel();
        let cancel = Arc::new(AtomicBool::new(false));
        let (dir, stop, ctx) = (root.clone(), cancel.clone(), ctx.clone());
        std::thread::spawn(move || {
            let mut visited = |n| {
                let _ = tx.send(ScanMsg::Visited(n));
                ctx.request_repaint();
            };
            let result = scan(&dir, LIMITS, &stop, &mut visited).map(|mut found| {
                found.repos = found.repos.into_iter().map(top_folder).collect();
                found
            });
            let _ = tx.send(ScanMsg::Done(result));
            ctx.request_repaint();
        });
        Self {
            root,
            rx,
            visited: 0,
            result: None,
            cancel,
        }
    }

    fn poll(&mut self) {
        while let Ok(msg) = self.rx.try_recv() {
            match msg {
                ScanMsg::Visited(n) => self.visited = n,
                ScanMsg::Done(result) => self.result = Some(result),
            }
        }
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
    Close,
}

impl App {
    /// Asks for a folder and starts scanning it.
    pub(super) fn start_repo_scan(&mut self, ctx: &egui::Context) {
        if let Some(dir) = rfd::FileDialog::new()
            .set_title("Scan a folder for repositories")
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
        scan.poll();
        let t = Tokens::get(ctx);
        let frame = egui::Frame::new()
            .fill(t.card)
            .stroke(Stroke::new(1.0, t.field_border))
            .corner_radius(t.radius_lg)
            .inner_margin(egui::Margin::same(18))
            .shadow(ctx.global_style().visuals.window_shadow);
        let mut click = None;
        let resp = egui::Modal::new(egui::Id::new("repo-scan-dialog"))
            .frame(frame)
            .backdrop_color(egui::Color32::from_black_alpha(110))
            .show(ctx, |ui| {
                ui.set_width(520.0);
                ui.label(RichText::new("Repositories found").font(theme::semibold(15.0)));
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
                    let label = if count > MAX_RECENT {
                        format!("Add the first {MAX_RECENT} to the recent list")
                    } else {
                        "Add all to the recent list".to_owned()
                    };
                    if crate::repo::primary_button(ui, &label, count > 0, 220.0).clicked() {
                        click = Some(ScanClick::AddAll);
                    }
                    let close = if scan.result.is_some() {
                        "Close"
                    } else {
                        "Stop"
                    };
                    if ui
                        .add(egui::Button::new(close).min_size(vec2(80.0, 30.0)))
                        .clicked()
                    {
                        click = Some(ScanClick::Close);
                    }
                });
            });
        if resp.should_close() && click.is_none() {
            click = Some(ScanClick::Close);
        }
        match click {
            Some(ScanClick::Open(path)) => {
                self.repo_scan = None;
                self.open_repo(&path, ctx);
            }
            Some(ScanClick::AddAll) => {
                if let Some(Some(Ok(found))) = self.repo_scan.take().map(|mut s| s.result.take()) {
                    let added = self.add_recent(&found.repos);
                    self.toasts.push(Toast::new(
                        format!(
                            "Added {added} {} to the recent list",
                            if added == 1 {
                                "repository"
                            } else {
                                "repositories"
                            }
                        ),
                        false,
                    ));
                }
            }
            Some(ScanClick::Close) => self.repo_scan = None,
            None => {}
        }
    }

    /// Puts `repos` at the top of the recent list, in their order, as many as it holds.
    /// Returns how many of them it has now.
    fn add_recent(&mut self, repos: &[PathBuf]) -> usize {
        let take: Vec<PathBuf> = repos.iter().take(MAX_RECENT).cloned().collect();
        self.recent.retain(|p| !take.contains(p));
        self.recent.splice(0..0, take.iter().cloned());
        self.recent.truncate(MAX_RECENT);
        take.len()
    }
}

/// The progress, the error, or the repositories found with a button each to open it.
fn scan_body(ui: &mut egui::Ui, scan: &RepoScan, t: &Tokens) -> Option<ScanClick> {
    let found = match &scan.result {
        None => {
            ui.horizontal(|ui| {
                ui.add(crate::spinner::Spinner::new());
                ui.label(format!("Looking… {} folders so far", scan.visited));
            });
            return None;
        }
        Some(Err(e)) => {
            ui.colored_label(t.danger, e);
            return None;
        }
        Some(Ok(found)) => found,
    };
    if found.repos.is_empty() {
        ui.label(format!(
            "No repositories found ({} folders down, hidden folders and ones like node_modules \
             skipped).",
            LIMITS.depth
        ));
        return None;
    }
    let mut click = None;
    egui::ScrollArea::vertical()
        .max_height(360.0)
        .auto_shrink([false, true])
        .show(ui, |ui| {
            for repo in &found.repos {
                ui.horizontal(|ui| {
                    if ui.button("Open").clicked() {
                        click = Some(ScanClick::Open(repo.clone()));
                    }
                    ui.vertical(|ui| {
                        ui.label(
                            RichText::new(super::home_filter::repo_name(repo))
                                .font(theme::medium(12.5)),
                        );
                        ui.label(
                            RichText::new(crate::remote::display(repo))
                                .color(t.text_faint)
                                .small(),
                        );
                    });
                });
                ui.add_space(2.0);
            }
        });
    if found.cut {
        ui.add_space(4.0);
        ui.label(
            RichText::new("It stopped at its limit: there may be more.")
                .color(t.text_faint)
                .small(),
        );
    }
    click
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git;

    fn tree(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("gitcraft-scan-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::create_dir_all(&dir);
        dir
    }

    fn init(dir: &Path) {
        let _ = std::fs::create_dir_all(dir);
        assert!(git::run(Some(dir), &["init", "-q"]).is_ok());
    }

    fn names(root: &Path, found: &Found) -> Vec<String> {
        found
            .repos
            .iter()
            .map(|p| {
                p.strip_prefix(root)
                    .unwrap_or(p)
                    .to_string_lossy()
                    .replace('\\', "/")
            })
            .collect()
    }

    fn run(root: &Path, limits: ScanLimits) -> Found {
        let stop = AtomicBool::new(false);
        scan(root, limits, &stop, &mut |_| {}).unwrap_or_default()
    }

    #[test]
    fn repositories_are_found_where_they_are_looked_for() {
        let root = tree("found");
        init(&root.join("app"));
        init(&root.join("app/plugins/inner"));
        init(&root.join("my projects/Größe 日本"));
        init(&root.join("node_modules/dep"));
        init(&root.join(".hidden/secret"));
        init(&root.join("a/b/c/d"));
        init(&root.join("a/b/c/d/e"));
        let _ = std::fs::write(root.join("a/file.txt"), "x");
        let found = run(&root, LIMITS);
        assert_eq!(
            names(&root, &found),
            vec![
                "a/b/c/d",
                "app",
                "app/plugins/inner",
                "my projects/Größe 日本"
            ]
        );
        assert!(!found.cut);
        // Not as deep.
        let shallow = ScanLimits { depth: 1, ..LIMITS };
        assert_eq!(names(&root, &run(&root, shallow)), vec!["app"]);
        // At most so many.
        let few = ScanLimits {
            max_repos: 2,
            ..LIMITS
        };
        let cut = run(&root, few);
        assert_eq!(cut.repos.len(), 2);
        assert!(cut.cut);
        // The folder picked is itself one.
        let inner = run(&root.join("app"), LIMITS);
        assert_eq!(names(&root, &inner), vec!["app", "app/plugins/inner"]);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_missing_folder_is_an_error_and_an_empty_one_finds_nothing() {
        let root = tree("empty");
        let stop = AtomicBool::new(false);
        assert_eq!(
            scan(&root, LIMITS, &stop, &mut |_| {}),
            Ok(Found::default())
        );
        assert!(scan(&root.join("gone"), LIMITS, &stop, &mut |_| {}).is_err());
        // Stopped at once.
        init(&root.join("r"));
        let stopped = AtomicBool::new(true);
        assert_eq!(
            scan(&root, LIMITS, &stopped, &mut |_| {}),
            Ok(Found::default())
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[cfg(unix)]
    #[test]
    fn links_are_not_followed() {
        let root = tree("loop");
        init(&root.join("real"));
        let _ = std::fs::create_dir_all(root.join("dir"));
        // A loop, and a second way to the repository.
        let _ = std::os::unix::fs::symlink(&root, root.join("dir/up"));
        let _ = std::os::unix::fs::symlink(root.join("real"), root.join("again"));
        let found = run(&root, LIMITS);
        assert_eq!(names(&root, &found), vec!["real"]);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn skipped_folders() {
        assert!(walked("src"));
        assert!(walked("my code"));
        assert!(!walked(".git"));
        assert!(!walked(".cache"));
        assert!(!walked("node_modules"));
        assert!(!walked("target"));
    }
}
