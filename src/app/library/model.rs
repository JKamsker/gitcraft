//! What a profile's repository library keeps: its configured roots, the nearby folders
//! inferred from repositories opened, the repositories found with where each was found, and
//! the paths hidden from it. Pure data and the rules that change it; no file system here.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::identity::PathKey;
use crate::app::repo_scan::Stopped;

/// The deepest a root may be scanned.
pub(in crate::app) const MAX_DEPTH: u8 = 4;
/// How many nearby folders are kept: the latest distinct ones.
pub(in crate::app) const MAX_NEARBY: usize = 5;
/// The most repositories the library keeps (and saves).
pub(in crate::app) const MAX_INDEX: usize = 5_000;

/// Where a repository was found.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub(in crate::app) enum Source {
    /// A configured root, by its id.
    Root(u64),
    /// The nearby folder (parent) of a repository that was opened.
    Nearby(PathBuf),
}

/// How a source's last scan ended.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(in crate::app) enum Outcome {
    /// It looked everywhere it was meant to.
    Complete,
    /// It stopped early (a limit, a stop): there may be more.
    Partial(Stopped),
    /// It couldn't look at all (the folder is gone, not allowed): the reason, raw.
    Failed(String),
}

/// A source's last scan.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(in crate::app) struct ScanRecord {
    /// When it ended, in seconds since 1970.
    pub at: i64,
    pub found: usize,
    pub outcome: Outcome,
}

/// A folder the user asked to be scanned for repositories.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(in crate::app) struct Root {
    pub id: u64,
    pub path: PathBuf,
    /// How many folders down: 0 is the root itself, 1 its children (the default).
    pub depth: u8,
    pub enabled: bool,
    pub last: Option<ScanRecord>,
}

impl Default for Root {
    fn default() -> Self {
        Self {
            id: 0,
            path: PathBuf::new(),
            depth: 1,
            enabled: true,
            last: None,
        }
    }
}

/// A nearby folder: the parent of a repository opened, its children looked at.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(in crate::app) struct Nearby {
    pub parent: PathBuf,
    pub last: Option<ScanRecord>,
}

/// A repository the library found.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(in crate::app) struct Entry {
    pub path: PathBuf,
    /// Every source that found it.
    pub sources: BTreeSet<Source>,
    /// The sources whose last scan didn't get to confirm it (stopped early or failed).
    #[serde(default)]
    pub unconfirmed: BTreeSet<Source>,
}

impl Entry {
    /// No source's last scan saw it: it may not be there any more.
    pub fn stale(&self) -> bool {
        !self.sources.is_empty() && self.sources.iter().all(|s| self.unconfirmed.contains(s))
    }
}

/// A profile's library, as saved.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(in crate::app) struct Library {
    pub roots: Vec<Root>,
    /// Look in the folder of each repository opened for its siblings.
    pub nearby_enabled: bool,
    /// The latest nearby folders first.
    pub nearby: Vec<Nearby>,
    /// Paths hidden from the library (never from Recent, nor from opening).
    pub hidden: BTreeSet<PathBuf>,
    pub entries: Vec<Entry>,
}

impl Default for Library {
    fn default() -> Self {
        Self {
            roots: Vec::new(),
            nearby_enabled: true,
            nearby: Vec::new(),
            hidden: BTreeSet::new(),
            entries: Vec::new(),
        }
    }
}

/// Every profile's library, by profile id.
pub(in crate::app) type Libraries = BTreeMap<u64, Library>;

/// The repositories a scan found (canonical paths), and how it ended.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::app) struct ScanResult {
    pub repos: Vec<PathBuf>,
    pub outcome: Outcome,
    pub at: i64,
}

impl Library {
    pub fn root(&self, id: u64) -> Option<&Root> {
        self.roots.iter().find(|r| r.id == id)
    }

    pub fn root_mut(&mut self, id: u64) -> Option<&mut Root> {
        self.roots.iter_mut().find(|r| r.id == id)
    }

    /// The root other than `except` whose folder `path` is.
    pub fn root_at(&self, path: &Path, except: Option<u64>) -> Option<u64> {
        let key = PathKey::of(path);
        self.roots
            .iter()
            .find(|r| Some(r.id) != except && PathKey::of(&r.path) == key)
            .map(|r| r.id)
    }

    /// Adds `path` as a root (or returns the one it already is, scanned at least `depth`
    /// deep from now on). Returns its id.
    pub fn add_root(&mut self, path: PathBuf, depth: u8) -> u64 {
        if let Some(id) = self.root_at(&path, None) {
            if let Some(root) = self.root_mut(id) {
                root.depth = root.depth.max(depth.min(MAX_DEPTH));
            }
            return id;
        }
        let id = self.roots.iter().map(|r| r.id).max().unwrap_or(0) + 1;
        self.roots.push(Root {
            id,
            path,
            depth: depth.min(MAX_DEPTH),
            ..Root::default()
        });
        id
    }

    /// Takes away everything `source` found; what another source found too stays.
    pub fn forget_source(&mut self, source: &Source) {
        for entry in &mut self.entries {
            entry.sources.remove(source);
            entry.unconfirmed.remove(source);
        }
        self.entries.retain(|e| !e.sources.is_empty());
    }

    pub fn remove_root(&mut self, id: u64) {
        self.roots.retain(|r| r.id != id);
        self.forget_source(&Source::Root(id));
    }

    /// Turns a root on or off; off, what only it found leaves the library.
    pub fn set_enabled(&mut self, id: u64, enabled: bool) {
        if let Some(root) = self.root_mut(id) {
            root.enabled = enabled;
        }
        if !enabled {
            self.forget_source(&Source::Root(id));
        }
    }

