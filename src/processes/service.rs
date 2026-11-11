//! Process queue admission, priority scheduling, tickets, and runtime ownership.
use super::*;

struct Request {
    command: Command,
    options: Options,
    reply: mpsc::Sender<ResultOutput>,
    ticket: Cancellation,
    deadline: Option<Instant>,
    submitted: Instant,
    #[cfg(test)]
    trace: Option<Arc<trace::Trace>>,
}

pub(crate) struct Ticket {
    pub(super) reply: mpsc::Receiver<ResultOutput>,
    cancel: Cancellation,
    #[cfg(test)]
    trace: Option<Arc<trace::Trace>>,
}

impl Ticket {
    pub(crate) fn wait(self) -> ResultOutput {
        #[cfg(test)]
        if let Some(trace) = &self.trace {
            loop {
                match self.reply.recv_timeout(Duration::from_secs(30)) {
                    Ok(result) => return result,
                    Err(mpsc::RecvTimeoutError::Disconnected) => {
                        return Err("Process service stopped before returning a result".into());
                    }
                    Err(mpsc::RecvTimeoutError::Timeout) => trace.report(),
                }
            }
        }
        self.reply
            .recv()
            .map_err(|_| "Process service stopped before returning a result".to_owned())?
    }
    pub(crate) fn try_result(&self) -> Option<ResultOutput> {
        match self.reply.try_recv() {
            Ok(result) => Some(result),
            Err(mpsc::TryRecvError::Empty) => None,
            Err(mpsc::TryRecvError::Disconnected) => Some(Err(
                "Process service stopped before returning a result".into(),
            )),
        }
    }
    pub(crate) fn cancel(&self) {
        self.cancel.cancel();
    }
}

impl Drop for Ticket {
    fn drop(&mut self) {
        self.cancel.cancel();
    }
}

#[derive(Default)]
pub(super) struct Counters {
    active: AtomicUsize,
    queued: AtomicUsize,
    pub(super) spawned: AtomicUsize,
    peak: AtomicUsize,
}

#[derive(Clone, Copy, Debug, serde::Serialize)]
pub(crate) struct Counts {
    pub active: usize,
    pub queued: usize,
    pub spawned: usize,
    pub peak: usize,
}

pub(crate) struct Service {
    sender: tokio::sync::mpsc::Sender<Request>,
    stop: Cancellation,
    thread: Mutex<Option<std::thread::JoinHandle<()>>>,
    counters: Arc<Counters>,
    capacity: usize,
}

mod blocking_budget;

