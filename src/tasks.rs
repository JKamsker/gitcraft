//! Bounded workers for existing blocking APIs. Process I/O lives on `processes`' event loop.
//! Submission never waits for a worker, including when called from egui.

use std::cell::RefCell;
use std::collections::{HashSet, VecDeque};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, OnceLock};

pub(crate) const WORKERS: usize = 12;
pub(crate) const QUEUE_CAPACITY: usize = 512;
pub(crate) const BUSY: &str = "Background queue is full; try again when current work finishes";

#[derive(Clone, Default)]
pub(crate) struct Cancellation(Arc<AtomicBool>, Option<Arc<Cancellation>>);

impl Cancellation {
    pub(crate) fn cancel(&self) {
        self.0.store(true, Ordering::Release);
    }
    pub(crate) fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Acquire)
            || self.1.as_ref().is_some_and(|parent| parent.is_cancelled())
    }
    pub(crate) fn child(&self) -> Self {
        Self(Arc::default(), Some(Arc::new(self.clone())))
    }
}

/// Only the tab owns this guard; jobs clone its token, never the guard.
#[derive(Default)]
pub(crate) struct Scope(pub(crate) Cancellation);
impl Drop for Scope {
    fn drop(&mut self) {
        self.0.cancel();
    }
}

thread_local! {
    static CURRENT: RefCell<Option<Cancellation>> = const { RefCell::new(None) };
    static URGENT: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    static SPECULATIVE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

pub(crate) fn cancellation() -> Cancellation {
    CURRENT.with(|slot| slot.borrow().clone().unwrap_or_default())
}

/// Whether this thread runs work the user waits on ([`Priority::Urgent`]): its processes go
/// ahead of the others (see `processes`).
pub(crate) fn urgent() -> bool {
    URGENT.with(std::cell::Cell::get)
}

/// Whether this thread reads ahead ([`Priority::Speculative`]): its processes start only in
/// slots nothing else waits for (see `processes`).
pub(crate) fn speculative() -> bool {
    SPECULATIVE.with(std::cell::Cell::get)
}

/// What a job's own threads take over from it: its cancellation, its urgency, its connections.
pub(crate) struct Inherited {
    cancel: Cancellation,
    urgent: bool,
    speculative: bool,
    connections: crate::connections::Context,
}

impl Inherited {
    pub(crate) fn capture() -> Self {
        Self {
            cancel: cancellation(),
            urgent: urgent(),
            speculative: speculative(),
            connections: crate::connections::current(),
        }
    }

    pub(crate) fn run<T>(self, work: impl FnOnce() -> T) -> T {
        let was = URGENT.with(|u| u.replace(self.urgent));
        let was_speculative = SPECULATIVE.with(|s| s.replace(self.speculative));
        let out = self
            .connections
            .run(|| with_cancellation(self.cancel, work));
        URGENT.with(|u| u.set(was));
        SPECULATIVE.with(|s| s.set(was_speculative));
        out
    }
}

/// Bind an inherited lifetime in a nested worker, restoring the previous token on unwind.
pub(crate) fn with_cancellation<T>(cancel: Cancellation, work: impl FnOnce() -> T) -> T {
    struct Restore(Option<Cancellation>);
    impl Drop for Restore {
        fn drop(&mut self) {
            CURRENT.with(|slot| *slot.borrow_mut() = self.0.take());
        }
    }
    let _restore = Restore(CURRENT.with(|slot| slot.replace(Some(cancel))));
    work()
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Priority {
    /// What the user just asked to see (the commit clicked): started at once on a thread of
    /// its own, not queued behind the others, and its processes go first.
    Urgent,
    Interactive,
    Background,
    /// Read ahead, before anyone asked (the files of the commits on screen): taken only when
    /// nothing else waits, on at most [`SPECULATIVE_LIMIT`] workers.
    Speculative,
}

/// How many workers may read ahead at once: the others stay free for what is asked for.
const SPECULATIVE_LIMIT: usize = 2;

/// How many urgent jobs may have threads of their own at once; more wait in the queue's front.
const URGENT_THREADS: usize = 4;
static URGENT_RUNNING: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// App-internal exclusion for complete mutation operations. Paths can be retargeted
/// after submission, so neither path equality nor a retained metadata handle proves
/// that independent local jobs will execute against independent repositories.
/// Conservatively serialize local mutations, and separately serialize remote ones.
/// This trades unrelated-local concurrency for safety. Reads remain unkeyed; this
/// does not anchor Git's target or coordinate external Git/application instances.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) enum MutationScheduleKey {
    Local,
    Remote,
}

pub(crate) fn mutation_schedule_key(path: PathBuf) -> MutationScheduleKey {
    if crate::remote::is_remote(&path) {
        MutationScheduleKey::Remote
    } else {
        MutationScheduleKey::Local
    }
}

type Work = Box<dyn FnOnce() + Send>;
mod pool;
pub(crate) use pool::*;

/// Reload leaf reads have a separate fixed pool: an outer reload may join them without
/// consuming the workers needed to run them. No leaf may recursively submit leaf work.
static READ_POOL: OnceLock<Result<Pool, String>> = OnceLock::new();
pub(crate) fn read<T: Send + 'static>(
    work: impl FnOnce() -> T + Send + 'static,
) -> Result<Read<T>, String> {
    let pool = READ_POOL
        .get_or_init(|| Pool::with_capacity(8, 64).map_err(|e| e.to_string()))
        .as_ref()
        .map_err(Clone::clone)?;
    let (tx, rx) = std::sync::mpsc::channel();
    let cancel = cancellation().child();
    pool.submit(
        cancel.clone(),
        Priority::Interactive,
        None,
        Box::new(move || {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(work))
                .map_err(|_| "Reading the repository failed: a reader panicked".to_owned());
            let _ = tx.send(result);
        }),
    )
    .map_err(str::to_owned)?;
    Ok(Read {
        reply: Some(rx),
        cancel,
    })
}

