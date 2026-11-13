//! The bounded filesystem walk shared by _Scan a folder for repositories_ and the repository
//! library (configured roots and nearby folders): one walker, one skip policy, one set of
//! limits. It only reads folder listings and `.git` entries: no git process, no network.

use std::collections::BinaryHeap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

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

/// How often cancellation and the time budget are looked at while a listing is read.
const CHECK_EVERY: usize = 256;

/// How far a scan goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::app) struct ScanLimits {
    /// How many folders down from the one picked (its children are 1; 0 is only itself).
    pub depth: usize,
    /// The most repositories listed.
    pub max_repos: usize,
    /// The most folders looked in (and kept in memory to look in).
    pub max_dirs: usize,
    /// The most listing entries (files and folders) read in all: a folder of a million
    /// files stops the walk rather than being read to its end.
    pub max_entries: usize,
    /// How long the walk may take.
    pub budget: Option<Duration>,
}

/// The limits the start page's _Scan a folder_ scans with.
pub(in crate::app) const LIMITS: ScanLimits = ScanLimits {
    depth: 4,
    max_repos: 200,
    max_dirs: 50_000,
    max_entries: 1_000_000,
    budget: None,
};

/// The limits a configured library root is scanned with (its depth is its own).
pub(in crate::app) const ROOT_LIMITS: ScanLimits = ScanLimits {
    depth: 1,
    max_repos: 1_000,
    max_dirs: 5_000,
    max_entries: 200_000,
    budget: Some(Duration::from_secs(30)),
};

/// The limits a nearby folder (the parent of an opened repository) is scanned with.
pub(in crate::app) const NEARBY_LIMITS: ScanLimits = ScanLimits {
    depth: 1,
    max_repos: 128,
    max_dirs: 256,
    max_entries: 4_096,
    budget: Some(Duration::from_secs(5)),
};

/// Why a scan ended before it looked everywhere.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(in crate::app) enum Stopped {
    /// It found [`ScanLimits::max_repos`].
    Repos,
    /// It looked in [`ScanLimits::max_dirs`] folders.
    Folders,
    /// It read [`ScanLimits::max_entries`] listing entries.
    Entries,
    /// It took [`ScanLimits::budget`].
    Time,
    /// It was asked to stop.
    Asked,
}

/// What a scan found.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(in crate::app) struct Found {
    /// The repositories' folders, sorted.
    pub repos: Vec<PathBuf>,
    /// It stopped before it looked everywhere: there may be more.
    pub stopped: Option<Stopped>,
}

/// How a scan is going, now and then.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(in crate::app) struct Progress {
    pub folders: usize,
    pub repos: usize,
}

/// What a walk tells as it goes.
pub(in crate::app) enum ScanEvent<'a> {
    Progress(Progress),
    /// A repository found (also listed in [`Found::repos`] at the end).
    Repo(&'a Path),
}

/// Whether the folder `name` is looked in.
pub(in crate::app) fn walked(name: &str) -> bool {
    !name.starts_with('.') && !SKIPPED.contains(&name)
}

/// Whether `dir` is a repository's top folder, checked cheaply: a `.git` folder with a `HEAD`,
/// or the `.git` file of a linked worktree or submodule naming a folder that is there. A
/// malformed or unreadable one is not (opening it is what checks it with git).
pub(in crate::app) fn is_repo(dir: &Path) -> bool {
    let git = dir.join(".git");
    match std::fs::metadata(&git) {
        Ok(m) if m.is_dir() => git.join("HEAD").is_file(),
        Ok(m) if m.is_file() => gitfile_target(dir, &git).is_some_and(|t| t.is_dir()),
        _ => false,
    }
}

/// The folder a `.git` file names (`gitdir: <path>`), relative ones from `dir`. Reads at
/// most 4 KiB of it.
pub(in crate::app) fn gitfile_target(dir: &Path, file: &Path) -> Option<PathBuf> {
    let mut text = String::new();
    std::fs::File::open(file)
        .ok()?
        .take(4096)
        .read_to_string(&mut text)
        .ok()?;
    let target = text.lines().next()?.strip_prefix("gitdir:")?.trim();
    (!target.is_empty()).then(|| dir.join(target))
}

