//! A caller's whole-operation deadline, inherited by each owned process it submits.
use std::{cell::Cell, time::Instant};
thread_local! {
    static DEADLINE: Cell<Option<Instant>> = const { Cell::new(None) };
}
pub(crate) fn within<T>(deadline: Instant, work: impl FnOnce() -> T) -> T {
    struct Restore(Option<Instant>);
    impl Drop for Restore {
        fn drop(&mut self) {
            DEADLINE.with(|d| d.set(self.0));
        }
    }
    let _restore = Restore(
        DEADLINE.with(|d| d.replace(Some(earliest(d.get(), Some(deadline)).unwrap_or(deadline)))),
    );
    work()
}
pub(crate) fn cleanup<T>(work: impl FnOnce() -> T) -> T {
    let saved = DEADLINE.with(|d| d.replace(None));
    struct Restore(Option<Instant>);
    impl Drop for Restore {
        fn drop(&mut self) {
            DEADLINE.with(|d| d.set(self.0));
        }
    }
    let _restore = Restore(saved);
    within(Instant::now() + std::time::Duration::from_secs(30), work)
}
pub(super) fn earliest(a: Option<Instant>, b: Option<Instant>) -> Option<Instant> {
    match (a, b) {
        (Some(a), Some(b)) => Some(a.min(b)),
        (a, b) => a.or(b),
    }
}
pub(crate) fn check() -> Result<(), String> {
    if crate::tasks::cancellation().is_cancelled() {
        return Err("cancelled".into());
    }
    if effective(None).is_some_and(|d| Instant::now() >= d) {
        return Err("process timed out and was stopped".into());
    }
    Ok(())
}
pub(super) fn effective(per_process: Option<Instant>) -> Option<Instant> {
    DEADLINE.with(|d| earliest(d.get(), per_process))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn synthetic_deadline_includes_queue_and_nested_steps_and_restores() {
        let start = Instant::now();
        let later = start + std::time::Duration::from_secs(60);
        assert_eq!(effective(None), None);
        within(start, || {
            assert_eq!(effective(Some(later)), Some(start));
            within(later, || assert_eq!(effective(None), Some(start)));
            cleanup(|| assert!(effective(None).is_some_and(|d| d > start)));
            assert_eq!(effective(None), Some(start));
        });
        assert_eq!(effective(None), None);
    }
}
