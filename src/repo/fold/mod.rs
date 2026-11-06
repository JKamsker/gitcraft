//! Files better read folded: generated ones (`linguist-generated`, `-diff` in
//! `.gitattributes`), lockfiles, minified files and the patterns chosen in the settings. They
//! start folded in file lists and diffs, behind a line like _+1,204 −880, lockfile_.

mod banner;
mod list;
mod rules;
#[cfg(test)]
mod tests;

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

pub(super) use banner::folded_diff;
pub(super) use list::split;
pub(super) use rules::{FoldReason, Rules, stats_line};

use crate::git::{FoldAttr, WorkStatus};

/// While a `.gitattributes` has changes, how often its files' attributes are read again (it
/// may have been edited since, and its status letter wouldn't say).
const REREAD_EVERY: Duration = Duration::from_secs(30);

/// What a tab knows of its files' attributes, asked of git in the background.
#[derive(Default)]
pub(crate) struct Folding {
    /// What the attributes said when last read, by path; kept while they are read again, so
    /// files don't jump out of their fold and back meanwhile.
    attrs: HashMap<String, FoldAttr>,
    /// Paths read (or given up on) since the attributes were last forgotten.
    fresh: HashSet<String>,
    /// Paths asked about, and when (by `epoch`): not asked again then.
    asked: HashMap<String, u64>,
    /// Changes each time answers come in or are forgotten: lists work out again what they fold.
    pub(super) generation: u64,
    /// Changes each time the attributes are forgotten. Shared with the reads under way, which
    /// stop when it moves on.
    epoch: Arc<AtomicU64>,
    /// What a `.gitattributes` looked like at the last status read (path and status letter of
    /// those changed) and the commit checked out: when either changes, the attributes may have.
    seen: (Vec<(String, char)>, String),
    /// When they were last forgotten.
    forgot: Option<Instant>,
}

impl Folding {
    /// Why `path` is folded, if it is (`rules`: the settings'). While its attributes aren't
    /// known yet, only its name decides.
    pub(super) fn reason(&self, rules: &Rules, path: &str) -> Option<FoldReason> {
        rules.reason(path, self.attrs.get(path).copied())
    }

    /// Whether anything is known of the attributes of `path` yet.
    pub(super) fn knows(&self, path: &str) -> bool {
        self.attrs.contains_key(path)
    }

    fn epoch(&self) -> u64 {
        self.epoch.load(Ordering::Relaxed)
    }

    /// Of `paths`, those not read since the attributes were last forgotten and not asked
    /// about since either.
    pub(super) fn unknown<'a>(&self, paths: impl IntoIterator<Item = &'a str>) -> Vec<String> {
        let epoch = self.epoch();
        let mut seen = HashSet::new();
        paths
            .into_iter()
            .filter(|p| !self.fresh.contains(*p) && self.asked.get(*p) != Some(&epoch))
            .filter(|p| seen.insert(*p))
            .map(str::to_owned)
            .collect()
    }

    /// `paths` are being asked about: not again until they are forgotten. Returns the epoch
    /// the answer is handed back with, and what tells the read whether it is still wanted.
    pub(super) fn asking(&mut self, paths: &[String]) -> (u64, Arc<AtomicU64>) {
        let epoch = self.epoch();
        for path in paths {
            self.asked.insert(path.clone(), epoch);
        }
        (epoch, self.epoch.clone())
    }

    /// The answer about `paths`, asked at `epoch`: what was `found`, and the error that left
    /// the others unread. Those count as saying nothing (their names decide) and aren't asked
    /// again until the attributes are forgotten. An answer asked before they were forgotten
    /// only fills in paths not read since, and leaves them to be asked again.
    pub(super) fn arrived(
        &mut self,
        epoch: u64,
        paths: Vec<String>,
        mut found: HashMap<String, FoldAttr>,
        error: Option<String>,
    ) {
        let current = epoch == self.epoch();
        for path in paths {
            if self.asked.get(&path) == Some(&epoch) {
                self.asked.remove(&path);
            }
            if self.fresh.contains(&path) {
                continue;
            }
            match found.remove(&path) {
                Some(attr) => {
                    self.attrs.insert(path.clone(), attr);
                }
                None if current && error.is_none() => {
                    self.attrs.insert(path.clone(), FoldAttr::Unspecified);
                }
                None => {
                    self.attrs
                        .entry(path.clone())
                        .or_insert(FoldAttr::Unspecified);
                }
            }
            if current {
                self.fresh.insert(path);
            }
        }
        self.generation += 1;
    }

    /// After a status read (at `now`): forgets the attributes when a `.gitattributes` changed
    /// or another commit is checked out, and every [`REREAD_EVERY`] while one has changes.
    pub(super) fn saw_status(&mut self, work: &WorkStatus, head: &str, now: Instant) {
        let changed: Vec<(String, char)> = work
            .staged
            .iter()
            .chain(&work.unstaged)
            .chain(&work.conflicted)
            .filter(|f| is_attributes_file(&f.path))
            .map(|f| (f.path.clone(), f.code))
            .collect();
        let seen = (changed, head.to_owned());
        let due = self
            .forgot
            .is_none_or(|at| now.saturating_duration_since(at) >= REREAD_EVERY);
        if seen != self.seen || (!seen.0.is_empty() && due) {
            self.seen = seen;
            self.forget(now);
        }
    }

    /// Forgets what was read: the files of the lists shown are asked about again (their last
    /// answers apply meanwhile), and reads under way stop.
    fn forget(&mut self, now: Instant) {
        self.forgot = Some(now);
        if self.fresh.is_empty() && self.asked.is_empty() {
            return;
        }
        self.epoch.fetch_add(1, Ordering::Relaxed);
        self.fresh.clear();
        self.asked.clear();
        self.generation += 1;
    }
}

/// Whether `path` is a `.gitattributes` file, at the top or in a folder.
fn is_attributes_file(path: &str) -> bool {
    path == ".gitattributes" || path.ends_with("/.gitattributes")
}

impl super::RepoTab {
    /// Asks git, in the background, what `.gitattributes` says of those of `paths` not known
    /// yet; lists and diffs fold them once the answer is in.
    pub(super) fn ask_fold_attributes(&mut self, paths: &[String], ctx: &egui::Context) {
        let paths = self.folding.unknown(paths.iter().map(String::as_str));
        if paths.is_empty() {
            return;
        }
        let (epoch, now) = self.folding.asking(&paths);
        let repo = self.path.clone();
        self.then(
            ctx,
            move || {
                let wanted = || now.load(Ordering::Relaxed) == epoch;
                let found = crate::git::fold_attributes(&repo, &paths, &wanted);
                (paths, found)
            },
            move |tab, (paths, (found, error)), _| {
                tab.folding.arrived(epoch, paths, found, error);
            },
        );
    }
}