    /// Puts `parent` first among the nearby folders; one pushed out of the latest
    /// [`MAX_NEARBY`] takes what only it found away. Returns those pushed out.
    pub fn note_nearby(&mut self, parent: PathBuf) -> Vec<PathBuf> {
        let item = match self.nearby_at(&parent) {
            Some(i) => self.nearby.remove(i),
            None => Nearby { parent, last: None },
        };
        self.nearby.insert(0, item);
        let gone: Vec<PathBuf> = self
            .nearby
            .drain(MAX_NEARBY.min(self.nearby.len())..)
            .map(|n| n.parent)
            .collect();
        for parent in &gone {
            self.forget_source(&Source::Nearby(parent.clone()));
        }
        gone
    }

    /// Stops looking near repositories; what only nearby folders found leaves the library.
    pub fn set_nearby_enabled(&mut self, on: bool) {
        self.nearby_enabled = on;
        if !on {
            for parent in std::mem::take(&mut self.nearby) {
                self.forget_source(&Source::Nearby(parent.parent));
            }
        }
    }

    pub fn record_mut(&mut self, source: &Source) -> Option<&mut Option<ScanRecord>> {
        match source {
            Source::Root(id) => self.root_mut(*id).map(|r| &mut r.last),
            Source::Nearby(parent) => {
                let at = self.nearby_at(parent)?;
                Some(&mut self.nearby[at].last)
            }
        }
    }

    /// Where `parent` is among the nearby folders.
    pub fn nearby_at(&self, parent: &Path) -> Option<usize> {
        let key = PathKey::of(parent);
        self.nearby
            .iter()
            .position(|n| PathKey::of(&n.parent) == key)
    }

    /// Whether `source` is still one the library has (a root not removed or turned off, a
    /// nearby folder still among the latest).
    pub fn has_source(&self, source: &Source) -> bool {
        match source {
            Source::Root(id) => self.root(*id).is_some_and(|r| r.enabled),
            Source::Nearby(parent) => self.nearby_enabled && self.nearby_at(parent).is_some(),
        }
    }

    /// Adds what a scan of `source` found so far, merged by path with what other sources
    /// found. Returns how many were new to the library, and how many weren't added because it
    /// holds [`MAX_INDEX`] already.
    pub fn add_found(&mut self, source: &Source, repos: &[PathBuf]) -> (usize, usize) {
        let mut index: BTreeMap<PathKey, usize> = self
            .entries
            .iter()
            .enumerate()
            .map(|(i, e)| (PathKey::of(&e.path), i))
            .collect();
        let (mut new, mut left_out) = (0, 0);
        for repo in repos {
            let key = PathKey::of(repo);
            if let Some(&i) = index.get(&key) {
                let entry = &mut self.entries[i];
                entry.sources.insert(source.clone());
                entry.unconfirmed.remove(source);
            } else if self.entries.len() < MAX_INDEX {
                index.insert(key, self.entries.len());
                self.entries.push(Entry {
                    path: repo.clone(),
                    sources: BTreeSet::from([source.clone()]),
                    unconfirmed: BTreeSet::new(),
                });
                new += 1;
            } else {
                left_out += 1;
            }
        }
        (new, left_out)
    }

    /// Settles `source`'s scan: what it found is added; what it found before and not now
    /// leaves the library when the scan was complete (unless another source has it), and is
    /// kept as stale when the scan wasn't. Its record says how it went: partial when the
    /// library was too full to take all it found.
    pub fn finish(&mut self, source: &Source, result: ScanResult) {
        let seen: BTreeSet<PathKey> = result.repos.iter().map(|p| PathKey::of(p)).collect();
        let complete = result.outcome == Outcome::Complete;
        for entry in &mut self.entries {
            if !entry.sources.contains(source) || seen.contains(&PathKey::of(&entry.path)) {
                continue;
            }
            if complete {
                entry.sources.remove(source);
                entry.unconfirmed.remove(source);
            } else {
                entry.unconfirmed.insert(source.clone());
            }
        }
        self.entries.retain(|e| !e.sources.is_empty());
        // After what is gone made room.
        let (_, left_out) = self.add_found(source, &result.repos);
        let outcome = match result.outcome {
            Outcome::Complete if left_out > 0 => Outcome::Partial(Stopped::Full),
            outcome => outcome,
        };
        if let Some(record) = self.record_mut(source) {
            *record = Some(ScanRecord {
                at: result.at,
                found: result.repos.len(),
                outcome,
            });
        }
    }

    /// Forgets the repository at `path` (gone from the disk), whatever found it. Returns
    /// whether the library had it.
    pub fn drop_entry(&mut self, path: &Path) -> bool {
        let key = PathKey::of(path);
        let before = self.entries.len();
        self.entries.retain(|e| PathKey::of(&e.path) != key);
        self.entries.len() != before
    }

    /// Hides `path` from the library for good (it stays in Recent, and can be opened).
    pub fn hide(&mut self, path: &Path) {
        self.hidden.insert(path.to_owned());
    }

    #[cfg(test)]
    pub fn is_hidden(&self, path: &Path) -> bool {
        let key = PathKey::of(path);
        self.hidden.iter().any(|h| PathKey::of(h) == key)
    }

    /// How many repositories the library has that `source` found.
    pub fn count(&self, source: &Source) -> usize {
        self.entries
            .iter()
            .filter(|e| e.sources.contains(source))
            .count()
    }
}

#[cfg(test)]
#[path = "model_tests.rs"]
mod tests;
