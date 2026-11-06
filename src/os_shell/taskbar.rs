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

/// Works out the state each frame and sends it when it changes.
#[derive(Default)]
pub struct Taskbar {
    sent: Option<TaskbarState>,
    sent_at: Option<Instant>,
    /// An operation failed and the window hasn't been looked at since.
    failed: bool,
}

impl Taskbar {
    /// The state for this frame: `running` the progress of each network operation running
    /// (none: idle), `failed_now` whether one just failed, `focused` whether the window is.
    pub fn state(
        &mut self,
        running: &[Option<f32>],
        failed_now: bool,
        focused: bool,
    ) -> TaskbarState {
        if failed_now {
            self.failed = true;
        }
        if !running.is_empty() {
            self.failed = false;
            // The least advanced, or unknown when any is.
            let known: Option<Vec<f32>> = running.iter().copied().collect();
            let least = known.and_then(|k| k.into_iter().reduce(f32::min));
            return TaskbarState::Running(least);
        }
        if self.failed && focused && !failed_now {
            self.failed = false;
        }
        if self.failed {
            TaskbarState::Failed
        } else {
            TaskbarState::Idle
        }
    }

    /// Whether `state` is to be sent now: it changed, and progress not more often than
    /// [`SEND_EVERY`].
    pub fn should_send(&mut self, state: TaskbarState) -> bool {
        if self.sent == Some(state) {
            return false;
        }
        let moving = matches!(
            (self.sent, state),
            (Some(TaskbarState::Running(_)), TaskbarState::Running(_))
        );
        if moving && self.sent_at.is_some_and(|t| t.elapsed() < SEND_EVERY) {
            return false;
        }
        self.sent = Some(state);
        self.sent_at = Some(Instant::now());
        true
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
        // A failure seen in the window it happened in still shows a frame.
        assert_eq!(t.state(&[], true, true), TaskbarState::Failed);
    }

    #[test]
    fn only_changes_are_sent_and_progress_not_too_often() {
        let mut t = Taskbar::default();
        assert!(t.should_send(TaskbarState::Idle));
        assert!(!t.should_send(TaskbarState::Idle));
        assert!(t.should_send(TaskbarState::Running(Some(0.1))));
        // Moving again at once: later.
        assert!(!t.should_send(TaskbarState::Running(Some(0.2))));
        // Ending is sent at once.
        assert!(t.should_send(TaskbarState::Failed));
    }
}
