//! Bounded queue admission, mutation exclusion, and worker lifecycle.
use super::*;

struct Job {
    work: Work,
    connections: crate::connections::Context,
    on_cancel: Option<Work>,
    cancel: Cancellation,
    priority: Priority,
    /// A whole mutation operation holds its key, including between subprocesses.
    key: Option<MutationScheduleKey>,
}

#[derive(Default)]
struct State {
    queue: VecDeque<Job>,
    writing: HashSet<MutationScheduleKey>,
    active: usize,
    background_active: usize,
    speculative_active: usize,
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
    pub(super) fn peak_queued(&self) -> usize {
        self.shared
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .peak_queued
    }

    #[cfg(test)]
    pub(crate) fn queued_priority_for(&self, cancel: &Cancellation) -> Option<Priority> {
        self.shared
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .queue
            .iter()
            .find(|job| Arc::ptr_eq(&job.cancel.0, &cancel.0))
            .map(|job| job.priority)
    }

    #[cfg(test)]
    pub(crate) fn queued_priorities_for(&self, key: &std::path::Path) -> Vec<Priority> {
        let key = mutation_schedule_key(key.to_owned());
        self.shared
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .queue
            .iter()
            .filter(|job| job.key.as_ref() == Some(&key))
            .map(|job| job.priority)
            .collect()
    }

    #[cfg(test)]
    pub(crate) fn new(workers: usize) -> std::io::Result<Self> {
        Self::with_capacity(workers, QUEUE_CAPACITY)
    }

    pub(super) fn with_capacity(workers: usize, capacity: usize) -> std::io::Result<Self> {
        Self::with_limits(workers, capacity, workers)
    }

    pub(super) fn with_limits(
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
        let key = key.map(mutation_schedule_key);
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
        let job = Job {
            work,
            connections: crate::connections::current(),
            on_cancel,
            cancel,
            priority,
            key,
        };
        // Dedicated urgent threads are only for reads; keyed jobs must hold the
        // shared mutation bucket through the complete operation.
        if priority == Priority::Urgent && job.key.is_none() {
            drop(state);
            let Some(job) = spawn_urgent(job) else {
                return Ok(());
            };
            // No thread for it: first in the queue.
            state = self
                .shared
                .state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            state.queue.push_front(job);
            self.shared.ready.notify_all();
            return Ok(());
        }
        if state.queue.len() >= self.capacity {
            return Err(BUSY);
        }
        state.queue.push_back(job);
        state.peak_queued = state.peak_queued.max(state.queue.len());
        self.shared.ready.notify_all();
        Ok(())
    }

    pub(super) fn shutdown(&self) {
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

    pub(super) fn join(&self) {
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

/// Starts an urgent job on a thread of its own, or gives it back when too many already run.
fn spawn_urgent(job: Job) -> Option<Job> {
    if URGENT_RUNNING.fetch_add(1, Ordering::AcqRel) >= URGENT_THREADS {
        URGENT_RUNNING.fetch_sub(1, Ordering::AcqRel);
        return Some(job);
    }
    let slot = Arc::new(Mutex::new(Some(job)));
    let taken = Arc::clone(&slot);
    let spawned = std::thread::Builder::new()
        .name("gitcraft-urgent".into())
        .spawn(move || {
            struct Done;
            impl Drop for Done {
                fn drop(&mut self) {
                    URGENT_RUNNING.fetch_sub(1, Ordering::AcqRel);
                }
            }
            let _done = Done;
            let job = taken
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .take();
            if let Some(job) = job {
                run_urgent(job);
            }
        });
    if spawned.is_ok() {
        return None;
    }
    URGENT_RUNNING.fetch_sub(1, Ordering::AcqRel);
    slot.lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .take()
}

fn run_urgent(mut job: Job) {
    if job.cancel.is_cancelled() {
        if let Some(done) = job.on_cancel.take() {
            let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(done));
        }
        return;
    }
    CURRENT.with(|slot| *slot.borrow_mut() = Some(job.cancel.clone()));
    URGENT.with(|u| u.set(true));
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        job.connections.run(job.work)
    }));
    URGENT.with(|u| u.set(false));
    CURRENT.with(|slot| *slot.borrow_mut() = None);
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
                    let room = match job.priority {
                        Priority::Background => state.background_active < shared.background_limit,
                        Priority::Speculative => state.speculative_active < SPECULATIVE_LIMIT,
                        Priority::Urgent | Priority::Interactive => true,
                    };
                    if runnable && room {
                        candidates.push(i);
                    }
                }
                let priority = |i: &usize| state.queue[*i].priority;
                // An urgent job that found no thread of its own goes first, the newest of them;
                // reading ahead only once nothing else is runnable.
                let selected = candidates
                    .iter()
                    .rev()
                    .find(|i| priority(i) == Priority::Urgent)
                    .or_else(|| {
                        candidates
                            .iter()
                            .find(|i| priority(i) != Priority::Speculative)
                    })
                    .or(candidates.first())
                    .copied();
                if let Some(i) = selected
                    && let Some(job) = state.queue.remove(i)
                {
                    if let Some(key) = &job.key {
                        state.writing.insert(key.clone());
                    }
                    state.active += 1;
                    match job.priority {
                        Priority::Background => state.background_active += 1,
                        Priority::Speculative => state.speculative_active += 1,
                        Priority::Urgent | Priority::Interactive => {}
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
        URGENT.with(|u| u.set(job.priority == Priority::Urgent));
        SPECULATIVE.with(|s| s.set(job.priority == Priority::Speculative));
        // Most callers supply an error continuation. Also contain panics from any other
        // caller so the fixed pool never silently loses a worker.
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            job.connections.run(job.work)
        }));
        URGENT.with(|u| u.set(false));
        SPECULATIVE.with(|s| s.set(false));
        CURRENT.with(|slot| *slot.borrow_mut() = None);
        let mut state = shared
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        state.active -= 1;
        match job.priority {
            Priority::Background => state.background_active -= 1,
            Priority::Speculative => state.speculative_active -= 1,
            Priority::Urgent | Priority::Interactive => {}
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
