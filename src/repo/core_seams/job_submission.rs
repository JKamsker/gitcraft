use super::*;

pub(in crate::repo) fn spawn(
    tx: &JobSender,
    ctx: &egui::Context,
    job: impl FnOnce() -> Msg + Send + 'static,
) {
    spawn_or(tx, ctx, Msg::JobPanicked, job);
}

impl RepoTab {
    /// Like `then`, but rejection, cancellation and panic have a feature-specific result.
    pub(in crate::repo) fn then_or<T: Send + 'static>(
        &self,
        ctx: &egui::Context,
        mutation: bool,
        failed: impl FnOnce(String) -> T + Send + 'static,
        work: impl FnOnce() -> T + Send + 'static,
        then: impl FnOnce(&mut RepoTab, T, &egui::Context) + Send + 'static,
    ) {
        let then = std::sync::Arc::new(std::sync::Mutex::new(Some(then)));
        let failure_then = std::sync::Arc::clone(&then);
        let failure = move |error| {
            let found = failed(error);
            Msg::Then(Box::new(move |tab, ctx| {
                if let Some(then) = failure_then
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .take()
                {
                    then(tab, found, ctx);
                }
            }))
        };
        let job = move || {
            let found = work();
            Msg::Then(Box::new(move |tab, ctx| {
                if let Some(then) = then
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .take()
                {
                    then(tab, found, ctx);
                }
            }))
        };
        if mutation {
            spawn_mutation(&self.tx, ctx, failure, job);
        } else {
            spawn_or(&self.tx, ctx, failure, job);
        }
    }

    /// Runs `work` in the background, then `then` with what it gave, on the UI thread.
    pub(in crate::repo) fn then<T: Send + 'static>(
        &self,
        ctx: &egui::Context,
        work: impl FnOnce() -> T + Send + 'static,
        then: impl FnOnce(&mut RepoTab, T, &egui::Context) + Send + 'static,
    ) {
        spawn(&self.tx, ctx, move || {
            let found = work();
            Msg::Then(Box::new(move |tab, ctx| then(tab, found, ctx)))
        });
    }

    /// [`Self::then`] for a repository on another machine, where `work` is a round trip or
    /// more (and may wait for a connection); for one here, right away, as it always was.
    pub(in crate::repo) fn here_or_later<T: Send + 'static>(
        &mut self,
        ctx: &egui::Context,
        work: impl FnOnce() -> T + Send + 'static,
        then: impl FnOnce(&mut RepoTab, T, &egui::Context) + Send + 'static,
    ) {
        if crate::remote::is_remote(&self.path) {
            self.then(ctx, work, then);
        } else {
            let found = work();
            then(self, found, ctx);
        }
    }
}

/// [`spawn`], sending what `failed` makes of what `job` said if it panics: the message it
/// would have sent had it failed, so that whatever waits for that message (the busy
/// operation, the loading flag…) is let go of.
pub(in crate::repo) fn spawn_or(
    tx: &JobSender,
    ctx: &egui::Context,
    failed: impl FnOnce(String) -> Msg + Send + 'static,
    job: impl FnOnce() -> Msg + Send + 'static,
) {
    spawn_with(
        tx,
        ctx,
        crate::tasks::Priority::Interactive,
        None,
        tx.cancel.clone(),
        failed,
        job,
    );
}

/// [`spawn_or`] for what the user just asked to see (the commit clicked): it goes ahead of all
/// else (see [`crate::tasks::Priority::Urgent`]) and ends with `cancel`, cancelled when the
/// selection moves on. `prefetch` reads what may be asked next, behind everything else.
pub(in crate::repo) fn spawn_selection(
    tx: &JobSender,
    ctx: &egui::Context,
    cancel: crate::tasks::Cancellation,
    prefetch: bool,
    failed: impl FnOnce(String) -> Msg + Send + 'static,
    job: impl FnOnce() -> Msg + Send + 'static,
) {
    let priority = if prefetch {
        crate::tasks::Priority::Speculative
    } else {
        crate::tasks::Priority::Urgent
    };
    spawn_with(tx, ctx, priority, None, cancel, failed, job);
}

