//! Git LFS file locks in the tab: who holds which file (a padlock on its row), Lock and Unlock
//! in a file's menu, the LFS locks panel, a warning on changed files someone else holds, and
//! after a push, an offer to give back the locks on what was pushed.

mod banner;
mod panel;
#[cfg(test)]
mod tests;

use std::collections::{BTreeSet, HashSet};
use std::time::{Duration, Instant};

use super::RepoTab;
use crate::git::{self, Asked, LfsLock};

/// How long a background read of the locks holds before the next one: they change when other
/// people lock files, which nothing tells this tab about.
const REREAD: Duration = Duration::from_secs(5 * 60);

/// The locks of the tab's repository and what is being done with them.
#[derive(Default)]
pub(in crate::repo) struct LockState {
    locks: Vec<LfsLock>,
    /// Why the last read failed.
    error: Option<String>,
    /// A read is on its way: it was asked for when.
    reading: Option<Instant>,
    read_at: Option<Instant>,
    /// Which reading this is: an older answer is dropped.
    reads: u64,
    /// The changed files that are lockable, for the working-tree read numbered so.
    lockable: HashSet<String>,
    lockable_for: Option<u64>,
    /// The patterns `.gitattributes` marks lockable.
    patterns: Vec<String>,
    /// A lock, unlock or pattern change running: what it is.
    busy: Option<String>,
    panel: Option<Panel>,
    /// After a push: our locks on files with no changes left, offered to be given back.
    offer: Vec<LfsLock>,
    /// The next read is the one after a push: it makes the offer.
    after_push: bool,
}

/// What the LFS locks panel has typed and picked.
#[derive(Default)]
pub(in crate::repo) struct Panel {
    filter: String,
    only_mine: bool,
    /// Lock ids.
    chosen: BTreeSet<String>,
    /// Force unlock was clicked once: the second click confirms.
    confirm_force: bool,
    new_pattern: String,
}

/// What the views ask of the locks.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum LockAct {
    Refresh,
    OpenPanel,
    ClosePanel,
    Lock(String),
    /// Give back these locks (id, path); `force`: someone else's.
    Unlock {
        locks: Vec<(String, String)>,
        force: bool,
    },
    /// Make files matching this pattern lockable.
    TrackLockable(String),
    DismissOffer,
}

impl LockState {
    /// The lock on `path`, if anyone holds it.
    pub(in crate::repo) fn lock_on(&self, path: &str) -> Option<&LfsLock> {
        self.locks.iter().find(|l| l.path == path)
    }

    pub(in crate::repo) fn is_lockable(&self, path: &str) -> bool {
        self.lockable.contains(path)
    }

    /// The locks shown in the panel: matching the filter (path or owner), only ours if asked.
    fn shown(&self, panel: &Panel) -> Vec<&LfsLock> {
        let needle = panel.filter.trim().to_lowercase();
        self.locks
            .iter()
            .filter(|l| !panel.only_mine || l.ours == Some(true))
            .filter(|l| {
                needle.is_empty()
                    || l.path.to_lowercase().contains(&needle)
                    || l.owner.to_lowercase().contains(&needle)
            })
            .collect()
    }
}

/// "Locked by Ana · 2 d", "Locked by you · now".
pub(in crate::repo) fn lock_label(lock: &LfsLock, now: i64) -> String {
    let who = match lock.ours {
        Some(true) => "you",
        _ if lock.owner.is_empty() => "someone",
        _ => lock.owner.as_str(),
    };
    match lock.locked_at {
        Some(at) => format!("Locked by {who} · {}", git::held_for(at, now)),
        None => format!("Locked by {who}"),
    }
}

impl RepoTab {
    /// Every frame: reads the locks when they are due, and which changed files are lockable
    /// when the working tree was read again; then draws the panel, if open.
    pub(in crate::repo) fn lfs_locks_ui(
        &mut self,
        ctx: &egui::Context,
        acts: &mut Vec<super::Action>,
    ) {
        if !self.lfs {
            return;
        }
        let due = self
            .lfs_locks
            .read_at
            .is_none_or(|at| at.elapsed() > REREAD);
        if due && self.lfs_locks.reading.is_none() {
            self.read_locks(Asked::Background, ctx);
        }
        if self.lfs_locks.lockable_for != Some(self.work_read) {
            self.read_lockable(ctx);
        }
        self.locks_panel(ctx, acts);
    }

    /// Asks the server for the locks, in the background.
    fn read_locks(&mut self, asked: Asked, ctx: &egui::Context) {
        let state = &mut self.lfs_locks;
        state.reads += 1;
        state.reading = Some(Instant::now());
        let read = state.reads;
        let path = self.path.clone();
        let creds = self.credentials.clone();
        self.then(
            ctx,
            move || {
                let locks = git::lfs_locks(&path, creds.as_ref(), asked);
                (locks, git::lockable_patterns(&path))
            },
            move |tab, (locks, patterns), _| tab.locks_read(read, locks, patterns),
        );
    }

