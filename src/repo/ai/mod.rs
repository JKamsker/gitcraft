//! The AI tool in a repository tab (see [`crate::ai`]): the commit box's button that has it
//! write the message ([`message`]), and the panel with its review of a branch ([`review`]).
//! Both run in the background; a newer request, or Cancel, leaves an older answer unused.

mod message;
mod review;
#[cfg(all(test, unix))]
mod tests;

use std::time::Instant;

use crate::ai::detect::Look;
use crate::ai::{AiTool, Cancel};

/// What the tab's AI work is at.
#[derive(Default)]
pub(in crate::repo) struct AiState {
    /// A commit message being written.
    writing: Option<Running>,
    /// A message written while the box held one already: (summary, description), until the
    /// user picks one.
    suggestion: Option<(String, String)>,
    /// What to say under the message: (text, whether it is an error).
    note: Option<(String, bool)>,
    review: Option<review::Review>,
    /// The tools on the repository's machine, looked for when the tool menu first opens.
    tools: Look,
    /// Numbers the requests, so that an answer is only taken for the latest.
    requests: u64,
}

/// A request the tool is working on.
struct Running {
    id: u64,
    tool: AiTool,
    cancel: Cancel,
    started: Instant,
}

impl AiState {
    /// A new request with `tool`: its number, and what stops it.
    fn start(&mut self, tool: AiTool) -> Running {
        self.requests += 1;
        Running {
            id: self.requests,
            tool,
            cancel: Cancel::new(),
            started: Instant::now(),
        }
    }
}

/// A tab closed (or a worktree view dropped) stops what its tool was doing.
impl Drop for AiState {
    fn drop(&mut self) {
        if let Some(running) = &self.writing {
            running.cancel.cancel();
        }
        if let Some(review) = &self.review {
            review.cancel();
        }
    }
}

/// "12 s", "1 min 5 s": how long a request has been running.
fn elapsed(since: Instant) -> String {
    let secs = since.elapsed().as_secs();
    if secs < 60 {
        crate::i18n::message(
            "repo-workflows-ai-elapsed-127483e6",
            &[("secs", (secs).clone().into())],
        )
    } else {
        crate::i18n::message(
            "repo-workflows-ai-elapsed-fba4d59d",
            &[
                ("secs-60-1", (secs / 60).clone().into()),
                ("secs-60-2", (secs % 60).clone().into()),
            ],
        )
    }
}

/// What the tool saw of the diff, when it didn't see all of it.
fn coverage(files: usize, left_out: usize) -> Option<String> {
    (left_out > 0).then(|| {
        let whole = files.saturating_sub(left_out);
        crate::i18n::message(
            "repo-workflows-ai-coverage",
            &[("whole", (whole).into()), ("files", (files).into())],
        )
    })
}
