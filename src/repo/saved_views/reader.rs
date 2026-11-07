//! Latest-only owned background reads, including the accepted transport's detached tail.
use crate::git::ForecastCompletion;
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
    mpsc::{self, Receiver, TryRecvError},
};
type Job<T> = Box<dyn FnOnce(&dyn Fn() -> bool) -> Result<T, String> + Send>;
struct Pending<T> {
    id: u64,
    context: crate::connections::Context,
    job: Job<T>,
}
struct Running<T> {
    id: u64,
    rx: Receiver<Result<T, String>>,
    answer: Option<Result<T, String>>,
    completion: ForecastCompletion,
}
pub(super) struct Reader<T> {
    generation: Arc<AtomicU64>,
    pending: Option<Pending<T>>,
    running: Option<Running<T>>,
}
impl<T> Default for Reader<T> {
    fn default() -> Self {
        Self {
            generation: Arc::new(AtomicU64::new(0)),
            pending: None,
            running: None,
        }
    }
}
impl<T: Send + 'static> Reader<T> {
    pub fn cancel(&mut self) {
        self.generation.fetch_add(1, Ordering::AcqRel);
        self.pending = None;
    }
    pub fn queue(
        &mut self,
        job: impl FnOnce(&dyn Fn() -> bool) -> Result<T, String> + Send + 'static,
    ) {
        self.cancel();
        let id = self.generation.load(Ordering::Acquire);
        self.pending = Some(Pending {
            id,
            context: crate::connections::current(),
            job: Box::new(job),
        });
    }
    pub fn busy(&self) -> bool {
        self.running.is_some() || self.pending.is_some()
    }
    pub fn poll(&mut self, ctx: &egui::Context) -> Option<Result<T, String>> {
        let mut answer = None;
        if let Some(running) = self.running.as_mut() {
            if running.answer.is_none() {
                match running.rx.try_recv() {
                    Ok(result) => running.answer = Some(result),
                    Err(TryRecvError::Disconnected) => {
                        running.answer = Some(Err(
                            "History worker stopped before returning a result".into(),
                        ))
                    }
                    Err(TryRecvError::Empty) => {}
                }
            }
            if running.answer.is_some() && !running.completion.pending() {
                if running.id == self.generation.load(Ordering::Acquire) {
                    answer = running.answer.take();
                }
                self.running = None;
            }
        }
        if self.running.is_none() {
            if let Some(Pending { id, context, job }) = self.pending.take() {
                let (tx, rx) = mpsc::channel();
                let completion = ForecastCompletion::default();
                let owner = completion.clone();
                let generation = Arc::downgrade(&self.generation);
                let repaint = ctx.clone();
                self.running = Some(Running {
                    id,
                    rx,
                    answer: None,
                    completion,
                });
                crate::connections::spawn(move || {
                    let active = || {
                        generation
                            .upgrade()
                            .is_some_and(|g| g.load(Ordering::Acquire) == id)
                    };
                    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        context.run(|| {
                            owner.run(|| {
                                if active() {
                                    job(&active)
                                } else {
                                    Err("History read cancelled".into())
                                }
                            })
                        })
                    }))
                    .unwrap_or_else(|_| Err("History worker panicked".into()));
                    let _ = tx.send(result);
                    repaint.request_repaint();
                });
            }
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
    fn replacement_and_cancel_keep_the_slot_until_actual_nested_work_returns() -> Result<(), String>
    {
        let ctx = egui::Context::default();
        let mut reader = Reader::<usize>::default();
        let (release, released) = mpsc::channel();
        let (entered, entry) = mpsc::channel();
        reader.queue(move |_| {
            crate::git::forecast_spawn(move || {
                let _ = entered.send(());
                let _ = released.recv_timeout(std::time::Duration::from_secs(5));
            });
            Ok(1)
        });
        reader.poll(&ctx);
        entry
            .recv_timeout(std::time::Duration::from_secs(5))
            .map_err(|e| e.to_string())?;
        let launched = Arc::new(AtomicU64::new(0));
        for i in 2..102 {
            let launched = launched.clone();
            reader.queue(move |_| {
                launched.store(i, Ordering::Release);
                Ok(i as usize)
            });
            reader.poll(&ctx);
        }
        assert_eq!(launched.load(Ordering::Acquire), 0);
        release.send(()).map_err(|e| e.to_string())?;
        let began = std::time::Instant::now();
        loop {
            if let Some(answer) = reader.poll(&ctx) {
                assert_eq!(answer?, 101);
                break;
            }
            if began.elapsed() > std::time::Duration::from_secs(5) {
                return Err("Reader did not finish".into());
            }
            std::thread::yield_now();
        }
        assert_eq!(launched.load(Ordering::Acquire), 101);
        let (started, start) = mpsc::channel();
        reader.queue(move |active| {
            started.send(()).map_err(|e| e.to_string())?;
            while active() {
                std::thread::yield_now();
            }
            Ok(200)
        });
        reader.poll(&ctx);
        start
            .recv_timeout(std::time::Duration::from_secs(5))
            .map_err(|e| e.to_string())?;
        reader.cancel();
        let began = std::time::Instant::now();
        while reader.busy() {
            assert!(reader.poll(&ctx).is_none());
            if began.elapsed() > std::time::Duration::from_secs(5) {
                return Err("Cancel did not stop worker".into());
            }
            std::thread::yield_now();
        }
        Ok(())
    }
    #[test]
    fn queued_read_keeps_initiating_profile_even_when_started_later() -> Result<(), String> {
        let mut reader = Reader::default();
        let ctx = egui::Context::default();
        let expected = crate::connections::Context {
            choices: crate::connections::Choices {
                ssh_key: "/recording/profile".into(),
                ..Default::default()
            },
            ..Default::default()
        };
        expected.run(|| reader.queue(|_| Ok(crate::connections::current())));
        reader.poll(&ctx);
        let began = std::time::Instant::now();
        loop {
            if let Some(answer) = reader.poll(&ctx) {
                assert_eq!(answer?, expected);
                break;
            }
            if began.elapsed() > std::time::Duration::from_secs(5) {
                return Err("Profile test timed out".into());
            }
            std::thread::yield_now();
        }
        Ok(())
    }
}
