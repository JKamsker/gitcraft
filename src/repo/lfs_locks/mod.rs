//! Git LFS file locks in the tab: who holds which file (a padlock on its row), Lock and Unlock
//! in a file's menu, the LFS locks panel, a warning on changed files someone else holds, and
//! after a push, an offer to give back the locks on what it pushed.

mod banner;
mod jobs;
mod panel;
#[cfg(test)]
mod tests;

use std::collections::{BTreeSet, HashMap, HashSet};
use std::time::{Duration, Instant};

use super::RepoTab;
use crate::git::{self, LfsLock, StatusFile};

/// How long a background read of the locks holds before the next one: they change when other
/// people lock files, which nothing tells this tab about.
const REREAD: Duration = Duration::from_secs(5 * 60);

/// The locks of the tab's repository and what is being done with them.
#[derive(Default)]
pub(in crate::repo) struct LockState {
    locks: Vec<LfsLock>,
    /// Index into `locks` by path.
    by_path: HashMap<String, usize>,
    /// The locks have been read at least once, the last time without an error.
    read_ok: bool,
    /// Why the last read failed.
    error: Option<String>,
    read_error: Option<ReadError>,
    /// The server has said it has no locks API: it isn't asked again in the background.
    unsupported: bool,
    /// A read is on its way: it was asked for when.
    reading: Option<Instant>,
    /// Another read was asked for meanwhile: it runs once this one is in.
    read_again: Option<git::Asked>,
    read_at: Option<Instant>,
    /// Which reading this is: an older answer is dropped.
    reads: u64,
    /// Whether each changed file asked about is lockable, as of `.gitattributes` `patterns_seen`.
    lockable: HashMap<String, bool>,
    lockable_for: Option<u64>,
    lockable_running: bool,
    /// Counts the answers taken into `lockable`, for `derived_for`.
    lockable_gen: u64,
    patterns_seen: Vec<String>,
    /// The patterns `.gitattributes` at the root marks lockable.
    patterns: Vec<String>,
    /// A lock, unlock or pattern change running: what it is.
    busy: Option<String>,
    /// How the last of them failed, for the panel.
    job_error: Option<String>,
    panel: Option<Panel>,
    /// After a push: our locks on files it pushed, with no changes left, offered back.
    offer: Vec<LfsLock>,
    /// The paths the last push carried, until a read of the locks makes the offer from them.
    pushed: Option<HashSet<String>>,
    /// What the banners show, worked out when the locks or the working tree change rather than
    /// every frame, and what for.
    derived: Derived,
    derived_for: Option<(u64, u64, u64)>,
}

/// The changed files the banners name.
#[derive(Default)]
struct Derived {
    /// Changed files someone else holds (indices into `locks`).
    theirs: Vec<usize>,
    /// Changed lockable files nobody holds (not new ones: nobody else can have those).
    unlocked: Vec<String>,
    /// Every changed path.
    changed: HashSet<String>,
}

/// What the LFS locks panel has typed and picked.
#[derive(Default)]
pub(in crate::repo) struct Panel {
    filter: String,
    only_mine: bool,
    /// Lock ids.
    chosen: BTreeSet<String>,
    /// Force unlock was clicked once, for these lock ids: the second click confirms them.
    confirm_force: Option<BTreeSet<String>>,
    new_pattern: String,
}

/// What the views ask of the locks.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum LockAct {
    Refresh,
    OpenPanel,
    ClosePanel,
    Lock(Vec<String>),
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
        self.by_path.get(path).and_then(|&i| self.locks.get(i))
    }

    pub(in crate::repo) fn is_lockable(&self, path: &str) -> bool {
        self.lockable.get(path).copied().unwrap_or(false)
    }

    fn set_locks(&mut self, locks: Vec<LfsLock>) {
        self.by_path = locks
            .iter()
            .enumerate()
            .map(|(i, l)| (l.path.clone(), i))
            .collect();
        self.locks = locks;
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

    /// Works out what the banners show from the locks, the lockable files and `work`.
    fn derive(&mut self, changed: &[&StatusFile]) {
        let paths: HashSet<String> = changed.iter().map(|f| f.path.clone()).collect();
        let mut theirs: Vec<usize> = paths
            .iter()
            .filter_map(|p| self.by_path.get(p).copied())
            .filter(|&i| self.locks[i].theirs())
            .collect();
        theirs.sort_by(|&a, &b| self.locks[a].path.cmp(&self.locks[b].path));
        let mut unlocked: Vec<String> = changed
            .iter()
            .filter(|f| !matches!(f.code, '?' | 'A'))
            .map(|f| &f.path)
            .filter(|p| self.is_lockable(p) && !self.by_path.contains_key(*p))
            .cloned()
            .collect();
        unlocked.sort();
        unlocked.dedup();
        self.derived = Derived {
            theirs,
            unlocked,
            changed: paths,
        };
    }
}

