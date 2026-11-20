//! The SonarQube quality gate of the checked-out branch or its pull request (issue #203,
//! slice 1): read in the background, re-evaluated against HEAD on every frame so that a moved
//! HEAD turns a result stale at once. Read-only; never sends repository contents.

mod binding;
mod chip;
#[cfg(test)]
mod tab_tests;
#[cfg(test)]
mod tests;

use std::time::{Duration, Instant};

use super::RepoTab;
use crate::analysis::gate::{Observation, Verdict};
use crate::analysis::{Failure, Provider, Query};
use binding::Binding;

/// How often a settled result is read again while the tab is shown.
const REFRESH: Duration = Duration::from_secs(300);
/// While an analysis is queued or running on the server.
const PENDING_POLL: Duration = Duration::from_secs(30);
/// After a failure (unreachable, refused…).
const RETRY: Duration = Duration::from_secs(120);
/// A request whose answer never came (the tab was suspended) frees its slot after this; the
/// requests themselves time out much sooner.
const LOST_AFTER: Duration = Duration::from_secs(180);
/// How often a paused network (offline, metered) is looked at again.
const PAUSED_POLL: Duration = Duration::from_secs(30);

/// One answer, with what it was asked for.
#[derive(Clone, Debug)]
struct Answer {
    query: Query,
    connection: String,
    /// HEAD when it was asked; a moved HEAD asks again (the badge re-evaluates at once).
    head: String,
    result: Result<Observation, Failure>,
    at: Instant,
}

#[derive(Default)]
pub(super) struct State {
    answer: Option<Answer>,
    /// The request on its way: its number and when it started.
    loading: Option<(u64, Instant)>,
    requests: u64,
    /// When to read again; `None` = as soon as possible.
    next: Option<Instant>,
}

/// What the toolbar shows.
#[derive(Clone, Debug, PartialEq)]
enum Badge {
    /// A connection exists but this repository has no project binding.
    Unbound,
    /// Bound, without a configured connection.
    NoConnection,
    /// HEAD is detached or unborn: no branch to ask about.
    NoTarget,
    Checking,
    Failed(Failure),
    Verdict(Verdict),
}

impl State {
    fn arrived(&mut self, number: u64, answer: Answer) {
        if self.loading.is_none_or(|(n, _)| n != number) {
            return;
        }
        self.loading = None;
        let wait = match &answer.result {
            Ok(o) if o.pending => PENDING_POLL,
            Ok(_) => REFRESH,
            Err(Failure::RateLimited(Some(s))) => Duration::from_secs((*s).clamp(60, 3600)),
            Err(Failure::RateLimited(None)) => REFRESH,
            Err(_) => RETRY,
        };
        self.next = Some(answer.at + wait);
        self.answer = Some(answer);
    }

    /// The answer for exactly this query and connection; anything else is not shown.
    fn current(&self, query: &Query, connection: &str) -> Option<&Answer> {
        self.answer
            .as_ref()
            .filter(|a| &a.query == query && a.connection == connection)
    }
}

impl RepoTab {
    /// The binding, connection and query for this checkout; `Err` is the badge without one.
    fn gate_context(&self, ctx: &egui::Context) -> Result<(Binding, Query, String), Option<Badge>> {
        let account = crate::connections::current().choices.sonar;
        let Some(binding) = Binding::load(ctx, &self.path) else {
            return Err(account.configured().then_some(Badge::Unbound));
        };
        if !account.configured() {
            return Err(Some(Badge::NoConnection));
        }
        let query = binding
            .query(self.refs.current.as_deref(), &self.prs)
            .ok_or(Some(Badge::NoTarget))?;
        Ok((binding, query, account.identity()))
    }

    fn quality_gate_badge(&self, ctx: &egui::Context) -> Option<Badge> {
        let (binding, query, connection) = match self.gate_context(ctx) {
            Ok(found) => found,
            Err(badge) => return badge,
        };
        if binding.connection != connection {
            return Some(Badge::Failed(Failure::BindingMismatch));
        }
        let head = Some(self.refs.head.as_str()).filter(|h| !h.is_empty());
        Some(match self.quality_gate.current(&query, &connection) {
            None if self.quality_gate.loading.is_none()
                && !crate::network::background_may_run() =>
            {
                Badge::Failed(Failure::Offline)
            }
            None => Badge::Checking,
            Some(Answer { result: Err(f), .. }) => Badge::Failed(f.clone()),
            Some(Answer { result: Ok(o), .. }) => {
                Badge::Verdict(crate::analysis::gate::evaluate(o, head))
            }
        })
    }

    /// Starts a read when one is due and allowed; called while the toolbar is painted.
    fn poll_quality_gate(&mut self, ctx: &egui::Context, asked: bool) {
        if let Some((_, started)) = self.quality_gate.loading {
            if started.elapsed() < LOST_AFTER {
                return;
            }
            self.quality_gate.loading = None;
        }
        let Ok((binding, query, connection)) = self.gate_context(ctx) else {
            return;
        };
        if binding.connection != connection {
            return;
        }
        // Unknown for this query, or asked while HEAD was elsewhere (a commit, a pull).
        let fresh = self
            .quality_gate
            .current(&query, &connection)
            .is_some_and(|a| a.head == self.refs.head);
        let due = self.quality_gate.next.is_none_or(|t| Instant::now() >= t);
        let allowed = if asked {
            crate::network::refused().is_none()
        } else {
            crate::network::background_may_run()
        };
        if !allowed {
            // Offline or metered: looked at again later, without repainting every frame.
            ctx.request_repaint_after(PAUSED_POLL);
            return;
        }
        if !(asked || due || !fresh) {
            if let Some(next) = self.quality_gate.next {
                ctx.request_repaint_after(next.saturating_duration_since(Instant::now()));
            }
            return;
        }
        self.read_quality_gate(ctx, query, connection);
    }

    fn read_quality_gate(&mut self, ctx: &egui::Context, query: Query, connection: String) {
        self.quality_gate.requests += 1;
        let number = self.quality_gate.requests;
        self.quality_gate.loading = Some((number, Instant::now()));
        let head = self.refs.head.clone();
        let (failed_query, failed_connection, failed_head) =
            (query.clone(), connection.clone(), head.clone());
        let failed = move |_: String| Answer {
            query: failed_query,
            connection: failed_connection,
            head: failed_head,
            result: Err(Failure::Busy),
            at: Instant::now(),
        };
        self.then_or(
            ctx,
            false,
            failed,
            move || {
                let result =
                    crate::analysis::sonar::live().and_then(|mut c| c.quality_gate(&query));
                Answer {
                    query,
                    connection,
                    head,
                    result,
                    at: Instant::now(),
                }
            },
            move |tab, answer, _| tab.quality_gate.arrived(number, answer),
        );
    }
}
