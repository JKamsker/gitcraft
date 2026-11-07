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
    pub(super) fn not_dispatched(&self) {
        self.state(|state| match state.phase {
            Phase::Queued => {
                state.phase = if state.cancel_requested {
                    Phase::Cancelled
                } else {
                    Phase::NotDispatched
                }
            }
            Phase::Running => state.phase = Phase::Unknown,
            _ => {}
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
    if owned {
        return FailureAction::OwnedUnknown;
    }
    match failure {
        Failure::NotSent(_) if reused => FailureAction::Retry,
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
    fn recording_only_pre_dispatch_refusal_can_retire_ownership() {
        let owner = fixture();
        owner.not_dispatched();
        assert_eq!(owner.phase(), Phase::NotDispatched);
        assert!(!owner.begin());
        let running = fixture();
        assert!(running.begin());
        running.not_dispatched();
        assert_eq!(running.phase(), Phase::Unknown);
        assert!(!running.begin());
        let cancelled = fixture();
        cancelled.cancel();
        cancelled.not_dispatched();
        assert_eq!(cancelled.phase(), Phase::Cancelled);
        assert!(!cancelled.begin());
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
                assert_eq!(action, FailureAction::OwnedUnknown);
                println!(
                    "owned reused={reused}: {action:?}; selected_stop_commands=0 selected_retries=0"
                );
            }
        }
        assert_eq!(
            failure_action(&Failure::TimedOut(Some(424242)), false, false),
            FailureAction::LegacyTimeout(Some(424242))
        );
    }
    #[test]
    fn recording_partial_and_flush_write_failures_are_unknown_without_retry() {
        use std::io::{self, Write};
        struct Writer {
            bytes: Vec<u8>,
            limit: usize,
            fail_flush: bool,
        }
        impl Write for Writer {
            fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
                let count = bytes.len().min(self.limit.saturating_sub(self.bytes.len()));
                if count == 0 {
                    return Err(io::Error::new(
                        io::ErrorKind::BrokenPipe,
                        "recording write failure",
                    ));
                }
                self.bytes.extend_from_slice(&bytes[..count]);
                Ok(count)
            }
            fn flush(&mut self) -> io::Result<()> {
                if self.fail_flush {
                    Err(io::Error::new(
                        io::ErrorKind::BrokenPipe,
                        "recording flush failure",
                    ))
                } else {
                    Ok(())
                }
            }
        }
        let frame = "{\nprintf fixture\n}\n";
        for (limit, flush) in [(0, false), (frame.len() - 1, false), (usize::MAX, true)] {
            let mut writer = Writer {
                bytes: Vec::new(),
                limit,
                fail_flush: flush,
            };
            let error = super::super::shell::write_request(&mut writer, frame).err();
            assert!(error.is_some());
            if limit == frame.len() - 1 {
                assert!(writer.bytes.ends_with(b"}"));
            }
            let failure =
                super::super::shell::Failure::NotSent("recorded real writer error".into());
            for reused in [false, true] {
                assert_eq!(
                    failure_action(&failure, reused, true),
                    FailureAction::OwnedUnknown
                );
            }
            println!(
                "write bytes={} flush={flush}: Unknown; selected_retries=0 selected_stop_commands=0",
                writer.bytes.len()
            );
        }
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
