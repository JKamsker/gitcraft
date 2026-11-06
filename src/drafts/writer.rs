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

/// The drafts of every repository, as this run knows them.
pub(super) struct Writer {
    /// The drafts folder; `None` when there is no data folder (nothing is kept then).
    folder: Option<PathBuf>,
    /// What was last given for each repository.
    latest: Mutex<HashMap<PathBuf, RepoDrafts>>,
    jobs: Mutex<Option<Sender<Job>>>,
    /// The last thing that went wrong, for the window to say.
    error: Arc<Mutex<Option<String>>>,
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
            latest: Mutex::new(HashMap::new()),
            jobs: Mutex::new(None),
            error: Arc::default(),
        }
    }

    pub(super) fn load(&self, repo: &Path) -> Result<RepoDrafts, String> {
        if let Some(known) = lock(&self.latest).get(repo) {
            return Ok(known.clone());
        }
        let Some(folder) = &self.folder else {
            return Ok(RepoDrafts::default());
        };
        store::read(&store::file_of(folder, repo), repo)
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

    pub(super) fn take_error(&self) -> Option<String> {
        lock(&self.error).take()
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
            let error = Arc::clone(&self.error);
            std::thread::spawn(move || run(&folder, &rx, &error));
            tx
        });
        tx.send(job).is_ok()
    }
}

/// The writer's thread: takes the jobs as they come, the latest drafts of each repository
/// only when several queued up.
fn run(folder: &Path, jobs: &Receiver<Job>, error: &Mutex<Option<String>>) {
    let mut warned = HashSet::new();
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
            if let Err(e) = write(folder, &repo, drafts, &mut warned) {
                *lock(error) = Some(e);
            }
        }
        for done in flushes {
            let _ = done.send(());
        }
    }
}

/// Writes `repo`'s drafts, or removes its file when there are none. Says once per repository
/// when they had to be cut down to be kept.
pub(super) fn write(
    folder: &Path,
    repo: &Path,
    drafts: RepoDrafts,
    warned: &mut HashSet<PathBuf>,
) -> Result<(), String> {
    let file = store::file_of(folder, repo);
    let name = repo
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| repo.display().to_string());
    let failed = |e: String| format!("Couldn't save what you typed in {name}: {e}");
    if drafts.is_empty() {
        return store::remove(&file).map_err(failed);
    }
    let (bytes, cut) = limits::encode(repo, drafts).map_err(failed)?;
    store::write_atomic(&file, &bytes).map_err(failed)?;
    if cut && warned.insert(repo.to_owned()) {
        return Err(format!(
            "What you typed in {name} is too long to keep whole in case gitcraft closes: the \
             longest texts are kept in part"
        ));
    }
    Ok(())
}
