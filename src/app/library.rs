//! The repository library: repositories found in folders the user configured (roots) and
//! next to repositories they open (nearby folders), kept per profile apart from the 12
//! Recent ones. Scans run in the background with the shared bounded engine
//! (`repo_scan::engine`); the window thread only reads what they sent.

mod actions;
mod checks;
mod identity;
mod model;
mod roots_dialog;
mod row;
mod rows;
mod scheduler;
mod status;
mod view;

pub(super) use model::Libraries;
use model::{Library, Source};
use scheduler::{Job, Scheduler};
pub(super) use view::{LibraryClick, search_id};

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use super::home_recent::now_secs;
use super::repo_scan::{NEARBY_LIMITS, ROOT_LIMITS, ScanLimits};

/// How long after the window first shows the configured roots are scanned.
const START_AFTER: Duration = Duration::from_secs(2);
/// How often, at most, the roots' ages are looked at.
const CHECK_EVERY: Duration = Duration::from_secs(5);
/// A scan older than this is done again (roots while the window is in use, nearby folders
/// when a repository in them is opened).
pub(in crate::app) const STALE_AFTER: i64 = 15 * 60;

/// The libraries of every profile, and the scans that fill them.
#[derive(Default)]
pub(super) struct LibraryState {
    saved: Libraries,
    scheduler: Scheduler,
    checks: checks::Checks,
    /// When the window was first looked after.
    started: Option<Instant>,
    checked: Option<Instant>,
    /// Counts the changes to the libraries (the rows are made again after one).
    rev: u64,
    view: view::LibraryView,
    rows: view::RowsCache,
}

impl LibraryState {
    pub fn new(saved: Libraries) -> Self {
        Self {
            saved,
            ..Self::default()
        }
    }

    pub fn saved(&self) -> &Libraries {
        &self.saved
    }

    /// The profile's library, to change.
    fn lib(&mut self, profile: u64) -> &mut Library {
        self.rev += 1;
        self.saved.entry(profile).or_default()
    }

    fn get(&self, profile: u64) -> Option<&Library> {
        self.saved.get(&profile)
    }

    /// Another profile is in use: nothing the old one's scans send is applied.
    pub fn switch_profile(&mut self) {
        self.scheduler.retire_all();
        self.view = Default::default();
        // Its roots are looked at again as on start.
        self.checked = None;
        self.rev += 1;
    }

    pub fn forget_profile(&mut self, profile: u64) {
        self.saved.remove(&profile);
        self.rev += 1;
    }

    /// Applies what the scans sent, and once the window has been up a moment (and while it is
    /// in use), scans the enabled roots whose last scan is old.
    fn tick(&mut self, profile: u64, active: bool, ctx: &egui::Context) {
        self.scheduler.set_repaint(ctx);
        self.checks.set_repaint(ctx);
        let now = now_secs();
        let lib = self.saved.entry(profile).or_default();
        if self.scheduler.poll(profile, lib, now) {
            self.rev += 1;
        }
        self.answered(profile);
        let started = *self.started.get_or_insert_with(Instant::now);
        if started.elapsed() < START_AFTER {
            ctx.request_repaint_after(START_AFTER);
            return;
        }
        if !active || self.checked.is_some_and(|c| c.elapsed() < CHECK_EVERY) {
            return;
        }
        // The first look after start scans every root: what the cache shows is only what was
        // there when gitcraft last looked.
        let first = self.checked.replace(Instant::now()).is_none();
        let due: Vec<u64> = self.get(profile).map_or_else(Vec::new, |lib| {
            lib.roots
                .iter()
                .filter(|r| {
                    r.enabled
                        && (first || r.last.as_ref().is_none_or(|l| now - l.at >= STALE_AFTER))
                })
                .map(|r| r.id)
                .collect()
        });
        for id in due {
            self.rescan(profile, &Source::Root(id));
        }
    }

    /// Scans `source` again (now, or once a worker is free).
    pub fn rescan(&mut self, profile: u64, source: &Source) {
        let Some(lib) = self.get(profile) else {
            return;
        };
        let job = match source {
            Source::Root(id) => lib.root(*id).filter(|r| r.enabled).map(|r| Job {
                source: source.clone(),
                path: r.path.clone(),
                limits: ScanLimits {
                    depth: usize::from(r.depth),
                    ..ROOT_LIMITS
                },
            }),
            Source::Nearby(parent) => lib.has_source(source).then(|| Job {
                source: source.clone(),
                path: parent.clone(),
                limits: NEARBY_LIMITS,
            }),
        };
        if let Some(job) = job {
            self.scheduler.queue(profile, job);
        }
    }

    /// Every enabled root and nearby folder, scanned again.
    pub fn rescan_all(&mut self, profile: u64) {
        let sources: Vec<Source> = self.get(profile).map_or_else(Vec::new, |lib| {
            let roots = lib.roots.iter().map(|r| Source::Root(r.id));
            let nearby = lib.nearby.iter().map(|n| Source::Nearby(n.parent.clone()));
            roots.chain(nearby).collect()
        });
        for source in sources {
            self.rescan(profile, &source);
        }
    }

    /// Applies what the file-system checks found (for `profile`, the one in use).
    fn answered(&mut self, profile: u64) {
        for answer in self.checks.poll() {
            match answer {
                checks::Answer::Nearby {
                    profile: p,
                    parent: Some(parent),
                } if p == profile => self.look_nearby(profile, parent),
                checks::Answer::Gone { profile: p, path } if p == profile => {
                    if let Some(lib) = self.saved.get_mut(&profile)
                        && lib.drop_entry(&path)
                    {
                        self.rev += 1;
                    }
                }
                _ => {}
            }
        }
    }