/// "Locked by Ana 2 days ago", "Locked by you just now".
pub(in crate::repo) fn lock_label(lock: &LfsLock, now: i64) -> String {
    let who = match lock.ours {
        Some(true) => crate::i18n::text("repo-workflows-lfs_locks-lock-label-bb0347a4"),
        _ if lock.owner.is_empty() => {
            crate::i18n::text("repo-workflows-lfs_locks-lock-label-2a59d59e")
        }
        _ => lock.owner.as_str(),
    };
    match lock.locked_at {
        Some(at) => crate::i18n::message(
            "repo-workflows-lfs_locks-lock-label-398e9b81",
            &[
                ("who", (who).into()),
                (
                    "git-relative-time-at-now-1",
                    (git::relative_time(at, now)).clone().into(),
                ),
            ],
        ),
        None => crate::i18n::message(
            "repo-workflows-lfs_locks-lock-label-6329feb6",
            &[("who", (who).into())],
        ),
    }
}

/// Our locks on files the push carried that have no changes left in the working tree.
pub(in crate::repo) fn unlock_offer(
    locks: &[LfsLock],
    pushed: &HashSet<String>,
    changed: &HashSet<String>,
) -> Vec<LfsLock> {
    locks
        .iter()
        .filter(|l| l.ours == Some(true) && pushed.contains(&l.path) && !changed.contains(&l.path))
        .cloned()
        .collect()
}

/// Whether a push labelled so pushed the checked-out branch (`current`): "Push", or "Push x to
/// origin" for its first push; not a tag nor another branch.
pub(in crate::repo) fn pushes_current(label: &str, current: Option<&str>) -> bool {
    label == "Push"
        || current.is_some_and(|c| {
            label
                .strip_prefix("Push ")
                .and_then(|rest| rest.strip_prefix(c))
                .is_some_and(|rest| rest.starts_with(" to "))
        })
}

/// Whether the locks are due to be read in the background: background fetching is on
/// (`auto`), the server has a locks API, and they weren't read lately.
pub(in crate::repo) fn reads_due(auto: bool, unsupported: bool, read_at: Option<Instant>) -> bool {
    auto && !unsupported && read_at.is_none_or(|at| at.elapsed() > REREAD)
}

impl RepoTab {
    /// Every frame: the reads that are due, then the panel, if open.
    pub(in crate::repo) fn lfs_locks_ui(
        &mut self,
        ctx: &egui::Context,
        acts: &mut Vec<super::Action>,
    ) {
        if !self.lfs {
            return;
        }
        self.lfs_locks_tick(ctx);
        self.locks_panel(ctx, acts);
    }

    /// Reads the locks when they are due, which changed files are lockable once the working
    /// tree was read again, and what the banners show when any of that changed.
    pub(in crate::repo) fn lfs_locks_tick(&mut self, ctx: &egui::Context) {
        if !self.lfs {
            return;
        }
        let auto = crate::settings::Settings::get(ctx).auto_fetch_minutes > 0;
        let state = &self.lfs_locks;
        let due = reads_due(auto, state.unsupported, state.read_at);
        if due && state.reading.is_none() {
            self.read_locks(git::Asked::Background, ctx);
        }
        if self.lfs_locks.lockable_for != Some(self.work_read) && !self.lfs_locks.lockable_running {
            self.read_lockable(ctx);
        }
        let key = (
            self.lfs_locks.reads,
            self.work_read,
            self.lfs_locks.lockable_gen,
        );
        if self.lfs_locks.derived_for != Some(key) {
            let work = &self.work;
            let changed: Vec<&StatusFile> = work
                .unstaged
                .iter()
                .chain(&work.staged)
                .chain(&work.conflicted)
                .collect();
            self.lfs_locks.derive(&changed);
            self.lfs_locks.derived_for = Some(key);
        }
    }