    fn locks_read(
        &mut self,
        read: u64,
        locks: Result<Vec<LfsLock>, String>,
        patterns: Vec<String>,
    ) {
        let changed = self.changed_paths();
        let state = &mut self.lfs_locks;
        if read != state.reads {
            return;
        }
        state.reading = None;
        state.read_at = Some(Instant::now());
        state.patterns = patterns;
        match locks {
            Ok(locks) => {
                state.locks = locks;
                state.error = None;
            }
            Err(e) => state.error = Some(format!("Could not read the LFS locks: {e}")),
        }
        if std::mem::take(&mut state.after_push) {
            state.offer = unlock_offer(&state.locks, &changed);
        }
        if let Some(panel) = &mut state.panel {
            let ids: HashSet<&str> = state.locks.iter().map(|l| l.id.as_str()).collect();
            panel.chosen.retain(|id| ids.contains(id.as_str()));
        }
    }

    /// Every file with changes in the working tree, staged or not.
    fn changed_paths(&self) -> HashSet<String> {
        self.work
            .unstaged
            .iter()
            .chain(&self.work.staged)
            .chain(&self.work.conflicted)
            .map(|f| f.path.clone())
            .collect()
    }

    /// Asks git which changed files are lockable, for the working tree as last read.
    fn read_lockable(&mut self, ctx: &egui::Context) {
        let read = self.work_read;
        self.lfs_locks.lockable_for = Some(read);
        let paths: Vec<String> = self.changed_paths().into_iter().collect();
        if paths.is_empty() {
            self.lfs_locks.lockable.clear();
            return;
        }
        let path = self.path.clone();
        self.then(
            ctx,
            move || git::lockable_paths(&path, &paths),
            move |tab, found, _| {
                if tab.lfs_locks.lockable_for == Some(read) {
                    // A failure (an old git) leaves no file lockable: only the warnings go.
                    tab.lfs_locks.lockable = found.unwrap_or_default().into_iter().collect();
                }
            },
        );
    }

    pub(in crate::repo) fn apply_lock_act(&mut self, act: LockAct, ctx: &egui::Context) {
        match act {
            LockAct::Refresh => self.read_locks(Asked::ByUser, ctx),
            LockAct::OpenPanel => {
                self.lfs_locks.panel = Some(Panel::default());
                self.read_locks(Asked::ByUser, ctx);
            }
            LockAct::ClosePanel => self.lfs_locks.panel = None,
            LockAct::DismissOffer => self.lfs_locks.offer.clear(),
            LockAct::Lock(path) => {
                let label = format!("Lock {path}");
                self.lock_job(label, ctx, move |repo, creds| {
                    git::lfs_lock(repo, &path, creds).map(|_| ())
                });
            }
            LockAct::Unlock { locks, force } => {
                let label = match (locks.as_slice(), force) {
                    ([(_, path)], false) => format!("Unlock {path}"),
                    ([(_, path)], true) => format!("Force unlock {path}"),
                    (_, false) => format!("Unlock {} files", locks.len()),
                    (_, true) => format!("Force unlock {} files", locks.len()),
                };
                let ids: HashSet<String> = locks.iter().map(|(id, _)| id.clone()).collect();
                self.lfs_locks.offer.retain(|l| !ids.contains(&l.id));
                self.lock_job(label, ctx, move |repo, creds| {
                    git::lfs_unlock(repo, &locks, force, creds)
                });
            }
            LockAct::TrackLockable(pattern) => {
                let label = format!("Make {} lockable", pattern.trim());
                self.lock_job(label, ctx, move |repo, _| {
                    git::track_lockable(repo, &pattern).map(|_| ())
                });
            }
        }
    }

    /// Runs `work` in the background, then says how it went and reads the locks again.
    fn lock_job(
        &mut self,
        label: String,
        ctx: &egui::Context,
        work: impl FnOnce(&std::path::Path, Option<&git::Credentials>) -> Result<(), String>
        + Send
        + 'static,
    ) {
        if self.lfs_locks.busy.is_some() {
            return;
        }
        self.lfs_locks.busy = Some(label.clone());
        let path = self.path.clone();
        let creds = self.credentials.clone();
        self.then(
            ctx,
            move || work(&path, creds.as_ref()),
            move |tab, result, ctx| {
                tab.lfs_locks.busy = None;
                tab.status = Some(match &result {
                    Ok(()) => (format!("{label}: done"), false),
                    Err(e) => (format!("{label} failed: {e}"), true),
                });
                if let Some(panel) = &mut tab.lfs_locks.panel {
                    panel.confirm_force = false;
                    if result.is_ok() {
                        panel.chosen.clear();
                        panel.new_pattern.clear();
                    }
                }
                // Made lockable: `.gitattributes` changed, which status shows.
                tab.reload_status(ctx);
                tab.read_locks(Asked::ByUser, ctx);
            },
        );
    }

    /// A push went through: the locks are read again, and those of ours on files with no
    /// changes left are offered to be given back.
    pub(in crate::repo) fn locks_after_push(&mut self, ctx: &egui::Context) {
        if !self.lfs {
            return;
        }
        self.lfs_locks.after_push = true;
        self.read_locks(Asked::Background, ctx);
    }
}

/// Our locks on files without changes in the working tree: what was pushed, done with.
pub(in crate::repo) fn unlock_offer(locks: &[LfsLock], changed: &HashSet<String>) -> Vec<LfsLock> {
    locks
        .iter()
        .filter(|l| l.ours == Some(true) && !changed.contains(&l.path))
        .cloned()
        .collect()
}
