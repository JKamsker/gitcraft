//! Ownership of detached remote work, independent of a caller's wait/deadline.
//! This changes no process control: a ticket lives until the existing worker returns.
use std::cell::RefCell;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

#[derive(Clone, Default)]
pub(crate) struct ForecastCompletion(Arc<AtomicUsize>);
thread_local! { static CURRENT: RefCell<Option<ForecastCompletion>> = const { RefCell::new(None) }; }
impl ForecastCompletion {
    pub fn pending(&self) -> bool {
        self.0.load(Ordering::Acquire) != 0
    }
    pub fn run<T>(&self, job: impl FnOnce() -> T) -> T {
        struct Restore(Option<ForecastCompletion>);
        impl Drop for Restore {
            fn drop(&mut self) {
                CURRENT.with(|current| *current.borrow_mut() = self.0.take());
            }
        }
        let _restore = Restore(CURRENT.with(|current| current.replace(Some(self.clone()))));
        job()
    }
}
struct Ticket(ForecastCompletion);
impl Drop for Ticket {
    fn drop(&mut self) {
        self.0.0.fetch_sub(1, Ordering::AcqRel);
    }
}
/// Acquire ownership before spawning, including nested cleanup workers. Scheduling,
/// connection/pool waits and the entire remote::exec lifetime remain covered.
pub(crate) fn forecast_spawn<T: Send + 'static>(
    job: impl FnOnce() -> T + Send + 'static,
) -> std::thread::JoinHandle<T> {
    let owner = CURRENT.with(|current| current.borrow().clone());
    let ticket = owner.as_ref().map(|owner| {
        owner.0.fetch_add(1, Ordering::AcqRel);
        Ticket(owner.clone())
    });
    crate::connections::spawn(move || {
        let _ticket = ticket;
        match owner {
            Some(owner) => owner.run(job),
            None => job(),
        }
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::{sync::mpsc, time::Duration};
    #[test]
    fn completion_tracks_nested_workers_after_the_original_caller_stops_waiting()
    -> Result<(), String> {
        let scope = ForecastCompletion::default();
        let profile = crate::connections::Context {
            choices: crate::connections::Choices {
                ssh_key: "/recording-only/profile-key".into(),
                ..Default::default()
            },
            ..Default::default()
        };
        let (finish, finished) = mpsc::channel();
        let (entered, entry) = mpsc::channel();
        let expected = profile.clone();
        let parent = profile.run(|| {
            scope.run(|| {
                forecast_spawn(move || {
                    assert_eq!(crate::connections::current(), expected);
                    let nested = crate::connections::current();
                    forecast_spawn(move || {
                        assert_eq!(crate::connections::current(), nested);
                        entered.send(()).map_err(|e| e.to_string())?;
                        finished
                            .recv_timeout(Duration::from_secs(5))
                            .map_err(|e| e.to_string())
                    })
                })
            })
        });
        let child = parent.join().map_err(|_| "Parent panicked")?;
        entry
            .recv_timeout(Duration::from_secs(5))
            .map_err(|e| e.to_string())?;
        assert!(
            scope.pending(),
            "Caller and parent returned, but child still owns the ticket"
        );
        finish.send(()).map_err(|e| e.to_string())?;
        child.join().map_err(|_| "Child panicked")??;
        assert!(!scope.pending());
        Ok(())
    }
}
