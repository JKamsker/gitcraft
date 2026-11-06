//! What agents ask a tab to do over `gitcraft mcp` (see `crate::mcp`): each request is planned
//! in the background ([`plan`]), waits its turn, and runs as the tab's own operations do, so
//! that it lands in the undo list, marked as the agent's. One that can't be undone waits on an
//! Approve / Deny card first (drawn by the app, see `app::agent_card`), for a while; then it is
//! denied by itself.

pub(crate) mod plan;

use std::collections::VecDeque;
use std::sync::mpsc::Sender;
use std::time::{Duration, Instant};

use super::{OpKind, RepoTab, Undo};
use crate::mcp::link::Incoming;
use crate::mcp::wire::{Outcome, Reply, WriteOp};
use plan::{Ask, Plan};

/// How long an approval card waits for the user.
pub(crate) const APPROVAL_WAIT: Duration = Duration::from_secs(120);

/// How long a request waits for the tab to be free (another operation running, a card open).
const QUEUE_WAIT: Duration = Duration::from_secs(5 * 60);

/// One request, from the agent `agent`, answered through `reply`.
pub(crate) struct Job {
    pub(crate) agent: String,
    pub(crate) op: WriteOp,
    reply: Sender<Reply>,
    came: Instant,
}

impl Job {
    pub(crate) fn new(agent: String, op: WriteOp, reply: Sender<Reply>) -> Self {
        Self {
            agent,
            op,
            reply,
            came: Instant::now(),
        }
    }

    fn answer(self, outcome: Outcome, message: impl Into<String>) {
        let _ = self.reply.send(Reply::new(outcome, message));
    }
}

/// A request planned, waiting to run.
struct Ready {
    job: Job,
    plan: Plan,
}

/// A request waiting for the user's Approve or Deny.
struct Waiting {
    ready: Ready,
    until: Instant,
}

/// A request's operation running.
struct Running {
    label: String,
    agent: String,
    reply: Sender<Reply>,
}

/// The tab's agents' requests.
pub(crate) struct Agents {
    queue: VecDeque<Ready>,
    card: Option<Waiting>,
    running: Option<Running>,
    /// How long a card waits ([`APPROVAL_WAIT`]; less in the tests).
    approval_wait: Duration,
}

impl Default for Agents {
    fn default() -> Self {
        Self {
            queue: VecDeque::new(),
            card: None,
            running: None,
            approval_wait: APPROVAL_WAIT,
        }
    }
}

/// What the approval card shows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CardView {
    pub agent: String,
    pub ask: Ask,
    /// The git commands it runs, as they would be typed.
    pub commands: Vec<String>,
    /// Whether gitcraft can undo it once done.
    pub undoable: bool,
    pub left: Duration,
}

/// `args` as typed in a shell, quoted where it needs it; `None` for a step that runs nothing (a
/// check that a ref hasn't moved).
fn shown_command(args: &[String]) -> Option<String> {
    if args.first().is_some_and(|a| a.starts_with('\u{0}')) {
        return None;
    }
    let words: Vec<String> = std::iter::once("git".to_owned())
        .chain(args.iter().map(|a| {
            let plain = !a.is_empty()
                && a.chars()
                    .all(|c| c.is_ascii_alphanumeric() || "-_./:=@^~{}+,".contains(c));
            if plain {
                a.clone()
            } else {
                format!("'{}'", a.replace('\'', "'\\''"))
            }
        }))
        .collect();
    Some(words.join(" "))
}

impl RepoTab {
    /// Takes a request from an agent: planned in the background, then queued.
    pub(crate) fn agent_request(&mut self, incoming: Incoming, ctx: &egui::Context) {
        let job = Job::new(incoming.agent, incoming.op, incoming.reply);
        let path = self.path.clone();
        self.then(
            ctx,
            move || {
                let planned = plan::plan(&path, &job.op);
                (job, planned)
            },
            |tab, (job, planned), ctx| tab.agent_planned(job, planned, ctx),
        );
    }

    /// A request planned: queued, or answered with why it can't be done.
    fn agent_planned(&mut self, job: Job, planned: Result<Plan, String>, ctx: &egui::Context) {
        match planned {
            Ok(mut plan) => {
                // What can't be undone always asks, whatever the plan made of it.
                if job.op.needs_approval() && plan.ask.is_none() {
                    plan.ask = Some(Ask {
                        title: plan.label.clone(),
                        body: "This can't be undone from gitcraft.".to_owned(),
                    });
                }
                self.agents.queue.push_back(Ready { job, plan });
                self.agent_step(ctx);
            }
            Err(e) => job.answer(Outcome::Failed, e),
        }
    }

