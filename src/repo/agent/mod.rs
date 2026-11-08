//! What agents ask a tab to do over `gitcraft mcp` (see `crate::mcp`): each request is planned
//! in the background ([`plan`]), waits its turn, and runs as the tab's own operations do, so
//! that it lands in the undo list, marked as the agent's. One that can't be undone waits on an
//! Approve / Deny card first (drawn by the app, see `app::agent_card`), for a while; then it is
//! denied automatically.

pub(crate) mod plan;
pub(crate) mod preview;

use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
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
    /// The agent stopped waiting for it (see `Incoming::gone`).
    gone: Arc<AtomicBool>,
    came: Instant,
}

impl Job {
    pub(crate) fn new(incoming: Incoming) -> Self {
        Self {
            agent: incoming.agent,
            op: incoming.op,
            reply: incoming.reply,
            gone: incoming.gone,
            came: Instant::now(),
        }
    }

    /// The agent no longer waits for it: nothing of it is done any more.
    fn abandoned(&self) -> bool {
        self.gone.load(Ordering::SeqCst)
    }

    fn answer(self, outcome: Outcome, message: impl Into<String>) {
        let _ = self.reply.send(Reply::new(outcome, message));
    }
}

/// A request planned, about to run or on its card.
struct Ready {
    job: Job,
    plan: Plan,
    preview: Option<preview::Preview>,
}

/// A request waiting for the user's Approve or Deny.
struct Waiting {
    ready: Ready,
    shown: Instant,
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
    /// Waiting their turn, not planned yet: each is planned when the tab is free for it, so that
    /// what it checks (a conflict, where a branch is) is the repository as it will run on.
    queue: VecDeque<Job>,
    /// A request being planned (on a background thread).
    planning: bool,
    card: Option<Waiting>,
    running: Option<Running>,
    /// How long a card waits ([`APPROVAL_WAIT`]; less in the tests).
    approval_wait: Duration,
}