    /// A local repository at `root` (its canonical top folder) was opened: its folder's other
    /// repositories are looked for, unless that folder was looked in lately or is too broad.
    /// Whether it is (against the canonical home and temporary folders, and mounts) is found
    /// on another thread.
    pub fn noticed_open(&mut self, profile: u64, root: &Path) {
        if !self.get(profile).is_none_or(|l| l.nearby_enabled)
            || identity::is_far(root)
            || !root.is_absolute()
        {
            return;
        }
        self.checks.nearby(profile, root.to_owned());
    }

    /// A row's repository at `path` didn't open: once found gone from the disk, the library
    /// forgets it (Recent keeps what it has).
    pub fn open_failed(&mut self, profile: u64, path: &Path) {
        self.checks.gone(profile, path.to_owned());
    }

    /// Looks in `parent`, a folder fit to look in near a repository opened, unless it was
    /// looked in lately (however that went: only the user asks for it again sooner).
    fn look_nearby(&mut self, profile: u64, parent: PathBuf) {
        if !self.get(profile).is_none_or(|l| l.nearby_enabled) {
            return;
        }
        let lib = self.lib(profile);
        let gone = lib.note_nearby(parent);
        // As the library spells it (a folder noted before keeps its first spelling).
        let Some(first) = lib.nearby.first() else {
            return;
        };
        let parent = first.parent.clone();
        let fresh = first
            .last
            .as_ref()
            .is_some_and(|l| now_secs() - l.at < STALE_AFTER);
        for parent in gone {
            self.scheduler.retire(&Source::Nearby(parent));
        }
        if !fresh {
            self.rescan(profile, &Source::Nearby(parent));
        }
    }

    /// Adds `path` as a root `depth` deep (a root already, at least that deep) and scans it.
    /// A path that isn't valid UTF-8 can't be saved: it isn't added.
    pub fn add_root(&mut self, profile: u64, path: PathBuf, depth: u8) {
        if path.to_str().is_none() {
            return;
        }
        let lib = self.lib(profile);
        let before = lib.root_at(&path, None).and_then(|id| lib.root(id)).map(|r| r.depth);
        let id = lib.add_root(path, depth);
        let after = lib.root(id).map(|r| r.depth);
        // Deeper now: a scan under way for the old depth is dropped.
        if before.is_some() && before != after {
            self.scheduler.retire(&Source::Root(id));
        }
        self.rescan(profile, &Source::Root(id));
    }

    /// Takes root `id` away, with what only it found. Tabs, Recent and workspaces stay.
    pub fn remove_root(&mut self, profile: u64, id: u64) {
        self.scheduler.retire(&Source::Root(id));
        self.lib(profile).remove_root(id);
    }

    pub fn set_enabled(&mut self, profile: u64, id: u64, on: bool) {
        self.scheduler.retire(&Source::Root(id));
        self.lib(profile).set_enabled(id, on);
        if on {
            self.rescan(profile, &Source::Root(id));
        }
    }

    /// Scans root `id` `depth` deep from now on (a scan under way for the old depth is
    /// dropped; the complete one that follows settles what is in reach).
    pub fn set_depth(&mut self, profile: u64, id: u64, depth: u8) {
        self.scheduler.retire(&Source::Root(id));
        if let Some(root) = self.lib(profile).root_mut(id) {
            root.depth = depth.min(model::MAX_DEPTH);
        }
        self.rescan(profile, &Source::Root(id));
    }

    /// Root `id` is `path` now: what the old folder found goes. Not when another root has that
    /// folder already (nor for a path that couldn't be saved).
    pub fn move_root(&mut self, profile: u64, id: u64, path: PathBuf) {
        let taken = self
            .get(profile)
            .is_some_and(|l| l.root_at(&path, Some(id)).is_some());
        if taken || path.to_str().is_none() {
            return;
        }
        let source = Source::Root(id);
        self.scheduler.retire(&source);
        let lib = self.lib(profile);
        lib.forget_source(&source);
        if let Some(root) = lib.root_mut(id) {
            root.path = path;
            root.last = None;
        }
        self.rescan(profile, &source);
    }

    pub fn set_nearby(&mut self, profile: u64, on: bool) {
        if !on && let Some(lib) = self.get(profile) {
            for n in lib.nearby.clone() {
                self.scheduler.retire(&Source::Nearby(n.parent));
            }
        }
        self.lib(profile).set_nearby_enabled(on);
    }

    pub fn forget_nearby(&mut self, profile: u64, parent: &Path) {
        let source = Source::Nearby(parent.to_owned());
        self.scheduler.retire(&source);
        let lib = self.lib(profile);
        if let Some(at) = lib.nearby_at(parent) {
            lib.nearby.remove(at);
        }
        lib.forget_source(&source);
    }

    pub fn stop(&mut self, source: &Source) {
        self.scheduler.stop(source);
    }

    pub fn hide(&mut self, profile: u64, path: &Path) {
        self.lib(profile).hide(path);
    }

    pub fn unhide_all(&mut self, profile: u64) {
        self.lib(profile).hidden.clear();
    }
}

#[cfg(test)]
#[path = "library/fs_tests.rs"]
mod fs_tests;
#[cfg(test)]
#[path = "library/tests.rs"]
mod tests;
