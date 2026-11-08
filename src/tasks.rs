//! Bounded workers for existing blocking APIs. Process I/O lives on `processes`' event loop.
//! Submission never waits for a worker, including when called from egui.

use std::cell::RefCell;
use std::collections::{HashSet, VecDeque};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, OnceLock};

pub(crate) const WORKERS: usize = 8;
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
}

pub(crate) fn cancellation() -> Cancellation {
    CURRENT.with(|slot| slot.borrow().clone().unwrap_or_default())
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
    Interactive,
    Background,
}

type Work = Box<dyn FnOnce() + Send>;
struct Job {
    work: Work,
    connections: crate::connections::Context,
    on_cancel: Option<Work>,
    cancel: Cancellation,
    priority: Priority,
    /// A whole mutation operation holds its key, including between subprocesses.
    key: Option<PathBuf>,
}

#[derive(Default)]
struct State {
    queue: VecDeque<Job>,
    writing: HashSet<PathBuf>,
    active: usize,
    background_active: usize,
    peak_queued: usize,
    stopped: bool,
    running: Vec<Cancellation>,
}

struct Shared {
    state: Mutex<State>,
    ready: Condvar,
    background_limit: usize,
}

pub(crate) struct Pool {
    shared: Arc<Shared>,
    capacity: usize,
    workers: Mutex<Vec<std::thread::JoinHandle<()>>>,
}

impl Pool {
    #[cfg(test)]
    fn new(workers: usize) -> std::io::Result<Self> {
        Self::with_capacity(workers, QUEUE_CAPACITY)
    }

    fn with_capacity(workers: usize, capacity: usize) -> std::io::Result<Self> {
        Self::with_limits(workers, capacity, workers)
    }

    fn with_limits(
        workers: usize,
        capacity: usize,
        background_limit: usize,
    ) -> std::io::Result<Self> {
        let shared = Arc::new(Shared {
            state: Mutex::new(State::default()),
            ready: Condvar::new(),
            background_limit,
        });
        let mut handles = Vec::new();
        for i in 0..workers {
            let state = Arc::clone(&shared);
            match std::thread::Builder::new()
                .name(format!("gitcraft-job-{i}"))
                .spawn(move || worker(state))
            {
                Ok(handle) => handles.push(handle),
                Err(e) => {
                    let mut locked = shared
                        .state
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner);
                    locked.stopped = true;
                    shared.ready.notify_all();
                    return Err(e);
                }
            }
        }
        Ok(Self {
            shared,
            capacity,
            workers: Mutex::new(handles),
        })
    }

    pub(crate) fn submit(
        &self,
        cancel: Cancellation,
        priority: Priority,
        key: Option<PathBuf>,
        work: Work,
    ) -> Result<(), &'static str> {
        self.submit_cancellable(cancel, priority, key, None, work)
    }

    pub(crate) fn submit_cancellable(
        &self,
        cancel: Cancellation,
        priority: Priority,
        key: Option<PathBuf>,
        on_cancel: Option<Work>,
        work: Work,
    ) -> Result<(), &'static str> {
        let mut state = self
            .shared
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if state.stopped {
            return Err("Background work has shut down");
        }
        // Drop cancelled queued closures promptly, releasing their captures and capacity.
        purge(&mut state);
        if state.queue.len() >= self.capacity {
            return Err(BUSY);
        }
        state.queue.push_back(Job {
            work,
            connections: crate::connections::current(),
            on_cancel,
            cancel,
            priority,
            key,
        });
        state.peak_queued = state.peak_queued.max(state.queue.len());
        self.shared.ready.notify_all();
        Ok(())
    }

    fn shutdown(&self) {
        let mut state = self
            .shared
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        state.stopped = true;
        for cancel in &state.running {
            cancel.cancel();
        }
        state.queue.clear();
        self.shared.ready.notify_all();
    }

    fn join(&self) {
        let workers = std::mem::take(
            &mut *self
                .workers
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner),
        );
        for worker in workers {
            let _ = worker.join();
        }
    }

    pub(crate) fn counts(&self) -> (usize, usize) {
        let state = self
            .shared
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        (state.active, state.queue.len())
    }
}

fn purge(state: &mut State) {
    state.queue.retain_mut(|job| {
        if !job.cancel.is_cancelled() {
            return true;
        }
        if let Some(done) = job.on_cancel.take() {
            let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(done));
        }
        false
    });
}

pub(crate) fn purge_cancelled() {
    if let Some(Ok(pool)) = POOL.get() {
        let mut state = pool
            .shared
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        purge(&mut state);
        pool.shared.ready.notify_all();
    }
}

