//! The library's background scans: at most [`MAX_WORKERS`] threads at once, each walking one
//! source with the shared engine and sending what it finds as it goes. Every scan carries a
//! token and the profile it was started for; a result is applied only while the scan is
//! still the current one for a source the library still has, in that profile.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};

use super::model::{Library, Outcome, ScanResult, Source};
use crate::app::repo_scan::{self, Found, Progress, ScanEvent, ScanLimits};

/// The most scans running at once. A scan stopped while a slow mount blocks it keeps its
/// slot until its thread really ends: a blocked call never makes another thread start.
pub(in crate::app) const MAX_WORKERS: usize = 2;

/// A scan to run.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::app) struct Job {
    pub source: Source,
    pub path: PathBuf,
    pub limits: ScanLimits,
}

/// What walks a job (the engine; tests give their own).
pub(in crate::app) type Runner = Arc<
    dyn Fn(&Job, &AtomicBool, &mut dyn FnMut(ScanEvent)) -> Result<Found, String> + Send + Sync,
>;

enum Msg {
    Found(Vec<PathBuf>),
    Progress(Progress),
    Done(Result<Found, String>),
    /// The thread ended (also after a panic): its slot is free.
    Ended,
}

struct Running {
    token: u64,
    profile: u64,
    source: Source,
    cancel: Arc<AtomicBool>,
    progress: Progress,
    /// Everything found so far (canonical paths).
    found: Vec<PathBuf>,
    /// No longer wanted: what it sends is dropped. It still holds its slot until it ends.
    retired: bool,
}

/// Where a source's scan stands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::app) enum Activity {
    Queued,
    Running(Progress),
}

pub(in crate::app) struct Scheduler {
    tx: Sender<(u64, Msg)>,
    rx: Receiver<(u64, Msg)>,
    queue: VecDeque<(u64, Job)>,
    running: Vec<Running>,
    next_token: u64,
    runner: Runner,
    repaint: Option<egui::Context>,
}

impl Default for Scheduler {
    fn default() -> Self {
        Self::with_runner(Arc::new(|job, cancel, on| {
            repo_scan::walk(&job.path, job.limits, cancel, on)
        }))
    }
}

/// Frees the thread's slot however it ends.
struct Ended(Sender<(u64, Msg)>, u64, Option<egui::Context>);

impl Drop for Ended {
    fn drop(&mut self) {
        let _ = self.0.send((self.1, Msg::Ended));
        if let Some(ctx) = &self.2 {
            ctx.request_repaint();
        }
    }
}

impl Scheduler {
    pub fn with_runner(runner: Runner) -> Self {
        let (tx, rx) = channel();
        Self {
            tx,
            rx,
            queue: VecDeque::new(),
            running: Vec::new(),
            next_token: 1,
            runner,
            repaint: None,
        }
    }

    /// The window to wake when a scan has news.
    pub fn set_repaint(&mut self, ctx: &egui::Context) {
        if self.repaint.is_none() {
            self.repaint = Some(ctx.clone());
        }
    }

    /// Asks for `job` in `profile`; nothing when that source is already waiting or running.
    pub fn queue(&mut self, profile: u64, job: Job) -> bool {
        let busy = self
            .queue
            .iter()
            .any(|(p, j)| *p == profile && j.source == job.source)
            || self
                .running
                .iter()
                .any(|r| !r.retired && r.profile == profile && r.source == job.source);
        if !busy {
            self.queue.push_back((profile, job));
        }
        !busy
    }

    /// Stops `source`'s scan: what it found so far is kept, as a partial result.
    pub fn stop(&mut self, source: &Source) {
        self.queue.retain(|(_, j)| &j.source != source);
        for r in self.running.iter().filter(|r| &r.source == source) {
            r.cancel.store(true, Ordering::Relaxed);
        }
    }

    /// Drops `source`'s scans and anything they would still send (the root was edited,
    /// removed or turned off).
    pub fn retire(&mut self, source: &Source) {
        self.queue.retain(|(_, j)| &j.source != source);
        for r in self.running.iter_mut().filter(|r| &r.source == source) {
            r.retired = true;
            r.cancel.store(true, Ordering::Relaxed);
        }
    }

    /// Drops every scan (the profile changed).
    pub fn retire_all(&mut self) {
        self.queue.clear();
        for r in &mut self.running {
            r.retired = true;
            r.cancel.store(true, Ordering::Relaxed);
        }
    }