pub(crate) struct Read<T> {
    reply: Option<std::sync::mpsc::Receiver<Result<T, String>>>,
    cancel: Cancellation,
}
impl<T> Read<T> {
    pub(crate) fn join(mut self) -> Result<T, String> {
        self.reply
            .take()
            .ok_or("reader already joined")?
            .recv()
            .map_err(|_| "cancelled".to_owned())?
    }
}
impl<T> Drop for Read<T> {
    fn drop(&mut self) {
        self.cancel.cancel();
        // A failed reload cannot release its loading state while sibling readers still
        // own process slots. Cancellation also removes queued readers without running them.
        if let Some(reply) = self.reply.take() {
            let _ = reply.recv();
        }
    }
}

static POOL: OnceLock<Result<Pool, String>> = OnceLock::new();
pub(crate) fn pool() -> Result<&'static Pool, String> {
    POOL.get_or_init(|| {
        Pool::with_limits(WORKERS, QUEUE_CAPACITY, WORKERS - 2)
            .map_err(|e| format!("Could not start background workers: {e}"))
    })
    .as_ref()
    .map_err(Clone::clone)
}

pub(crate) fn peak_queued() -> Option<usize> {
    POOL.get()
        .and_then(|p| p.as_ref().ok())
        .map(Pool::peak_queued)
}
pub(crate) fn counts() -> Option<(usize, usize)> {
    POOL.get().and_then(|s| s.as_ref().ok()).map(Pool::counts)
}
pub(crate) fn shutdown() {
    if let Some(Ok(pool)) = READ_POOL.get() {
        pool.shutdown();
    }
    if let Some(Ok(pool)) = POOL.get() {
        pool.shutdown();
    }
    if let Some(Ok(pool)) = POOL.get() {
        pool.join();
    }
    if let Some(Ok(pool)) = READ_POOL.get() {
        pool.join();
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic)]
// Poisoned state or an executed cancelled job must fail these scheduler tests.
mod tests;

#[cfg(test)]
mod r2_scope_tests;
