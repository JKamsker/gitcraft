//! Commit messages and branch reviews written by an AI command-line tool the user already has:
//! `claude -p`, `codex exec`, `gh copilot`, or a command of their own. gitcraft keeps no API key
//! and talks to no AI service itself: it runs the tool on the repository's machine (here, in
//! WSL or over SSH, like git), hands it the prompt on its input and reads its answer.
//!
//! - [`settings`]: which tool, the prompts, how much of a diff is sent.
//! - [`prompt`]: the prompt, the diff cut to size with a list of what was left out.
//! - [`gather`]: what git says (the staged diff, a branch's changes, recent summaries).
//! - [`detect`]: which tools are installed on a machine.
//! - [`run`]: running one, with a time limit and a way to stop it.
//! - [`ask`]: the two questions put together: a commit message, a branch review.

pub mod ask;
pub mod detect;
pub mod gather;
pub mod prompt;
pub mod run;
pub mod settings;

pub use ask::{Answer, Question, ask};
pub use run::{Cancel, RunError, TIMEOUT};
pub use settings::{AiSettings, AiTool};