    pub fn activity(&self, profile: u64, source: &Source) -> Option<Activity> {
        if let Some(r) = self
            .running
            .iter()
            .find(|r| !r.retired && r.profile == profile && &r.source == source)
        {
            return Some(Activity::Running(r.progress));
        }
        self.queue
            .iter()
            .any(|(p, j)| *p == profile && &j.source == source)
            .then_some(Activity::Queued)
    }

    /// Threads running (also those stopped and not ended yet).
    pub fn threads(&self) -> usize {
        self.running.len()
    }

    /// Whether anything is waiting or running for the library.
    pub fn busy(&self) -> bool {
        !self.queue.is_empty() || self.running.iter().any(|r| !r.retired)
    }

    /// Applies what the scans sent to `library` (that of `profile`, the one in use) and
    /// starts what waits while there is room. Returns whether the library changed.
    pub fn poll(&mut self, profile: u64, library: &mut Library, now: i64) -> bool {
        let mut changed = false;
        while let Ok((token, msg)) = self.rx.try_recv() {
            let Some(at) = self.running.iter().position(|r| r.token == token) else {
                continue;
            };
            if matches!(msg, Msg::Ended) {
                self.running.remove(at);
                continue;
            }
            let run = &mut self.running[at];
            if run.retired || run.profile != profile || !library.has_source(&run.source) {
                run.retired = true;
                continue;
            }
            match msg {
                Msg::Progress(p) => run.progress = p,
                Msg::Found(batch) => {
                    library.add_found(&run.source, &batch);
                    run.found.extend(batch);
                    changed = true;
                }
                Msg::Done(result) => {
                    let outcome = match &result {
                        Ok(found) => found.stopped.map_or(Outcome::Complete, Outcome::Partial),
                        Err(e) => Outcome::Failed(e.clone()),
                    };
                    let repos = std::mem::take(&mut run.found);
                    library.finish(
                        &run.source,
                        ScanResult {
                            repos,
                            outcome,
                            at: now,
                        },
                    );
                    // Done: nothing more is applied; the slot frees once the thread ends.
                    run.retired = true;
                    changed = true;
                }
                Msg::Ended => {}
            }
        }
        self.start_waiting();
        changed
    }

    fn start_waiting(&mut self) {
        while self.running.len() < MAX_WORKERS {
            let Some((profile, job)) = self.queue.pop_front() else {
                return;
            };
            let token = self.next_token;
            self.next_token += 1;
            let cancel = Arc::new(AtomicBool::new(false));
            self.running.push(Running {
                token,
                profile,
                source: job.source.clone(),
                cancel: cancel.clone(),
                progress: Progress::default(),
                found: Vec::new(),
                retired: false,
            });
            let (tx, runner, ctx) = (self.tx.clone(), self.runner.clone(), self.repaint.clone());
            let spawned = std::thread::Builder::new()
                .name("library-scan".into())
                .spawn(move || run(job, token, &cancel, tx, runner, ctx));
            if let Err(e) = spawned {
                let _ = self.tx.send((token, Msg::Done(Err(e.to_string()))));
                let _ = self.tx.send((token, Msg::Ended));
            }
        }
    }
}

/// The scan's thread: canonical paths (as opening them would give) in batches, progress
/// now and then, then how it ended.
fn run(
    job: Job,
    token: u64,
    cancel: &AtomicBool,
    tx: Sender<(u64, Msg)>,
    runner: Runner,
    ctx: Option<egui::Context>,
) {
    let _ended = Ended(tx.clone(), token, ctx.clone());
    let batch = RefCell::new(Vec::new());
    let send = |msg| {
        let _ = tx.send((token, msg));
        if let Some(ctx) = &ctx {
            ctx.request_repaint();
        }
    };
    let flush = || {
        let found = batch.take();
        if !found.is_empty() {
            send(Msg::Found(found));
        }
    };
    let result = runner(&job, cancel, &mut |event| match event {
        ScanEvent::Repo(path) => batch
            .borrow_mut()
            .push(repo_scan::top_folder(path.to_owned())),
        ScanEvent::Progress(p) => {
            flush();
            send(Msg::Progress(p));
        }
    });
    flush();
    send(Msg::Done(result));
}

#[cfg(test)]
#[path = "scheduler_tests.rs"]
mod tests;
