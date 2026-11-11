//! Bounded output retention and cancellation lifetime for an owned bisect run.

use super::*;

/// A `git bisect run` and what it printed.
pub(super) struct RunLog {
    pub(super) command: String,
    /// Its lines, filled from the thread that runs it.
    pub(super) lines: Arc<Mutex<VecDeque<String>>>,
    pub(super) stopper: git::Stopper,
    pub(super) started: Instant,
    /// How it ended, once it has.
    pub(super) ended: Option<Result<git::Ended, String>>,
    /// Its output is shown.
    pub(super) open: bool,
}

impl RunLog {
    pub(super) fn new(command: String) -> Self {
        Self {
            command,
            lines: Arc::default(),
            stopper: git::Stopper::default(),
            started: Instant::now(),
            ended: None,
            open: true,
        }
    }
}

/// A run still going when its tab closes is stopped: nothing would show its end.
impl Drop for RunLog {
    fn drop(&mut self) {
        if self.ended.is_none() {
            self.stopper.stop();
        }
    }
}

/// Adds `line` to `lines`, the oldest let go of past [`KEPT_LINES`].
pub(super) fn keep_line(lines: &Mutex<VecDeque<String>>, line: String) {
    if let Ok(mut lines) = lines.lock() {
        if lines.len() >= KEPT_LINES {
            lines.pop_front();
        }
        lines.push_back(line);
    }
}