    /// Moves the requests along: a card that waited too long is denied, the next request
    /// starts (or shows its card) once the tab is free. Called on every poll.
    pub(super) fn agent_step(&mut self, ctx: &egui::Context) {
        let agents = &mut self.agents;
        if agents
            .card
            .as_ref()
            .is_some_and(|w| Instant::now() >= w.until)
            && let Some(waiting) = agents.card.take()
        {
            waiting.ready.job.answer(
                Outcome::TimedOut,
                format!(
                    "Nobody approved it in gitcraft within {} seconds, so nothing was done.",
                    agents.approval_wait.as_secs()
                ),
            );
        }
        while agents
            .queue
            .front()
            .is_some_and(|r| r.job.came.elapsed() > QUEUE_WAIT)
        {
            if let Some(stale) = agents.queue.pop_front() {
                stale.job.answer(
                    Outcome::TimedOut,
                    "gitcraft was busy with other work the whole time; nothing was done",
                );
            }
        }
        if agents.card.is_some() {
            // The countdown, and the end of the wait.
            ctx.request_repaint_after(Duration::from_secs(1));
            return;
        }
        if agents.running.is_some() || self.busy_op.is_some() || self.loading {
            return;
        }
        let Some(ready) = self.agents.queue.pop_front() else {
            return;
        };
        if ready.plan.ask.is_some() {
            self.agents.card = Some(Waiting {
                ready,
                until: Instant::now() + self.agents.approval_wait,
            });
            ctx.send_viewport_cmd(egui::ViewportCommand::RequestUserAttention(
                egui::UserAttentionType::Informational,
            ));
            ctx.request_repaint();
        } else {
            self.agent_start(ready, ctx);
        }
    }

    fn agent_start(&mut self, ready: Ready, ctx: &egui::Context) {
        let Ready { job, plan } = ready;
        let before = self.refs.head.clone();
        self.run_ops(
            &plan.label,
            plan.ops,
            plan.refresh,
            OpKind::Normal(plan.undo, before),
            ctx,
        );
        if self.busy_op.as_deref() == Some(plan.label.as_str()) {
            self.agents.running = Some(Running {
                label: plan.label,
                agent: job.agent,
                reply: job.reply,
            });
        } else {
            job.answer(Outcome::Failed, "gitcraft couldn't start it: try again");
        }
    }

    /// The card waiting for the user, if there is one.
    pub(crate) fn agent_card(&self) -> Option<CardView> {
        let waiting = self.agents.card.as_ref()?;
        let plan = &waiting.ready.plan;
        Some(CardView {
            agent: waiting.ready.job.agent.clone(),
            ask: plan.ask.clone()?,
            commands: plan.ops.iter().filter_map(|op| shown_command(op)).collect(),
            undoable: plan.undo != Undo::None,
            left: waiting.until.saturating_duration_since(Instant::now()),
        })
    }

    /// The user's answer to the card: runs it, or tells the agent no.
    pub(crate) fn agent_decide(&mut self, approve: bool, ctx: &egui::Context) {
        let Some(waiting) = self.agents.card.take() else {
            return;
        };
        let mut ready = waiting.ready;
        if !approve {
            ready.job.answer(Outcome::Denied, "The user chose Deny.");
            return;
        }
        // Approved: it runs next, without asking again.
        ready.plan.ask = None;
        self.agents.queue.push_front(ready);
        self.agent_step(ctx);
    }

    /// An operation ended (see `op_done`): when it was an agent's, its undo step is marked as
    /// the agent's and the agent is told how it went. `undoable`: a step went on the undo list.
    pub(super) fn agent_op_done(
        &mut self,
        label: &str,
        result: &Result<String, String>,
        undoable: bool,
    ) {
        let Some(running) = self.agents.running.take_if(|r| r.label == label) else {
            return;
        };
        let reply = match result {
            Ok(_) => {
                if undoable && let Some(entry) = self.undo_stack.last_mut() {
                    entry.agent = Some(running.agent.clone());
                }
                self.status = Some((format!("{}: {label}: done", running.agent), false));
                let how = if undoable {
                    "It is the latest step of gitcraft's undo list: the user can undo it with \
                     Ctrl+Z."
                } else {
                    "It can't be undone from gitcraft."
                };
                Reply::new(Outcome::Done, format!("{label}: done. {how}"))
            }
            Err(e) => Reply::new(Outcome::Failed, format!("{label} failed: {e}")),
        };
        let _ = running.reply.send(reply);
    }

    /// An agent's operation needs the user to sign in first: the window asks them for it, and the
    /// agent is told to try again after.
    pub(super) fn agent_needs_sign_in(&mut self, label: &str) {
        if let Some(running) = self.agents.running.take_if(|r| r.label == label) {
            let _ = running.reply.send(Reply::new(
                Outcome::Failed,
                format!(
                    "{label} needs the user to sign in to the remote: gitcraft asked them for \
                     it. Try again once they have."
                ),
            ));
        }
    }
}

#[cfg(test)]
mod flow_tests;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commands_read_as_typed_and_checks_are_left_out() {
        let shown = |args: &[&str]| {
            shown_command(&args.iter().map(|a| (*a).to_owned()).collect::<Vec<_>>())
        };
        assert_eq!(
            shown(&["branch", "-D", "--", "feature/x"]).as_deref(),
            Some("git branch -D -- feature/x")
        );
        assert_eq!(
            shown(&["commit", "-m", "it's done"]).as_deref(),
            Some("git commit -m 'it'\\''s done'")
        );
        assert_eq!(shown(&["\u{0}expect", "refs/heads/x", "abc"]), None);
    }
}
