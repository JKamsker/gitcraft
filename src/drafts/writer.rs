//! Writes the drafts on a thread of its own, so the window never waits for the disk, and keeps
//! what was last given for each repository, which is what a tab opened again starts from (its
//! file may not be written yet).

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};
use std::time::Duration;

use super::{RepoDrafts, limits, store};

enum Job {
    Save(PathBuf, RepoDrafts),
    /// Answered once everything asked before it is written.
    Flush(Sender<()>),
}

type Latest = Arc<Mutex<HashMap<PathBuf, RepoDrafts>>>;

/// The drafts of every repository, as this run knows them.
pub(super) struct Writer {
    /// The drafts folder; `None` when there is no data folder (nothing is kept then).
    folder: Option<PathBuf>,
    /// What was last given for each repository, or what its tab started with. A drafts' file
    /// that couldn't be read (or is another version's) is so left alone until something new is
    /// typed, rather than replaced by the empty drafts the tab started with. Shared with the
    /// thread, which drops an entry whose write failed, so the next save tries again.
    latest: Latest,
    jobs: Mutex<Option<Sender<Job>>>,
    /// What the window is to say next: a failure, or that drafts were cut down.
    notice: Arc<Mutex<Option<Notice>>>,
}

/// The one writer of this run.
pub(super) fn shared() -> &'static Writer {
    static WRITER: OnceLock<Writer> = OnceLock::new();
    WRITER.get_or_init(|| Writer::new(super::data_dir().map(|d| store::folder(&d))))
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    // A thread that panicked holding it left plain data, still usable.
    mutex.lock().unwrap_or_else(|e| e.into_inner())
}

impl Writer {
    pub(super) fn new(folder: Option<PathBuf>) -> Self {
        if let Some(folder) = folder.clone() {
            std::thread::spawn(move || store::tidy(&folder));
        }
        Self {
            folder,
            latest: Arc::default(),
            jobs: Mutex::new(None),
            notice: Arc::default(),
        }
    }

    pub(super) fn load(&self, repo: &Path) -> Result<RepoDrafts, String> {
        if let Some(known) = lock(&self.latest).get(repo) {
            return Ok(known.clone());
        }
        let Some(folder) = &self.folder else {
            return Ok(RepoDrafts::default());
        };
        let read = store::read(&store::file_of(folder, repo), repo);
        let start = read.clone().unwrap_or_default();
        lock(&self.latest).insert(repo.to_owned(), start);
        read
    }

    /// Writes `drafts` as `repo`'s in the background, unless they are what was last written.
    pub(super) fn save(&self, repo: &Path, drafts: RepoDrafts) {
        if self.folder.is_none() {
            return;
        }
        {
            let mut latest = lock(&self.latest);
            if latest.get(repo) == Some(&drafts) {
                return;
            }
            latest.insert(repo.to_owned(), drafts.clone());
        }
        self.send(Job::Save(repo.to_owned(), drafts));
    }

    pub(super) fn take_notice(&self) -> Option<Notice> {
        lock(&self.notice).take()
    }

    pub(super) fn flush(&self, timeout: Duration) {
        let (tx, rx) = channel();
        if self.send(Job::Flush(tx)) {
            let _ = rx.recv_timeout(timeout);
        }
    }

    /// Hands `job` to the thread, started on first use. Whether it took it.
    fn send(&self, job: Job) -> bool {
        let Some(folder) = self.folder.clone() else {
            return false;
        };
        let mut jobs = lock(&self.jobs);
        let tx = jobs.get_or_insert_with(|| {
            let (tx, rx) = channel();
            let (notice, latest) = (Arc::clone(&self.notice), Arc::clone(&self.latest));
            std::thread::spawn(move || run(&folder, &rx, &notice, &latest));
            tx
        });
        tx.send(job).is_ok()
    }
}

/// The writer's thread: takes the jobs as they come, the latest drafts of each repository
/// only when several queued up.
///
/// Each failure is said once, not at every save that fails the same way (a full disk would
/// otherwise say so every few seconds); a repository that saves again is said of afresh.
fn run(folder: &Path, jobs: &Receiver<Job>, notice: &Mutex<Option<Notice>>, latest: &Latest) {
    let mut cut_said = HashSet::new();
    let mut failing: HashMap<PathBuf, String> = HashMap::new();
    while let Ok(first) = jobs.recv() {
        let mut saves = BTreeMap::new();
        let mut flushes = Vec::new();
        for job in std::iter::once(first).chain(jobs.try_iter()) {
            match job {
                Job::Save(repo, drafts) => {
                    saves.insert(repo, drafts);
                }
                Job::Flush(done) => flushes.push(done),
            }
        }
        for (repo, drafts) in saves {
            let said = match write(folder, &repo, drafts.clone()) {
                Ok(cut) => {
                    failing.remove(&repo);
                    (cut && cut_said.insert(repo.clone())).then(|| Notice {
                        text: cut_text(&repo),
                        error: false,
                    })
                }
                Err(e) => {
                    // Not written: the same drafts given again are tried again.
                    let mut latest = lock(latest);
                    if latest.get(&repo) == Some(&drafts) {
                        latest.remove(&repo);
                    }
                    drop(latest);
                    (failing.get(&repo) != Some(&e)).then(|| {
                        failing.insert(repo.clone(), e.clone());
                        Notice {
                            text: e,
                            error: true,
                        }
                    })
                }
            };
            if let Some(said) = said {
                *lock(notice) = Some(said);
            }
        }
        for done in flushes {
            let _ = done.send(());
        }
    }
}

/// Something for the window to say about saving drafts.
#[derive(Debug, PartialEq)]
pub(crate) struct Notice {
    pub text: String,
    /// Saving failed; otherwise the drafts were saved, cut down.
    pub error: bool,
}

/// The repository's name, as messages give it.
fn name_of(repo: &Path) -> String {
    repo.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| repo.display().to_string())
}

/// What is said when `repo`'s drafts had to be cut down to be kept.
pub(super) fn cut_text(repo: &Path) -> String {
    format!(
        "Some of what you typed in {} is too long to keep in case gitcraft closes: not all of it \
         would come back after a crash",
        name_of(repo)
    )
}

/// Writes `repo`'s drafts, or removes its file when there are none. Whether they had to be cut
/// down to be kept.
pub(super) fn write(folder: &Path, repo: &Path, drafts: RepoDrafts) -> Result<bool, String> {
    let file = store::file_of(folder, repo);
    let name = name_of(repo);
    let failed = |e: String| format!("Couldn't save what you typed in {name}: {e}");
    if drafts.is_empty() {
        return store::remove(&file).map(|()| false).map_err(failed);
    }
    let (bytes, cut) = limits::encode(repo, drafts).map_err(failed)?;
    store::write_atomic(&file, &bytes).map_err(failed)?;
    Ok(cut)
}
