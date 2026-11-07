//! Dispatch/outcome ownership for explicit mutations. No process selection or signalling.
//! A lost connection is unknown, never proof that the host stopped mutating.
use std::sync::{Arc, Mutex};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Phase {
    #[default]
    Queued,
    Running,
    Cancelled,
    NotDispatched,
    Completed,
    Unknown,
}
#[derive(Default)]
struct State {
    phase: Phase,
    cancel_requested: bool,
}

#[derive(Clone)]
pub(crate) struct OwnedDispatch {
    state: Arc<Mutex<State>>,
    context: crate::connections::Context,
    completion: crate::git::ForecastCompletion,
}
impl Default for OwnedDispatch {
    fn default() -> Self {
        Self {
            state: Arc::default(),
            context: crate::connections::current(),
            completion: Default::default(),
        }
    }
}
impl OwnedDispatch {
    fn state<T>(&self, job: impl FnOnce(&mut State) -> T) -> T {
        job(&mut self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner))
    }
    pub fn phase(&self) -> Phase {
        self.state(|state| state.phase)
    }
    pub fn pending(&self) -> bool {
        self.completion.pending()
    }
    pub fn cancel(&self) {
        self.state(|state| {
            state.cancel_requested = true;
            if state.phase == Phase::Queued {
                state.phase = Phase::Cancelled;
            }
        });
    }
    pub(super) fn begin(&self) -> bool {
        self.state(|state| {
            if state.phase != Phase::Queued || state.cancel_requested {
                return false;
            }
            state.phase = Phase::Running;
            true
        })
    }
    pub(super) fn not_sent(&self, retry: bool) {
        self.state(|state| {
            state.phase = if state.cancel_requested {
                Phase::Cancelled
            } else if retry {
                Phase::Queued
            } else {
                Phase::NotDispatched
            }
        });
    }
    pub(super) fn completed(&self) {
        self.state(|state| state.phase = Phase::Completed);
    }
    pub(super) fn unknown(&self) {
        self.state(|state| state.phase = Phase::Unknown);
    }
    pub(super) fn context_error(&self) -> Option<String> {
        self.context.error.clone()
    }
    pub(crate) fn run<T>(&self, job: impl FnOnce() -> T) -> T {
        self.context.run(|| self.completion.track(job))
    }
}
#[derive(Debug)]
pub(crate) enum OwnedFailure {
    NotDispatched(String),
    Unknown,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum FailureAction {
    Retry,
    OwnedNotSent,
    OwnedUnknown,
    LegacyTimeout(Option<u32>),
    LegacyError,
}
pub(super) fn failure_action(
    failure: &super::shell::Failure,
    reused: bool,
    owned: bool,
) -> FailureAction {
    use super::shell::Failure;
    match failure {
        Failure::NotSent(_) if reused => FailureAction::Retry,
        Failure::NotSent(_) if owned => FailureAction::OwnedNotSent,
        Failure::Lost(_) | Failure::TimedOut(_) if owned => FailureAction::OwnedUnknown,
        Failure::TimedOut(pid) => FailureAction::LegacyTimeout(*pid),
        _ => FailureAction::LegacyError,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> OwnedDispatch {
        crate::connections::Context::default().run(OwnedDispatch::default)
    }
    #[test]
    fn recording_cancelled_queued_work_cannot_dispatch() {
        let owner = fixture();
        owner.cancel();
        assert_eq!(owner.phase(), Phase::Cancelled);
        assert!(!owner.begin());
        assert!(!owner.pending());
    }
    #[test]
    fn recording_cancellation_after_dispatch_never_claims_stopped() {
        let owner = fixture();
        assert!(owner.begin());
        owner.cancel();
        assert_eq!(owner.phase(), Phase::Running);
        assert!(!owner.begin());
        owner.unknown();
        assert_eq!(owner.phase(), Phase::Unknown);
        assert!(!owner.begin());
    }
    #[test]
    fn recording_not_sent_retry_obeys_cancellation_and_only_retries_unsent() {
        let owner = fixture();
        assert!(owner.begin());
        owner.not_sent(true);
        assert_eq!(owner.phase(), Phase::Queued);
        assert!(owner.begin());
        owner.cancel();
        owner.not_sent(true);
        assert_eq!(owner.phase(), Phase::Cancelled);
        assert!(!owner.begin());
        let other = fixture();
        assert!(other.begin());
        other.not_sent(false);
        assert_eq!(other.phase(), Phase::NotDispatched);
    }
    #[test]
    fn recording_owned_failures_never_select_the_legacy_stop_controller() {
        use super::super::shell::Failure;
        for reused in [false, true] {
            for failure in [
                Failure::NotSent("fixture".into()),
                Failure::Lost("fixture".into()),
                Failure::TimedOut(None),
                Failure::TimedOut(Some(424242)),
            ] {
                let action = failure_action(&failure, reused, true);
                assert!(!matches!(
                    action,
                    FailureAction::LegacyTimeout(_) | FailureAction::LegacyError
                ));
                println!("owned reused={reused}: {action:?}; selected_stop_commands=0");
            }
        }
        assert_eq!(
            failure_action(&Failure::TimedOut(Some(424242)), false, false),
            FailureAction::LegacyTimeout(Some(424242))
        );
    }
    #[test]
    fn recording_scope_retains_context_and_ticket_for_the_whole_owned_wait() -> Result<(), String> {
        use std::{sync::mpsc, time::Duration};
        let context = crate::connections::Context {
            choices: crate::connections::Choices {
                ssh_key: "/recording-only/captured-profile".into(),
                ..Default::default()
            },
            ..Default::default()
        };
        let owner = context.run(OwnedDispatch::default);
        let worker = owner.clone();
        let expected = context.clone();
        let (entered, entry) = mpsc::channel();
        let (finish, finished) = mpsc::channel();
        let thread = std::thread::spawn(move || {
            worker.run(|| -> Result<(), String> {
                assert_eq!(crate::connections::current(), expected);
                assert!(worker.pending());
                entered.send(()).map_err(|e| e.to_string())?;
                finished
                    .recv_timeout(Duration::from_secs(5))
                    .map_err(|e| e.to_string())?;
                Ok(())
            })
        });
        entry
            .recv_timeout(Duration::from_secs(5))
            .map_err(|e| e.to_string())?;
        assert!(owner.pending());
        finish.send(()).map_err(|e| e.to_string())?;
        thread.join().map_err(|_| "Recording worker panicked")??;
        assert!(!owner.pending());
        Ok(())
    }
    #[test]
    fn recording_terminal_receipt_is_distinct_from_transport_uncertainty() {
        let owner = fixture();
        assert!(owner.begin());
        owner.completed();
        assert_eq!(owner.phase(), Phase::Completed);
        assert!(!owner.begin());
        let other = fixture();
        assert!(other.begin());
        other.unknown();
        assert_ne!(other.phase(), Phase::Completed);
        assert!(!other.begin());
    }
}