impl Service {
    pub(super) fn new(active: usize, queued: usize) -> io::Result<Self> {
        let (sender, mut receiver) = tokio::sync::mpsc::channel::<Request>(queued);
        let stop = Cancellation::default();
        let shutdown = stop.clone();
        let counters = Arc::new(Counters::default());
        let metrics = Arc::clone(&counters);
        // Build before starting the thread, so runtime failures reach the caller.
        // Tokio's Windows anonymous pipes use blocking I/O internally. Each active child
        // needs room for both readers and stdin, plus remote cleanup. The bound follows
        // active children, never the number of queued requests.
        let blocking_threads = blocking_budget::threads(active, cfg!(windows));
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .max_blocking_threads(blocking_threads)
            .build()?;
        let thread = std::thread::Builder::new()
            .name("gitcraft-process-io".into())
            .spawn(move || {
                runtime.block_on(async move {
                    let blocking = Arc::new(tokio::sync::Semaphore::new(2));
                    let mut running = tokio::task::JoinSet::new();
                    let mut pending: VecDeque<Request> = VecDeque::new();
                    let mut background = 0usize;
                    let mut speculative = 0usize;
                    let background_limit = active.saturating_sub(2).max(1);
                    let mut tick = tokio::time::interval(POLL);
                    loop {
                        if shutdown.is_cancelled() {
                            break;
                        }
                        pending.retain(|request| {
                            let cancelled = request.ticket.is_cancelled()
                                || request.options.cancel.is_cancelled()
                                || request.options.cancelled.as_ref().is_some_and(|f| f());
                            let expired = request.deadline.is_some_and(|at| Instant::now() >= at);
                            if let Some(reason) = before_start(cancelled, expired) {
                                metrics.queued.fetch_sub(1, Ordering::Relaxed);
                                let _ = request.reply.send(Err(reason.into()));
                            }
                            !cancelled && !expired
                        });
                        loop {
                            // What the user waits on goes first, the newest of it, in slots
                            // of its own beyond the others when they are all taken.
                            let urgent = pending
                                .iter()
                                .rposition(|r| r.options.urgent)
                                .filter(|_| running.len() < active + URGENT_EXTRA);
                            // Reserve two slots for short reads; dispatch the oldest eligible
                            // request so a stream of reads cannot starve long operations.
                            // Reading ahead only once nothing else waits.
                            let selected = urgent.or_else(|| {
                                (running.len() < active)
                                    .then(|| {
                                        pending
                                            .iter()
                                            .position(|r| {
                                                !r.options.speculative
                                                    && (!r.options.background
                                                        || background < background_limit)
                                            })
                                            .or_else(|| {
                                                (speculative < SPECULATIVE
                                                    && pending
                                                        .iter()
                                                        .all(|r| r.options.speculative))
                                                .then_some(0)
                                            })
                                    })
                                    .flatten()
                            });
                            let Some(request) = selected.and_then(|i| pending.remove(i)) else {
                                break;
                            };
                            metrics.queued.fetch_sub(1, Ordering::Relaxed);
                            let is_background = request.options.background;
                            if is_background {
                                background += 1;
                            }
                            let is_speculative = request.options.speculative;
                            if is_speculative {
                                speculative += 1;
                            }
                            let stats = Arc::clone(&metrics);
                            let stop = shutdown.clone();
                            let blocking = Arc::clone(&blocking);
                            running.spawn(async move {
                                // Taken from the queue, and then started on this thread.
                                let queued = request.submitted.elapsed();
                                let n = stats.active.fetch_add(1, Ordering::Relaxed) + 1;
                                stats.peak.fetch_max(n, Ordering::Relaxed);
                                let result = execute(
                                    request.command,
                                    request.options,
                                    request.deadline,
                                    &request.ticket,
                                    &stop,
                                    &stats,
                                    &blocking,
                                    #[cfg(test)]
                                    request.trace.as_deref(),
                                )
                                .await
                                .map(|mut out| {
                                    out.phases.queued = queued;
                                    out
                                });
                                stats.active.fetch_sub(1, Ordering::Relaxed);
                                let _ = request.reply.send(result);
                                (is_background, is_speculative)
                            });
                        }
                        tokio::select! {
                            _ = tick.tick() => {}
                            Some(request) = receiver.recv() => pending.push_back(request),
                            Some(result) = running.join_next(), if !running.is_empty() => {
                                if let Ok((was_background, was_speculative)) = result {
                                    background -= usize::from(was_background);
                                    speculative -= usize::from(was_speculative);
                                }
                            }
                        }
                    }
                    receiver.close();
                    for request in pending {
                        metrics.queued.fetch_sub(1, Ordering::Relaxed);
                        let _ = request.reply.send(Err("cancelled".into()));
                    }
                    while let Some(request) = receiver.recv().await {
                        metrics.queued.fetch_sub(1, Ordering::Relaxed);
                        let _ = request.reply.send(Err("cancelled".into()));
                    }
                    // Never abort tasks: each kills and reaps its child before completing.
                    while running.join_next().await.is_some() {}
                });
            })?;
        Ok(Self {
            sender,
            stop,
            thread: Mutex::new(Some(thread)),
            counters,
            capacity: queued,
        })
    }

    pub(crate) fn submit(&self, command: Command, options: Options) -> Result<Ticket, String> {
        if self.stop.is_cancelled() {
            return Err("Process service has shut down".into());
        }
        let (reply, receive) = mpsc::channel();
        let ticket = Cancellation::default();
        let deadline = options.timeout.map(|limit| Instant::now() + limit);
        #[cfg(test)]
        let trace = trace::Trace::new(&command).map(Arc::new);
        let mut queued = self.counters.queued.load(Ordering::Relaxed);
        loop {
            if queued >= self.capacity {
                return Err("Process queue is full; try again when current work finishes".into());
            }
            match self.counters.queued.compare_exchange_weak(
                queued,
                queued + 1,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => break,
                Err(actual) => queued = actual,
            }
        }
        if self
            .sender
            .try_send(Request {
                command,
                options,
                reply,
                ticket: ticket.clone(),
                deadline,
                submitted: Instant::now(),
                #[cfg(test)]
                trace: trace.clone(),
            })
            .is_err()
        {
            self.counters.queued.fetch_sub(1, Ordering::Relaxed);
            return Err("Process queue is full; try again when current work finishes".into());
        }
        Ok(Ticket {
            reply: receive,
            cancel: ticket,
            #[cfg(test)]
            trace,
        })
    }

    pub(crate) fn counts(&self) -> Counts {
        Counts {
            active: self.counters.active.load(Ordering::Relaxed),
            queued: self.counters.queued.load(Ordering::Relaxed),
            spawned: self.counters.spawned.load(Ordering::Relaxed),
            peak: self.counters.peak.load(Ordering::Relaxed),
        }
    }

    pub(super) fn shutdown(&self) {
        self.stop.cancel();
        if let Some(thread) = self
            .thread
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take()
        {
            let _ = thread.join();
        }
    }
}

impl Drop for Service {
    fn drop(&mut self) {
        self.shutdown();
    }
}