/// Serializes a whole mutation operation across tabs of the same repository.
pub(in crate::repo) fn spawn_mutation(
    tx: &JobSender,
    ctx: &egui::Context,
    failed: impl FnOnce(String) -> Msg + Send + 'static,
    job: impl FnOnce() -> Msg + Send + 'static,
) {
    spawn_with(
        tx,
        ctx,
        crate::tasks::Priority::Background,
        Some(tx.repo.clone()),
        tx.cancel.clone(),
        failed,
        job,
    );
}

#[cfg(test)]
pub(super) type RecordedJob = Box<dyn FnOnce() -> Msg + Send>;

#[cfg(test)]
thread_local! {
    static RECORDED_JOBS: std::cell::RefCell<Option<Vec<RecordedJob>>> = const { std::cell::RefCell::new(None) };
}

/// Capture submissions on this test thread without executing their Git/process work.
#[cfg(test)]
pub(in crate::repo) fn record_jobs<T>(work: impl FnOnce() -> T) -> (T, Vec<RecordedJob>) {
    struct Reset;
    impl Drop for Reset {
        fn drop(&mut self) {
            RECORDED_JOBS.with(|slot| *slot.borrow_mut() = None);
        }
    }
    RECORDED_JOBS.with(|slot| {
        assert!(slot.borrow().is_none(), "nested job recording");
        *slot.borrow_mut() = Some(Vec::new());
    });
    let _reset = Reset;
    let result = work();
    let jobs = RECORDED_JOBS.with(|slot| slot.borrow_mut().take().unwrap_or_default());
    (result, jobs)
}

pub(super) fn spawn_with(
    tx: &JobSender,
    ctx: &egui::Context,
    priority: crate::tasks::Priority,
    key: Option<std::path::PathBuf>,
    cancel: crate::tasks::Cancellation,
    failed: impl FnOnce(String) -> Msg + Send + 'static,
    job: impl FnOnce() -> Msg + Send + 'static,
) {
    #[cfg(test)]
    let job = {
        let mut job: Option<RecordedJob> = Some(Box::new(job));
        RECORDED_JOBS.with(|slot| {
            if let Some(jobs) = &mut *slot.borrow_mut()
                && let Some(job) = job.take()
            {
                jobs.push(job);
            }
        });
        let Some(job) = job else { return };
        job
    };
    let tx = tx.clone();
    let ctx = ctx.clone();
    let rejected_tx = tx.clone();
    let rejected_ctx = ctx.clone();
    let failed = std::sync::Arc::new(std::sync::Mutex::new(Some(failed)));
    let rejected = std::sync::Arc::clone(&failed);
    let cancelled = std::sync::Arc::clone(&failed);
    let cancelled_tx = tx.clone();
    let cancelled_ctx = ctx.clone();
    let submitted = crate::tasks::pool().and_then(|pool| {
        pool.submit_cancellable(
            cancel,
            priority,
            key,
            Some(Box::new(move || {
                if let Some(failed) = cancelled
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .take()
                {
                    let _ = cancelled_tx.send(failed(crate::i18n::text("repo-core-2896").into()));
                    cancelled_ctx.request_repaint();
                }
            })),
            Box::new(move || {
                // What the job owned goes with it; a lock it held is left poisoned, which those
                // taking it handle.
                let msg = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(job)) {
                    Ok(msg) => msg,
                    Err(payload) => {
                        let Some(failed) = failed
                            .lock()
                            .unwrap_or_else(std::sync::PoisonError::into_inner)
                            .take()
                        else {
                            return;
                        };
                        failed(panic_text(payload.as_ref()))
                    }
                };
                let _ = tx.send(msg);
                ctx.request_repaint();
            }),
        )
        .map_err(str::to_owned)
    });
    if let Err(error) = submitted
        && let Some(failed) = rejected
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take()
    {
        let _ = rejected_tx.send(failed(error));
        rejected_ctx.request_repaint();
    }
}

/// What a panic said, as an error to show.
pub(super) fn panic_text(payload: &(dyn std::any::Any + Send)) -> String {
    let said = payload
        .downcast_ref::<&str>()
        .map(|s| (*s).to_owned())
        .or_else(|| payload.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| crate::i18n::text("repo-core-2897").to_owned());
    crate::i18n::message("repo-core-2898", &[("said", said.to_string().into())])
}