fn worker(shared: Arc<Shared>) {
    loop {
        let job = {
            let mut state = shared
                .state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            loop {
                if state.stopped {
                    return;
                }
                purge(&mut state);
                // Oldest runnable work first so reads cannot starve mutations. The
                // background limit still reserves worker capacity for interactive reads.
                let mut seen = HashSet::new();
                let mut candidates = Vec::new();
                for (i, job) in state.queue.iter().enumerate() {
                    let runnable = job
                        .key
                        .as_ref()
                        .is_none_or(|key| seen.insert(key.clone()) && !state.writing.contains(key));
                    if runnable
                        && (job.priority == Priority::Interactive
                            || state.background_active < shared.background_limit)
                    {
                        candidates.push(i);
                    }
                }
                let selected = candidates.first().copied();
                if let Some(i) = selected
                    && let Some(job) = state.queue.remove(i)
                {
                    if let Some(key) = &job.key {
                        state.writing.insert(key.clone());
                    }
                    state.active += 1;
                    if job.priority == Priority::Background {
                        state.background_active += 1;
                    }
                    state.running.push(job.cancel.clone());
                    break job;
                }
                state = shared
                    .ready
                    .wait(state)
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
            }
        };
        CURRENT.with(|slot| *slot.borrow_mut() = Some(job.cancel.clone()));
        // Most callers supply an error continuation. Also contain panics from any other
        // caller so the fixed pool never silently loses a worker.
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            job.connections.run(job.work)
        }));
        CURRENT.with(|slot| *slot.borrow_mut() = None);
        let mut state = shared
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        state.active -= 1;
        if job.priority == Priority::Background {
            state.background_active -= 1;
        }
        if let Some(i) = state
            .running
            .iter()
            .position(|cancel| Arc::ptr_eq(&cancel.0, &job.cancel.0))
        {
            state.running.swap_remove(i);
        }
        if let Some(key) = job.key {
            state.writing.remove(&key);
        }
        shared.ready.notify_all();
    }
}

impl Drop for Pool {
    fn drop(&mut self) {
        self.shutdown();
        self.join();
    }
}

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
    POOL.get().and_then(|p| p.as_ref().ok()).map(|pool| {
        pool.shared
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .peak_queued
    })
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
mod tests {
    use super::*;
    use std::sync::mpsc;
    use std::time::Duration;

    fn submit(
        pool: &Pool,
        priority: Priority,
        key: Option<&str>,
        work: impl FnOnce() + Send + 'static,
    ) -> Result<(), String> {
        pool.submit(
            Cancellation::default(),
            priority,
            key.map(PathBuf::from),
            Box::new(work),
        )
        .map_err(str::to_owned)
    }

    #[test]
    fn bounds_workers_queue_and_rejects_without_waiting() -> Result<(), String> {
        let pool = Pool::new(2).map_err(|e| e.to_string())?;
        let (started, rx) = mpsc::channel();
        let mut gates = Vec::new();
        for _ in 0..2 {
            let (tx, gate) = mpsc::channel::<()>();
            gates.push(tx);
            let started = started.clone();
            submit(&pool, Priority::Background, None, move || {
                let _ = started.send(());
                let _ = gate.recv();
            })?;
        }
        for _ in 0..2 {
            rx.recv_timeout(Duration::from_secs(5))
                .map_err(|e| e.to_string())?;
        }
        for _ in 0..QUEUE_CAPACITY {
            submit(&pool, Priority::Background, None, || {})?;
        }
        assert_eq!(pool.counts(), (2, QUEUE_CAPACITY));
        assert_eq!(
            submit(&pool, Priority::Interactive, None, || {}),
            Err(BUSY.into())
        );
        pool.shutdown();
        drop(gates);
        Ok(())
    }

    #[test]
    fn reads_and_writes_both_make_progress_and_repository_writes_stay_fifo() -> Result<(), String> {
        let pool = Pool::new(1).map_err(|e| e.to_string())?;
        let (gate_tx, gate) = mpsc::channel::<()>();
        let (started, rx) = mpsc::channel();
        submit(&pool, Priority::Background, None, move || {
            let _ = started.send(());
            let _ = gate.recv();
        })?;
        rx.recv_timeout(Duration::from_secs(5))
            .map_err(|e| e.to_string())?;
        let (out, events) = mpsc::channel();
        for (label, priority, key) in [
            ("write 1", Priority::Background, Some("repo")),
            ("write 2", Priority::Interactive, Some("repo")),
            ("read", Priority::Interactive, None),
            ("other repo", Priority::Background, Some("other")),
        ] {
            let out = out.clone();
            submit(&pool, priority, key, move || {
                let _ = out.send(label);
            })?;
        }
        let _ = gate_tx.send(());
        let mut result = Vec::new();
        for _ in 0..4 {
            result.push(
                events
                    .recv_timeout(Duration::from_secs(5))
                    .map_err(|e| e.to_string())?,
            );
        }
        assert_eq!(result, ["write 1", "write 2", "read", "other repo"]);
        pool.shutdown();
        Ok(())
    }

