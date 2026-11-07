//! Latest-only work on the tab's shared task pool. Replacement cancels the owned job and
//! retains its slot until it returns; at most one replacement is kept.
use super::messages::{JobSender, Msg, spawn_or};
use std::sync::mpsc::{self, Receiver};
type Job<T> = Box<dyn FnOnce(&dyn Fn() -> bool) -> Result<T, String> + Send>;
pub(super) struct Reader<T> {
    generation: u64,
    pending: Option<Job<T>>,
    running: Option<(u64, crate::tasks::Cancellation, Receiver<Result<T, String>>)>,
}
impl<T> Default for Reader<T> {
    fn default() -> Self {
        Self {
            generation: 0,
            pending: None,
            running: None,
        }
    }
}
impl<T> Drop for Reader<T> {
    fn drop(&mut self) {
        if let Some((_, token, _)) = &self.running {
            token.cancel();
        }
    }
}
impl<T: Send + 'static> Reader<T> {
    pub fn cancel(&mut self) {
        self.generation = self.generation.wrapping_add(1);
        self.pending = None;
        if let Some((_, token, _)) = &self.running {
            token.cancel();
        }
    }
    pub fn queue(
        &mut self,
        job: impl FnOnce(&dyn Fn() -> bool) -> Result<T, String> + Send + 'static,
    ) {
        self.cancel();
        self.pending = Some(Box::new(job));
    }
    pub fn busy(&self) -> bool {
        self.pending.is_some() || self.running.is_some()
    }
    pub fn poll(&mut self, owner: &JobSender, ctx: &egui::Context) -> Option<Result<T, String>> {
        let mut answer = None;
        if let Some((id, _, rx)) = &self.running {
            let result = match rx.try_recv() {
                Ok(result) => Some(result),
                Err(mpsc::TryRecvError::Disconnected) => Some(Err("History worker stopped".into())),
                Err(mpsc::TryRecvError::Empty) => None,
            };
            if let Some(result) = result {
                if *id == self.generation {
                    answer = Some(result);
                }
                self.running = None;
            }
        }
        if self.running.is_none()
            && let Some(job) = self.pending.take()
        {
            let mut sender = owner.clone();
            sender.cancel = owner.cancel.child();
            let active = sender.cancel.clone();
            let (tx, rx) = mpsc::channel();
            self.running = Some((self.generation, sender.cancel.clone(), rx));
            let failed = tx.clone();
            spawn_or(
                &sender,
                ctx,
                move |error| {
                    let _ = failed.send(Err(error));
                    Msg::Then(Box::new(|_, _| {}))
                },
                move || {
                    let result = job(&|| !active.is_cancelled());
                    let _ = tx.send(result);
                    Msg::Then(Box::new(|_, _| {}))
                },
            );
        }
        if self.busy() {
            ctx.request_repaint_after(std::time::Duration::from_millis(30));
        }
        answer
    }
}
