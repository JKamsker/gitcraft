//! _Scan a folder for repositories_: the folders under one picked, walked in the background a
//! few levels deep, and the repositories found there listed to open or add to the recent list.

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
    let n = |k: usize| {
        if k == 1 {
            "1 repository".to_owned()
        } else {
            format!("{k} repositories")
        }
    };
    let mut said = format!("Added {} to the recent list", n(added));
    let mut notes = Vec::new();
    if had > 0 {
        notes.push(format!(
            "{had} {} already there",
            if had == 1 { "was" } else { "were" }
        ));
    }
    if no_room > 0 {
        notes.push(format!(
            "{} didn't fit: it holds {MAX_RECENT}, remove some to make room",
            n(no_room)
        ));
    }
    if !notes.is_empty() {
        said.push_str(&format!(" ({})", notes.join("; ")));
    }
    said
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
                    "Repositories found"
                } else {
                    "Scanning for repositories…"
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
                    let add =
                        crate::repo::primary_button(ui, "Add to the recent list", count > 0, 200.0)
                            .on_hover_text(format!(
                                "Adds those not in it yet, as many as fit (it holds {MAX_RECENT})"
                            ));
                    if add.clicked() {
                        click = Some(ScanClick::AddAll);
                    }
                    let (label, what) = if done {
                        ("Close", ScanClick::Close)
                    } else {
                        ("Stop", ScanClick::Stop)
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

/// What the dialog says of how the scan ended, when it didn't look everywhere.
fn stopped_message(stopped: Stopped, found: usize) -> String {
    match stopped {
        Stopped::Repos => format!(
            "Stopped after {found} repositories: pick a folder further down to see the rest."
        ),
        Stopped::Folders => format!(
            "Stopped after looking in {} folders: pick a folder further down to see the rest.",
            LIMITS.max_dirs
        ),
        Stopped::Asked => format!("Stopped: found {found} so far."),
    }
}

/// The progress, the error, or the repositories found with a button each to open it.
fn scan_body(ui: &mut egui::Ui, scan: &RepoScan, t: &Tokens) -> Option<ScanClick> {
    let found = match &scan.result {
        None => {
            ui.horizontal(|ui| {
                ui.add(crate::spinner::Spinner::new());
                let p = scan.progress;
                ui.label(format!(
                    "Looked in {} folders, found {} repositories…",
                    p.folders, p.repos
                ));
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
        let said = match found.stopped {
            Some(stopped) => stopped_message(stopped, 0),
            None => format!(
                "No repositories found within {} folders of this one. Hidden folders and folders \
                 like node_modules, target and build aren't looked in.",
                LIMITS.depth
            ),
        };
        ui.label(said);
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
    if let Some(stopped) = found.stopped {
        ui.add_space(4.0);
        ui.label(
            RichText::new(stopped_message(stopped, found.repos.len()))
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
        assert_eq!(found.stopped, None);
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
        assert_eq!(cut.stopped, Some(Stopped::Repos));
        // The folder picked is itself one.
        let inner = run(&root.join("app"), LIMITS);
        assert_eq!(names(&root, &inner), vec!["app", "app/plugins/inner"]);
        let _ = std::fs::remove_dir_all(&root);
    }

    /// A repository named like a folder that isn't looked in (`build`, `.dotfiles`) is still
    /// found; what is inside it isn't looked at.
    #[test]
    fn repositories_named_like_skipped_folders_are_found() {
        let root = tree("skipped-names");
        init(&root.join("build"));
        init(&root.join("tools/target"));
        init(&root.join(".dotfiles"));
        init(&root.join("vendor/inside/repo"));
        let found = run(&root, LIMITS);
        let _ = std::fs::remove_dir_all(&root);
        assert_eq!(
            names(&root, &found),
            vec![".dotfiles", "build", "tools/target"]
        );
    }

    /// No more folders are kept to look in than the limit, however many a folder holds.
    #[test]
    fn the_folders_kept_stay_within_the_limit() {
        let root = tree("many");
        for i in 0..40 {
            let _ = std::fs::create_dir_all(root.join(format!("d{i:02}/x")));
        }
        init(&root.join("d00"));
        let mut most = 0;
        let stop = AtomicBool::new(false);
        let limits = ScanLimits {
            max_dirs: 10,
            ..LIMITS
        };
        let found = scan(&root, limits, &stop, &mut |p| most = most.max(p.folders));
        let _ = std::fs::remove_dir_all(&root);
        let found = found.unwrap_or_default();
        assert_eq!(found.stopped, Some(Stopped::Folders));
        assert!(most <= 10, "{most}");
        assert_eq!(names(&root, &found), vec!["d00"]);
    }

    #[test]
    fn a_missing_folder_is_an_error_and_an_empty_one_finds_nothing() {
        let root = tree("empty");
        let stop = AtomicBool::new(false);
        assert_eq!(
            scan(&root, LIMITS, &stop, &mut |_| {}),
            Ok(Found::default())
        );
        let gone = scan(&root.join("gone"), LIMITS, &stop, &mut |_| {});
        assert!(gone.is_err_and(|e| e.ends_with("isn't there any more.")));
        // Stopped at once: it says so, with nothing found.
        init(&root.join("r"));
        let stopped = AtomicBool::new(true);
        assert_eq!(
            scan(&root, LIMITS, &stopped, &mut |_| {}),
            Ok(Found {
                repos: Vec::new(),
                stopped: Some(Stopped::Asked)
            })
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

    /// Adding what a scan found never pushes out what the recent list has.
    #[test]
    fn adding_found_ones_keeps_the_recent_list() {
        let p = |s: &str| PathBuf::from(s);
        let mut recent: Vec<PathBuf> = (0..10).map(|i| p(&format!("/r{i}"))).collect();
        let found = [p("/r3"), p("/new1"), p("/new2"), p("/new3"), p("/new4")];
        assert_eq!(add_to_recent(&mut recent, &found, 12), (2, 1, 2));
        assert_eq!(recent.len(), 12);
        assert_eq!(recent[0], p("/r0"));
        assert_eq!(recent[10..], [p("/new1"), p("/new2")]);
        assert_eq!(
            added_message(2, 1, 2),
            format!(
                "Added 2 repositories to the recent list (1 was already there; 2 repositories \
                 didn't fit: it holds {MAX_RECENT}, remove some to make room)"
            )
        );
        assert_eq!(
            added_message(1, 0, 0),
            "Added 1 repository to the recent list"
        );
    }
}
