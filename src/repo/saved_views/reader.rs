//! Latest-only reads on the application's bounded worker pool. The running slot remains
//! owned until the shared local/remote transport returns, even after cancellation.
use crate::tasks::{Cancellation, Priority, Scope};
use std::sync::mpsc::{self, Receiver, TryRecvError};
type Read<T> = Box<dyn FnOnce(&dyn Fn() -> bool) -> Result<T, String> + Send>;
struct Job<T> {
    read: Read<T>,
    key: Option<std::path::PathBuf>,
}
struct Running<T> {
    id: u64,
    rx: Receiver<Result<T, String>>,
    scope: Scope,
}
pub(in crate::repo) struct Reader<T> {
    parent: Cancellation,
    generation: u64,
    pending: Option<Job<T>>,
    running: Option<Running<T>>,
}
impl<T> Default for Reader<T> {
    fn default() -> Self {
        Self {
            parent: Cancellation::default(),
            generation: 0,
            pending: None,
            running: None,
        }
    }
}
impl<T: Send + 'static> Reader<T> {
    pub fn bind(&mut self, parent: Cancellation) {
        self.parent = parent;
    }
    pub fn cancel(&mut self) {
        self.generation = self.generation.wrapping_add(1);
        self.pending = None;
        if let Some(running) = &self.running {
            running.scope.0.cancel();
        }
    }
    pub fn queue(
        &mut self,
        job: impl FnOnce(&dyn Fn() -> bool) -> Result<T, String> + Send + 'static,
    ) {
        self.cancel();
        self.pending = Some(Job {
            read: Box::new(job),
            key: None,
        });
    }
    pub fn queue_mutation(
        &mut self,
        key: std::path::PathBuf,
        job: impl FnOnce(&dyn Fn() -> bool) -> Result<T, String> + Send + 'static,
    ) {
        self.cancel();
        self.pending = Some(Job {
            read: Box::new(job),
            key: Some(key),
        });
    }
    pub fn busy(&self) -> bool {
        self.running.is_some() || self.pending.is_some()
    }
    pub fn poll(&mut self, ctx: &egui::Context) -> Option<Result<T, String>> {
        let mut answer = None;
        if let Some(running) = &self.running {
            match running.rx.try_recv() {
                Ok(result) => {
                    if running.id == self.generation {
                        answer = Some(result);
                    }
                    self.running = None;
                }
                Err(TryRecvError::Disconnected) => {
                    if running.id == self.generation {
                        answer = Some(Err("History worker stopped".into()));
                    }
                    self.running = None;
                }
                Err(TryRecvError::Empty) => {}
            }
        }
        if self.running.is_none()
            && let Some(job) = self.pending.take()
        {
            let (tx, rx) = mpsc::channel();
            let cancel = self.parent.child();
            let token = cancel.clone();
            let rejected = tx.clone();
            let cancelled_tx = tx.clone();
            let repaint = ctx.clone();
            let cancel_repaint = ctx.clone();
            let submitted = crate::tasks::pool().and_then(|pool| {
                pool.submit_cancellable(
                    cancel.clone(),
                    Priority::Background,
                    job.key,
                    Some(Box::new(move || {
                        let _ = cancelled_tx.send(Err("History read cancelled".into()));
                        cancel_repaint.request_repaint();
                    })),
                    Box::new(move || {
                        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                            (job.read)(&|| !token.is_cancelled())
                        }))
                        .unwrap_or_else(|_| Err("History worker panicked".into()));
                        let _ = tx.send(result);
                        repaint.request_repaint();
                    }),
                )
                .map_err(str::to_owned)
            });
            if let Err(error) = submitted {
                let _ = rejected.send(Err(error.into()));
            }
            self.running = Some(Running {
                id: self.generation,
                rx,
                scope: Scope(cancel),
            });
        }
        if self.busy() {
            ctx.request_repaint_after(std::time::Duration::from_millis(20));
        }
        answer
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn replacements_coalesce_and_hold_the_slot_until_cancelled_work_returns() -> Result<(), String>
    {
        let ctx = egui::Context::default();
        let mut reader = Reader::default();
        let (entered, entry) = mpsc::channel();
        let (release, released) = mpsc::channel();
        reader.queue(move |_| {
            let _ = entered.send(());
            let _ = released.recv_timeout(std::time::Duration::from_secs(5));
            Ok(1)
        });
        reader.poll(&ctx);
        entry
            .recv_timeout(std::time::Duration::from_secs(5))
            .map_err(|e| e.to_string())?;
        for i in 2..101 {
            reader.queue(move |_| Ok(i));
            reader.poll(&ctx);
        }
        release.send(()).map_err(|e| e.to_string())?;
        let start = std::time::Instant::now();
        loop {
            if let Some(result) = reader.poll(&ctx) {
                assert_eq!(result?, 100);
                break;
            }
            if start.elapsed().as_secs() > 5 {
                return Err("Reader did not finish".into());
            }
            std::thread::yield_now();
        }
        Ok(())
    }
}
