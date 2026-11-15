//! The file-system checks the library needs before it acts, run on a thread of their own and
//! never on the window's: whether an opened repository's folder is fit to look in (against
//! the canonical home and temporary folders, and not where a drive is mounted), and whether a
//! row that didn't open is gone from the disk.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, OnceLock};

use super::identity;

/// The folders a nearby folder is never in (temporary ones) or above (home), each as written
/// and canonical: a repository's path is canonical (`/private/var/folders/…` on macOS)
/// where `$TMPDIR` and `$HOME` may not be.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(in crate::app) struct Guards {
    pub homes: Vec<PathBuf>,
    pub temps: Vec<PathBuf>,
}

impl Guards {
    /// Those of this machine. Touches the file system: only on the checks' thread.
    pub fn current() -> Self {
        let mut temps = vec![std::env::temp_dir()];
        if cfg!(unix) {
            temps.extend(["/tmp", "/var/tmp"].map(PathBuf::from));
        }
        let home = crate::app::clone::home_dir();
        Self::canonical(home.into_iter().collect(), temps)
    }

    /// `homes` and `temps`, each also canonical.
    pub fn canonical(homes: Vec<PathBuf>, temps: Vec<PathBuf>) -> Self {
        let both = |paths: Vec<PathBuf>| {
            let mut all = Vec::new();
            for path in paths {
                if let Ok(real) = std::fs::canonicalize(&path) {
                    all.push(crate::app::strip_verbatim(real));
                }
                all.push(path);
            }
            all.dedup();
            all
        };
        Self {
            homes: both(homes),
            temps: both(temps),
        }
    }
}

/// The folder to look in near the repository `root` (canonical) opened, if it is fit to.
pub(in crate::app) fn vet(root: &Path, guards: &Guards) -> Option<PathBuf> {
    identity::nearby_parent(root, &guards.homes, &guards.temps).filter(|p| !is_mount(p))
}

/// Whether `dir` is where a drive is mounted (it is on another device than its parent),
/// or can't be looked at. A drive's root is never looked in uninvited.
#[cfg(unix)]
pub(in crate::app) fn is_mount(dir: &Path) -> bool {
    use std::os::unix::fs::MetadataExt;
    let own = std::fs::metadata(dir);
    let up = dir.parent().map(std::fs::metadata);
    match (own, up) {
        (Ok(own), Some(Ok(up))) => own.dev() != up.dev(),
        _ => true,
    }
}

#[cfg(not(unix))]
pub(in crate::app) fn is_mount(dir: &Path) -> bool {
    std::fs::metadata(dir).is_err()
}

/// What a check found.
pub(in crate::app) enum Answer {
    /// The folder to look in near a repository opened in `profile`, if any.
    Nearby {
        profile: u64,
        parent: Option<PathBuf>,
        key: (u64, identity::PathKey),
        token: u64,
    },
    /// `path`, a row of `profile`'s that didn't open, is gone from the disk.
    Gone { profile: u64, path: PathBuf },
    /// It is there (it didn't open for another reason).
    There,
}

pub(in crate::app) struct Checks {
    tx: Sender<(u64, Answer)>,
    rx: Receiver<(u64, Answer)>,
    worker: Option<Sender<Box<dyn FnOnce() + Send>>>,
    epoch: u64,
    next_token: u64,
    nearby: BTreeMap<(u64, identity::PathKey), u64>,
    guards: Arc<OnceLock<Guards>>,
    /// Checks asked for and not answered yet.
    pending: usize,
    repaint: Option<egui::Context>,
}

impl Default for Checks {
    fn default() -> Self {
        let (tx, rx) = channel();
        Self {
            tx,
            rx,
            guards: Arc::default(),
            worker: None,
            epoch: 0,
            next_token: 0,
            nearby: BTreeMap::new(),
            pending: 0,
            repaint: None,
        }
    }
}

impl Checks {
    pub fn set_repaint(&mut self, ctx: &egui::Context) {
        if self.repaint.is_none() {
            self.repaint = Some(ctx.clone());
        }
    }

    /// Checks with `guards` rather than this machine's (before any check ran).
    #[cfg(test)]
    pub fn set_guards(&mut self, guards: Guards) {
        let _ = self.guards.set(guards);
    }

    #[cfg(test)]
    pub fn busy(&self) -> bool {
        self.pending > 0
    }

    /// Finds the folder to look in near `root`, opened in `profile`.
    pub fn nearby(&mut self, profile: u64, root: PathBuf) {
        let Some(parent) = root.parent() else {
            return;
        };
        let key = (profile, identity::PathKey::of(parent));
        self.next_token += 1;
        let token = self.next_token;
        self.nearby.insert(key.clone(), token);
        let guards = self.guards.clone();
        self.spawn(move || {
            let parent = vet(&root, guards.get_or_init(Guards::current));
            Answer::Nearby {
                profile,
                parent,
                key,
                token,
            }
        });
    }

    /// Finds whether `path`, a row of `profile`'s that didn't open, is gone.
    pub fn gone(&mut self, profile: u64, path: PathBuf) {
        self.spawn(move || match std::fs::symlink_metadata(&path) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Answer::Gone { profile, path },
            _ => Answer::There,
        });
    }

    fn spawn(&mut self, check: impl FnOnce() -> Answer + Send + 'static) {
        // One worker per window: a slow filesystem never causes replacement threads.
        if self.worker.is_none() {
            let (tx, rx) = channel::<Box<dyn FnOnce() + Send>>();
            if std::thread::Builder::new()
                .name("library-check".into())
                .spawn(move || {
                    for check in rx {
                        check();
                    }
                })
                .is_err()
            {
                return;
            }
            self.worker = Some(tx);
        }
        let (tx, ctx, epoch) = (self.tx.clone(), self.repaint.clone(), self.epoch);
        let job = Box::new(move || {
            let _ = tx.send((epoch, check()));
            if let Some(ctx) = ctx {
                ctx.request_repaint();
            }
        });
        if self.worker.as_ref().is_some_and(|w| w.send(job).is_ok()) {
            self.pending += 1;
        }
    }

    /// Discards checks requested before a profile change, even after switching back.
    pub fn retire_all(&mut self) {
        self.epoch = self.epoch.wrapping_add(1);
        self.nearby.clear();
    }

    /// A forgotten folder's queued checks can never recreate its source.
    pub fn retire_nearby(&mut self, profile: u64, parent: &Path) {
        self.nearby
            .remove(&(profile, identity::PathKey::of(parent)));
    }

    /// The answers in so far.
    pub fn poll(&mut self) -> Vec<Answer> {
        let answers: Vec<_> = self.rx.try_iter().collect();
        self.pending = self.pending.saturating_sub(answers.len());
        answers
            .into_iter()
            .filter_map(|(epoch, answer)| {
                if epoch != self.epoch {
                    return None;
                }
                if let Answer::Nearby { key, token, .. } = &answer {
                    if self.nearby.get(key) != Some(token) {
                        return None;
                    }
                    self.nearby.remove(key);
                }
                Some(answer)
            })
            .collect()
    }
}
