//! The taskbar button's progress: what the window's operations add up to, sent to the shell
//! only when it changes.

use std::time::{Duration, Instant};

/// What the taskbar button should show.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TaskbarState {
    Idle,
    /// A clone, fetch, pull or push runs; how far, when git says.
    Running(Option<f32>),
    /// The last one failed (until the window is looked at, or another one starts).
    Failed,
}

/// How often the progress is sent at most while it moves.
const SEND_EVERY: Duration = Duration::from_millis(250);

/// How often a state that shows something is sent again (Explorer restarted, the button
/// wasn't there yet when it was first sent).
const RESEND_EVERY: Duration = Duration::from_secs(5);

/// Works out the state each frame and sends it when it changes.
#[derive(Default)]
pub struct Taskbar {
    sent: Option<TaskbarState>,
    sent_at: Option<Instant>,
    /// An operation failed and the window hasn't been looked at since.
    failed: bool,
    /// Something ran on the last frame.
    was_running: bool,
}

impl Taskbar {
    /// The state for this frame: `running` the progress of each network operation running
    /// (none: idle), `failed_now` whether one just failed, `focused` whether the window is.
    /// A failure seen in the window it happened in isn't kept red: the user saw it.
    pub fn state(
        &mut self,
        running: &[Option<f32>],
        failed_now: bool,
        focused: bool,
    ) -> TaskbarState {
        let starting = !running.is_empty() && !self.was_running;
        self.was_running = !running.is_empty();
        if starting {
            self.failed = false;
        }
        if failed_now && !focused {
            self.failed = true;
        }
        if focused {
            self.failed = false;
        }
        if !running.is_empty() {
            // The least advanced, or unknown when any is.
            let known: Option<Vec<f32>> = running.iter().copied().collect();
            let least = known.and_then(|k| k.into_iter().reduce(f32::min));
            return TaskbarState::Running(least);
        }
        if self.failed {
            TaskbarState::Failed
        } else {
            TaskbarState::Idle
        }
    }

    /// Whether `state` is to be sent now: it changed (progress not more often than
    /// [`SEND_EVERY`]), or it shows something and was sent a while ago. Idle is not sent
    /// before anything else was: another gitcraft window's progress would be cleared.
    pub fn should_send(&self, state: TaskbarState) -> bool {
        let since = self.sent_at.map(|t| t.elapsed());
        match self.sent {
            None => state != TaskbarState::Idle,
            Some(sent) if sent == state => {
                state != TaskbarState::Idle && since.is_some_and(|d| d > RESEND_EVERY)
            }
            Some(TaskbarState::Running(_)) if matches!(state, TaskbarState::Running(_)) => {
                since.is_none_or(|d| d >= SEND_EVERY)
            }
            Some(_) => true,
        }
    }

    /// `state` reached the shell.
    pub fn sent(&mut self, state: TaskbarState) {
        self.sent = Some(state);
        self.sent_at = Some(Instant::now());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn running_operations_add_up_to_the_least_advanced() {
        let mut t = Taskbar::default();
        assert_eq!(t.state(&[], false, true), TaskbarState::Idle);
        assert_eq!(
            t.state(&[Some(0.4), Some(0.9)], false, true),
            TaskbarState::Running(Some(0.4))
        );
        assert_eq!(
            t.state(&[Some(0.4), None], false, true),
            TaskbarState::Running(None)
        );
    }

    #[test]
    fn a_failure_stays_red_until_the_window_is_looked_at() {
        let mut t = Taskbar::default();
        // It failed while the window was in the background.
        assert_eq!(t.state(&[], true, false), TaskbarState::Failed);
        assert_eq!(t.state(&[], false, false), TaskbarState::Failed);
        assert_eq!(t.state(&[], false, true), TaskbarState::Idle);
        // Or until another one starts.
        assert_eq!(t.state(&[], true, false), TaskbarState::Failed);
        assert_eq!(t.state(&[None], false, false), TaskbarState::Running(None));
        assert_eq!(t.state(&[], false, false), TaskbarState::Idle);
        // Seen in the window it happened in: not kept red.
        assert_eq!(t.state(&[], true, true), TaskbarState::Idle);
    }

    /// One tab's push failing while another tab fetches: red once the fetch is done.
    #[test]
    fn a_failure_while_another_runs_is_kept() {
        let mut t = Taskbar::default();
        assert_eq!(
            t.state(&[Some(0.5)], false, false),
            TaskbarState::Running(Some(0.5))
        );
        assert_eq!(
            t.state(&[Some(0.6)], true, false),
            TaskbarState::Running(Some(0.6))
        );
        assert_eq!(
            t.state(&[Some(0.7)], false, false),
            TaskbarState::Running(Some(0.7))
        );
        assert_eq!(t.state(&[], false, false), TaskbarState::Failed);
    }

    #[test]
    fn only_changes_are_sent_and_progress_not_too_often() {
        let mut t = Taskbar::default();
        // Nothing to clear yet: another window's progress stays.
        assert!(!t.should_send(TaskbarState::Idle));
        assert!(t.should_send(TaskbarState::Running(Some(0.1))));
        t.sent(TaskbarState::Running(Some(0.1)));
        assert!(!t.should_send(TaskbarState::Running(Some(0.1))));
        // Moving again at once: later.
        assert!(!t.should_send(TaskbarState::Running(Some(0.2))));
        // Ending is sent at once.
        assert!(t.should_send(TaskbarState::Failed));
        t.sent(TaskbarState::Failed);
        assert!(t.should_send(TaskbarState::Idle));
        // Not sent (the call failed): asked again next frame.
        assert!(t.should_send(TaskbarState::Idle));
    }
}