    /// Every file with changes in the working tree, staged or not.
    fn changed_files(&self) -> impl Iterator<Item = &StatusFile> {
        self.work
            .unstaged
            .iter()
            .chain(&self.work.staged)
            .chain(&self.work.conflicted)
    }

    /// A push went through: what it carried is worked out, the locks are read again, and ours
    /// on those files with no changes left are offered back. Only for a push of the checked-out
    /// branch (`label`), not a tag nor another branch.
    pub(in crate::repo) fn locks_after_push(&mut self, label: &str, ctx: &egui::Context) {
        if !self.lfs || !pushes_current(label, self.refs.current.as_deref()) {
            return;
        }
        let path = self.path.clone();
        self.then(
            ctx,
            move || jobs::caught(|| git::pushed_paths(&path)),
            |tab, pushed, ctx| {
                let Ok(pushed) = pushed else { return };
                if pushed.is_empty() {
                    return;
                }
                tab.lfs_locks.pushed = Some(pushed.into_iter().collect());
                tab.read_locks(git::Asked::Background, ctx);
            },
        );
    }
}

#[cfg(test)]
mod fluent_identity_tests {
    use super::*;
    #[test]
    fn fluent_partition_lfs_lock_display_preserves_raw_push_classification() {
        let lock = LfsLock {
            id: "server-id".into(),
            path: "Art/My File.psd".into(),
            owner: "Opaque Owner".into(),
            locked_at: None,
            ours: None,
        };
        for (language, displayed) in [
            ("en-US", "Locked by Opaque Owner"),
            ("de", "Von Opaque Owner gesperrt"),
        ] {
            crate::i18n::set_language(language);
            assert_eq!(lock_label(&lock, 0), displayed);
            let rendered = crate::repo::messages::display_operation("Push topic to origin");
            assert_eq!(
                rendered,
                if language == "de" {
                    "topic nach origin pushen"
                } else {
                    "Push topic to origin"
                }
            );
            assert_eq!(
                crate::repo::messages::display_operation("opaque external operation"),
                "opaque external operation"
            );
            assert!(pushes_current("Push topic to origin", Some("topic")));
            assert!(!pushes_current("Push tag v1 to origin", Some("topic")));
            assert!(!pushes_current(
                "external arbitrary operation",
                Some("topic")
            ));
            assert_eq!(lock.id, "server-id");
            assert_eq!(lock.path, "Art/My File.psd");
        }
        crate::i18n::set_language("en-US");
    }
}

#[derive(Clone)]
struct ReadError {
    raw: String,
    detail: String,
}
impl LockState {
    fn remember_read_error(&mut self, detail: String) {
        let e = &detail;
        let raw = format!("Could not read the LFS locks: {e}");
        self.error = Some(raw.clone());
        self.read_error = Some(ReadError { raw, detail });
    }
    fn display_read_error(&self, raw: &str) -> String {
        match &self.read_error {
            Some(receipt) if receipt.raw == raw => crate::i18n::message(
                "repo-workflows-lfs_locks-locks-read-05347473",
                &[("e", receipt.detail.as_str().into())],
            ),
            _ => raw.to_owned(),
        }
    }
}

#[cfg(test)]
mod fluent_r1_read_tests {
    use super::*;
    #[test]
    fn fluent_partition_r1_lfs_locks_cached_read_error_preserves_raw_detail_and_generation()
    -> Result<(), String> {
        let detail = "provider; ;\nHistory worker stopped: {$x} 日本語";
        crate::i18n::set_language("de");
        let mut state = LockState::default();
        state.remember_read_error(detail.into());
        let raw = state.error.clone().ok_or("missing test value")?;
        for lang in ["de", "en-US", "de"] {
            crate::i18n::set_language(lang);
            let shown = state.display_read_error(&raw);
            assert!(shown.contains(detail));
            assert_eq!(shown == raw, lang == "en-US");
            assert_eq!(state.error.as_ref(), Some(&raw));
        }
        state.read_error = None;
        crate::i18n::set_language("de");
        assert_eq!(state.display_read_error(&raw), raw);
        crate::i18n::set_language("en-US");

        Ok(())
    }
}
