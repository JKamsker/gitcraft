//! Resolving conflicts in the background: a coding agent (Claude Code or Codex, headless) works
//! on the conflicted files in a scratch worktree of their own, out of the user's way, while it
//! reports its plan and each step to gitcraft over `gitcraft mcp`'s `resolve_*` tools. The tab
//! shows that as it goes (see `repo::resolver`); the user applies the result when it is done.
//!
//! - [`workspace`]: the scratch worktree, the files given and how they come back.
//! - [`agent`]: the agent's command line, prompt and what of its output is shown.
//! - [`process`]: running it.
//! - [`report`]: the agent's reports and the progress kept of them.

pub(crate) mod agent;
pub(crate) mod process;
pub mod report;
pub(crate) mod workspace;

/// A name for one run, for the agent's reports to be told apart by.
pub(crate) fn run_id() -> String {
    let token = crate::remote::askpass::random_token();
    format!("r{}", token.get(..16).unwrap_or(&token))
}

#[cfg(test)]
mod e2e_tests;