impl Default for Agents {
    fn default() -> Self {
        Self {
            queue: VecDeque::new(),
            planning: false,
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
    pub preview: Option<preview::Preview>,
    pub ask: Ask,
    /// The git commands it runs, as they would be typed.
    pub commands: Vec<String>,
    /// Whether gitcraft can undo it once done.
    pub undoable: bool,
    pub left: Duration,
    /// How long it has been on screen: Approve takes no click at first, which was surely meant
    /// for what the card popped up over.
    pub shown_for: Duration,
}

/// `text` without the characters that change how text around them reads: bidirectional
/// overrides and isolates, zero-width ones, other format and control characters.
fn visible(text: &str) -> String {
    text.chars()
        .filter(|&c| {
            !(c.is_control()
                || matches!(c, '\u{200b}'..='\u{200f}' | '\u{202a}'..='\u{202e}'
                    | '\u{2060}'..='\u{2069}' | '\u{feff}' | '\u{061c}' | '\u{00ad}'))
        })
        .collect()
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
    pub(in crate::repo) fn operation_agent(&self, label: &str) -> Option<String> {
        self.agent_calls
            .running
            .as_ref()
            .filter(|r| r.label == label)
            .map(|r| r.agent.clone())
    }

    /// Takes a request from an agent: it waits its turn, and is planned once the tab is free.
    pub(crate) fn agent_request(&mut self, incoming: Incoming, ctx: &egui::Context) {
        self.agent_calls.queue.push_back(Job::new(incoming));
        self.agent_step(ctx);
    }

    /// Plans `job` in the background. `approved`: the card the user approved for it, which the
    /// plan made now must still show for it to run without asking again.
    fn agent_plan(&mut self, job: Job, approved: Option<Ask>, ctx: &egui::Context) {
        self.agent_calls.planning = true;
        let path = self.path.clone();
        self.then(
            ctx,
            move || {
                let planned = plan::plan(&path, &job.op);
                let preview = if planned.as_ref().is_ok_and(|p| p.ask.is_some()) {
                    preview::read(&path, &job.op, &|| job.abandoned())
                } else {
                    None
                };
                (job, planned, preview)
            },
            move |tab, (job, planned, preview), ctx| {
                tab.agent_planned(job, planned, preview, approved, ctx)
            },
        );
    }

    /// A request planned: run, shown on its card, or answered with why it can't be done.
    fn agent_planned(
        &mut self,
        job: Job,
        planned: Result<Plan, String>,
        preview: Option<preview::Preview>,
        approved: Option<Ask>,
        ctx: &egui::Context,
    ) {
        self.agent_calls.planning = false;
        if job.abandoned() {
            return self.agent_step(ctx);
        }
        let mut plan = match planned {
            Ok(plan) => plan,
            Err(e) => {
                job.answer(Outcome::Failed, e);
                return self.agent_step(ctx);
            }
        };
        // What can't be undone always asks, whatever the plan made of it.
        if job.op.needs_approval() && plan.ask.is_none() {
            plan.ask = Some(Ask {
                title: plan.label.clone(),
                body: plan.label.clone(),
                undo_note: crate::i18n::text("repo-workflows-agent-agent-planned-14b0fc42")
                    .to_owned(),
            });
        }
        if self.busy_op.is_some() || self.loading {
            // Something started meanwhile: planned again when it is done, its wait counted
            // from now (an approved request isn't dropped for the time its card was open).
            let mut job = job;
            job.came = Instant::now();
            self.agent_calls.queue.push_front(job);
            return;
        }
        if let Some(ask) = &mut plan.ask {
            use sha2::{Digest, Sha256};
            let guards: Vec<_> = plan
                .ops
                .iter()
                .filter(|s| s.first().is_some_and(|v| v.starts_with('\0')))
                .collect();
            let bytes = serde_json::to_vec(&guards).expect("string vectors serialize");
            let digest = Sha256::digest(bytes)
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>();
            ask.body
                .push_str(&format!("\nExact source guards: {digest}\n{guards:?}"));
        }
        match plan.ask.take() {
            // What the user approved is still what it would do.
            Some(ask) if approved.as_ref() == Some(&ask) => {
                self.agent_start(Ready { job, plan, preview }, ctx)
            }
            Some(ask) => {
                plan.ask = Some(ask);
                self.agent_calls.card = Some(Waiting {
                    ready: Ready { job, plan, preview },
                    shown: Instant::now(),
                    until: Instant::now() + self.agent_calls.approval_wait,
                });
                ctx.send_viewport_cmd(egui::ViewportCommand::RequestUserAttention(
                    egui::UserAttentionType::Informational,
                ));
                ctx.request_repaint();
            }
            None => self.agent_start(Ready { job, plan, preview }, ctx),
        }
    }

    /// Moves the requests along: a card that waited too long is denied, the next request is
    /// planned once the tab is free. Called on every poll.
    pub(super) fn agent_step(&mut self, ctx: &egui::Context) {
        let agents = &mut self.agent_calls;
        // The agent stopped waiting: its card goes away, its queued requests are dropped.
        if agents
            .card
            .as_ref()
            .is_some_and(|w| w.ready.job.abandoned())
        {
            agents.card = None;
            ctx.request_repaint();
        }
        agents.queue.retain(|job| !job.abandoned());
        if agents
            .card
            .as_ref()
            .is_some_and(|w| Instant::now() >= w.until)
            && let Some(waiting) = agents.card.take()
        {
            waiting.ready.job.answer(
                Outcome::TimedOut,
                crate::i18n::message(
                    "repo-workflows-agent-agent-step-97d87ec1",
                    &[(
                        "agents-approval-wait-as-secs-1",
                        (agents.approval_wait.as_secs()).clone().into(),
                    )],
                ),
            );
        }
        while agents
            .queue
            .front()
            .is_some_and(|job| job.came.elapsed() > QUEUE_WAIT)
        {
            if let Some(stale) = agents.queue.pop_front() {
                stale.answer(
                    Outcome::TimedOut,
                    crate::i18n::message(
                        "repo-workflows-agent-agent-step-c03ca088",
                        &[(
                            "queue-wait-as-secs-60-1",
                            (QUEUE_WAIT.as_secs() / 60).clone().into(),
                        )],
                    ),
                );
            }
        }
        if agents.card.is_some() {
            // The countdown, and the end of the wait.
            ctx.request_repaint_after(Duration::from_secs(1));
            return;
        }
        if agents.planning || agents.running.is_some() || self.busy_op.is_some() || self.loading {
            return;
        }
        if let Some(job) = self.agent_calls.queue.pop_front() {
            self.agent_plan(job, None, ctx);
        }
    }

    fn agent_start(&mut self, ready: Ready, ctx: &egui::Context) {
        let Ready { job, plan, .. } = ready;
        let before = self.refs.head.clone();
        self.run_ops(
            &plan.label,
            plan.ops,
            plan.refresh,
            OpKind::Normal(plan.undo, before),
            ctx,
        );
        if self.busy_op.as_deref() == Some(plan.label.as_str()) {
            self.agent_calls.running = Some(Running {
                label: plan.label,
                agent: job.agent,
                reply: job.reply,
            });
        } else {
            job.answer(
                Outcome::Failed,
                crate::i18n::text("repo-workflows-agent-agent-start-917ed8ba"),
            );
        }
    }

    /// The card waiting for the user, if there is one. What the agent wrote into it (a branch
    /// name, a commit's summary) is shown without characters that reorder or hide text.
    pub(crate) fn agent_card(&self) -> Option<CardView> {
        let waiting = self.agent_calls.card.as_ref()?;
        let plan = &waiting.ready.plan;
        let ask = plan.ask.as_ref()?;
        Some(CardView {
            agent: visible(&waiting.ready.job.agent),
            preview: waiting.ready.preview.clone(),
            ask: Ask {
                title: visible(&ask.title),
                body: visible(&ask.body),
                undo_note: visible(&ask.undo_note),
            },
            commands: plan
                .ops
                .iter()
                .filter_map(|op| shown_command(op))
                .map(|c| visible(&c))
                .collect(),
            undoable: plan.undo != Undo::None,
            left: waiting.until.saturating_duration_since(Instant::now()),
            shown_for: waiting.shown.elapsed(),
        })
    }

    /// The user's answer to the card. Approved, the request is planned again, and runs if it
    /// still does what the card said; if the repository changed meanwhile, its new card shows.
    pub(crate) fn agent_decide(&mut self, approve: bool, ctx: &egui::Context) {
        let Some(waiting) = self.agent_calls.card.take() else {
            return;
        };
        let Ready { job, plan, .. } = waiting.ready;
        if !approve {
            job.answer(
                Outcome::Denied,
                crate::i18n::text("repo-workflows-agent-agent-decide-7695bed1"),
            );
            return;
        }
        self.agent_plan(job, plan.ask, ctx);
    }

    /// An operation ended (see `op_done`): when it was an agent's, its undo step is marked as
    /// the agent's and the agent is told how it went. `undoable`: a step went on the undo list.
    pub(super) fn agent_op_done(
        &mut self,
        label: &str,
        result: &Result<String, String>,
        undoable: bool,
    ) {
        let Some(running) = self.agent_calls.running.take_if(|r| r.label == label) else {
            return;
        };
        let reply = match result {
            Ok(_) => {
                self.status = Some((
                    crate::i18n::message(
                        "repo-workflows-agent-agent-op-done-16d1e9b2",
                        &[
                            ("running-agent-1", (running.agent).clone().into()),
                            ("label", (label).into()),
                        ],
                    ),
                    false,
                ));
                let how = if undoable {
                    crate::i18n::text("repo-workflows-agent-agent-op-done-d5e3a3af")
                } else {
                    crate::i18n::text("repo-workflows-agent-agent-op-done-cf3e44cf")
                };
                Reply::new(
                    Outcome::Done,
                    crate::i18n::message(
                        "repo-workflows-agent-agent-op-done-79a8519c",
                        &[("label", (label).into()), ("how", (how).into())],
                    ),
                )
            }
            Err(e) => Reply::new(
                Outcome::Failed,
                crate::i18n::message(
                    "repo-workflows-agent-agent-op-done-8336fbfe",
                    &[("label", (label).into()), ("e", (e).clone().into())],
                ),
            ),
        };
        let _ = running.reply.send(reply);
    }

    /// An agent's operation needs the user to sign in first: the window asks them for it, and the
    /// agent is told to try again after.
    pub(super) fn agent_needs_sign_in(&mut self, label: &str) {
        if let Some(running) = self.agent_calls.running.take_if(|r| r.label == label) {
            let _ = running.reply.send(Reply::new(
                Outcome::Failed,
                crate::i18n::message(
                    "repo-workflows-agent-agent-needs-sign-in-dc823c53",
                    &[("label", (label).into())],
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

    #[test]
    fn what_an_agent_wrote_cant_reorder_the_card() {
        assert_eq!(visible("evil\u{202e}txt.exe"), "eviltxt.exe");
        assert_eq!(visible("a\u{200b}b\u{2066}c\u{feff}"), "abc");
        assert_eq!(visible("line\nbreak"), "linebreak");
        assert_eq!(visible("plain “quoted” text"), "plain “quoted” text");
    }
}