/// Why `dir` couldn't be looked in, in words.
pub(in crate::app) fn unreadable(dir: &Path, e: &std::io::Error) -> String {
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
pub(in crate::app) fn scan(
    root: &Path,
    limits: ScanLimits,
    cancel: &AtomicBool,
    progress: &mut dyn FnMut(Progress),
) -> Result<Found, String> {
    walk(root, limits, cancel, &mut |event| {
        if let ScanEvent::Progress(p) = event {
            progress(p);
        }
    })
}

/// The walk under [`scan`], telling each repository as it is found too.
pub(in crate::app) fn walk(
    root: &Path,
    limits: ScanLimits,
    cancel: &AtomicBool,
    on: &mut dyn FnMut(ScanEvent),
) -> Result<Found, String> {
    std::fs::read_dir(root).map_err(|e| unreadable(root, &e))?;
    let mut walk = Walk {
        limits,
        cancel,
        started: Instant::now(),
        entries: 0,
        seen: 0,
    };
    let mut found = Found::default();
    found.stopped = walk.levels(root, &mut found.repos, on).err();
    on(ScanEvent::Progress(Progress {
        folders: walk.seen,
        repos: found.repos.len(),
    }));
    found.repos.sort();
    Ok(found)
}

struct Walk<'a> {
    limits: ScanLimits,
    cancel: &'a AtomicBool,
    started: Instant,
    /// Listing entries read so far.
    entries: usize,
    /// Folders looked at so far.
    seen: usize,
}

impl Walk<'_> {
    /// Why it has to stop now, if it does.
    fn interrupted(&self) -> Result<(), Stopped> {
        if self.cancel.load(Ordering::Relaxed) {
            return Err(Stopped::Asked);
        }
        match self.limits.budget {
            Some(budget) if self.started.elapsed() >= budget => Err(Stopped::Time),
            _ => Ok(()),
        }
    }

    /// Level by level from `root`; `Err` says why it stopped early.
    fn levels(
        &mut self,
        root: &Path,
        repos: &mut Vec<PathBuf>,
        on: &mut dyn FnMut(ScanEvent),
    ) -> Result<(), Stopped> {
        let limits = self.limits;
        // Each folder, and whether to look in it (not when it is only checked for being one).
        let mut level = vec![(root.to_owned(), true)];
        let mut told = Instant::now();
        let mut cut = None;
        for depth in 0..=limits.depth {
            let mut next = Vec::new();
            for (dir, look_in) in level {
                self.interrupted()?;
                self.seen += 1;
                let seen = self.seen;
                if told.elapsed() >= Duration::from_millis(100) {
                    told = Instant::now();
                    on(ScanEvent::Progress(Progress {
                        folders: seen,
                        repos: repos.len(),
                    }));
                }
                if is_repo(&dir) {
                    on(ScanEvent::Repo(&dir));
                    repos.push(dir.clone());
                    if repos.len() >= limits.max_repos {
                        return Err(Stopped::Repos);
                    }
                }
                if seen >= limits.max_dirs {
                    return Err(Stopped::Folders);
                }
                if depth == limits.depth || !look_in {
                    continue;
                }
                // Never more kept than will be looked in.
                let room = limits.max_dirs.saturating_sub(seen + next.len());
                let (children, more) = self.children(&dir, room)?;
                if more {
                    cut = Some(Stopped::Folders);
                }
                next.extend(children);
            }
            level = next;
        }
        cut.map_or(Ok(()), Err)
    }

    /// The first `room` folders in `dir` in name order (whatever order the file system lists
    /// them in: which ones are kept when the limit cuts in is the same everywhere), and whether
    /// there were more. Only `room` are ever held while the listing is read.
    fn children(
        &mut self,
        dir: &Path,
        room: usize,
    ) -> Result<(Vec<(PathBuf, bool)>, bool), Stopped> {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return Ok((Vec::new(), false));
        };
        let mut kept: BinaryHeap<PathBuf> = BinaryHeap::new();
        let mut more = false;
        for entry in entries.flatten() {
            self.entries += 1;
            if self.entries >= self.limits.max_entries {
                return Err(Stopped::Entries);
            }
            if self.entries.is_multiple_of(CHECK_EVERY) {
                self.interrupted()?;
            }
            // A link's own type: never followed (nor a Windows junction).
            if !entry.file_type().is_ok_and(|t| t.is_dir()) || entry.file_name() == ".git" {
                continue;
            }
            kept.push(entry.path());
            if kept.len() > room {
                more = true;
                kept.pop();
            }
        }
        let children = kept
            .into_sorted_vec()
            .into_iter()
            .map(|path| {
                let walk = path
                    .file_name()
                    .is_some_and(|n| walked(&n.to_string_lossy()));
                (path, walk)
            })
            .collect();
        Ok((children, more))
    }
}