    #[test]
    fn cancelled_queued_jobs_release_capacity_and_panics_do_not_remove_workers()
    -> Result<(), String> {
        let pool = Pool::new(1).map_err(|e| e.to_string())?;
        let (gate_tx, gate) = mpsc::channel::<()>();
        let (started, rx) = mpsc::channel();
        submit(&pool, Priority::Background, None, move || {
            let _ = started.send(());
            let _ = gate.recv();
        })?;
        rx.recv_timeout(Duration::from_secs(5))
            .map_err(|e| e.to_string())?;
        let cancel = Cancellation::default();
        let (out, events) = mpsc::channel();
        for _ in 0..QUEUE_CAPACITY {
            let out = out.clone();
            pool.submit(
                cancel.clone(),
                Priority::Interactive,
                None,
                Box::new(move || {
                    let _ = out.send("cancelled job ran");
                }),
            )
            .map_err(str::to_owned)?;
        }
        cancel.cancel();
        submit(&pool, Priority::Interactive, None, || {
            std::panic::resume_unwind(Box::new("test panic"))
        })?;
        submit(&pool, Priority::Interactive, None, move || {
            let _ = out.send("worker survived");
        })?;
        let _ = gate_tx.send(());
        assert_eq!(
            events
                .recv_timeout(Duration::from_secs(5))
                .map_err(|e| e.to_string())?,
            "worker survived"
        );
        pool.shutdown();
        Ok(())
    }
    #[test]
    fn cancelling_a_queued_mutation_completes_before_a_worker_becomes_available()
    -> Result<(), String> {
        let pool = Pool::new(1).map_err(|e| e.to_string())?;
        let (gate_tx, gate) = mpsc::channel::<()>();
        let (started, rx) = mpsc::channel();
        submit(&pool, Priority::Background, Some("repo"), move || {
            let _ = started.send(());
            let _ = gate.recv();
        })?;
        rx.recv_timeout(Duration::from_secs(5))
            .map_err(|e| e.to_string())?;
        let cancel = Cancellation::default();
        let (done, result) = mpsc::channel();
        let ran = done.clone();
        pool.submit_cancellable(
            cancel.clone(),
            Priority::Background,
            Some("repo".into()),
            Some(Box::new(move || {
                let _ = done.send("cancelled");
            })),
            Box::new(move || {
                let _ = ran.send("ran");
            }),
        )
        .map_err(str::to_owned)?;
        cancel.cancel();
        submit(&pool, Priority::Interactive, None, || {})?;
        assert_eq!(
            result
                .recv_timeout(Duration::from_secs(1))
                .map_err(|e| e.to_string())?,
            "cancelled"
        );
        drop(gate_tx);
        Ok(())
    }

    #[test]
    fn dropping_a_reader_waits_for_its_cancellation_without_cancelling_its_parent()
    -> Result<(), String> {
        let parent = Cancellation::default();
        CURRENT.with(|slot| *slot.borrow_mut() = Some(parent.clone()));
        let (started, rx) = mpsc::channel();
        let reader = read(move || {
            let cancel = cancellation();
            let _ = started.send(());
            while !cancel.is_cancelled() {
                std::thread::sleep(Duration::from_millis(5));
            }
        });
        CURRENT.with(|slot| *slot.borrow_mut() = None);
        let reader = reader?;
        rx.recv_timeout(Duration::from_secs(5))
            .map_err(|e| e.to_string())?;
        drop(reader);
        assert!(!parent.is_cancelled());
        Ok(())
    }

    #[test]
    fn writers_share_a_key_while_other_repositories_and_reserved_reads_can_run()
    -> Result<(), String> {
        let pool = Pool::with_limits(3, 16, 1).map_err(|e| e.to_string())?;
        let (gate_tx, gate) = mpsc::channel::<()>();
        let (tx, events) = mpsc::channel();
        let started = tx.clone();
        submit(&pool, Priority::Background, Some("repo"), move || {
            let _ = started.send("write 1");
            let _ = gate.recv();
        })?;
        assert_eq!(
            events
                .recv_timeout(Duration::from_secs(5))
                .map_err(|e| e.to_string())?,
            "write 1"
        );
        let next = tx.clone();
        submit(&pool, Priority::Interactive, Some("repo"), move || {
            let _ = next.send("write 2");
        })?;
        let read = tx.clone();
        submit(
            &pool,
            Priority::Interactive,
            Some("other repo"),
            move || {
                let _ = read.send("other repo");
            },
        )?;
        assert_eq!(
            events
                .recv_timeout(Duration::from_secs(5))
                .map_err(|e| e.to_string())?,
            "other repo"
        );
        assert!(
            events.try_recv().is_err(),
            "the second write ran while the first held its key"
        );
        let _ = gate_tx.send(());
        assert_eq!(
            events
                .recv_timeout(Duration::from_secs(5))
                .map_err(|e| e.to_string())?,
            "write 2"
        );
        pool.shutdown();
        Ok(())
    }
}
