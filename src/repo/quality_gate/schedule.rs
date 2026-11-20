//! When the quality gate is read again: a settled result every few minutes, a pending analysis
//! and a failing server with exponential backoff. After a failure nothing is read before the
//! backoff (or the server's `Retry-After`) ends, not even for another branch or a moved HEAD,
//! unless the user asks for a refresh.

use std::time::{Duration, Instant};

use crate::analysis::Failure;
use crate::analysis::gate::Observation;

/// How often a settled result is read again while the tab is shown.
pub(super) const REFRESH: Duration = Duration::from_secs(300);
/// The first wait while an analysis is queued or running, and after a failure; doubled each
/// further time.
pub(super) const BACKOFF_BASE: Duration = Duration::from_secs(30);
/// The longest backoff.
pub(super) const BACKOFF_CAP: Duration = Duration::from_secs(600);
/// The shortest and longest waits a `Retry-After` is held to.
const RETRY_AFTER_MIN: u64 = 60;
const RETRY_AFTER_MAX: u64 = 3600;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct Schedule {
    /// When the current answer is read again; `None` = as soon as possible.
    pub next: Option<Instant>,
    /// After a failure: no read before this, whatever changed, unless asked for.
    pub not_before: Option<Instant>,
    failures: u32,
    pending: u32,
}

/// `BACKOFF_BASE` doubled `streak - 1` times, at most `BACKOFF_CAP`.
pub(super) fn backoff(streak: u32) -> Duration {
    let doublings = streak.saturating_sub(1).min(16);
    BACKOFF_BASE
        .saturating_mul(1_u32 << doublings)
        .min(BACKOFF_CAP)
}

impl Schedule {
    /// Plans the next read after an answer that arrived `at`.
    pub(super) fn answered(&mut self, at: Instant, result: &Result<Observation, Failure>) {
        match result {
            Ok(observation) => {
                self.failures = 0;
                self.not_before = None;
                self.pending = if observation.pending {
                    self.pending.saturating_add(1)
                } else {
                    0
                };
                let wait = if observation.pending {
                    backoff(self.pending)
                } else {
                    REFRESH
                };
                self.next = Some(at + wait);
            }
            Err(failure) => {
                self.failures = self.failures.saturating_add(1);
                let mut wait = backoff(self.failures);
                if let Failure::RateLimited(Some(seconds)) = failure {
                    let asked = (*seconds).clamp(RETRY_AFTER_MIN, RETRY_AFTER_MAX);
                    wait = wait.max(Duration::from_secs(asked));
                }
                self.next = Some(at + wait);
                self.not_before = self.next;
            }
        }
    }

    /// Whether a read may start now. `stale`: the answer is for another query or HEAD.
    pub(super) fn due(&self, now: Instant, asked: bool, stale: bool) -> bool {
        if asked {
            return true;
        }
        if self.not_before.is_some_and(|t| now < t) {
            return false;
        }
        stale || self.next.is_none_or(|t| now >= t)
    }
}
