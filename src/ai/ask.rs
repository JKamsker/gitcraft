//! One question to the tool the settings pick: a commit message for what is staged, or a review
//! of a branch. What the commit box and the review panel run in the background.

use std::path::Path;
use std::time::Duration;

use super::prompt::clean_answer;
use super::run::{self, Cancel, CommandLine, Job, RunError};
use super::{AiSettings, gather};

/// What is asked.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Question {
    /// A message for the staged changes.
    CommitMessage,
    /// A review of what `branch` adds to `base`.
    Review { branch: String, base: String },
}

/// What the tool answered, and how much of the diff it saw.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Answer {
    pub text: String,
    /// Files in the diff.
    pub files: usize,
    /// Of them, those left out or cut short.
    pub left_out: usize,
}

/// Asks `question` about the repository at `repo`, on its machine, giving up after `timeout`.
/// Blocking: from a background thread.
pub fn ask(
    repo: &Path,
    settings: &AiSettings,
    question: &Question,
    timeout: Duration,
    cancel: &Cancel,
) -> Result<Answer, RunError> {
    let command = CommandLine::for_tool(settings).map_err(RunError::Failed)?;
    let prompt = match question {
        Question::CommitMessage => gather::commit_prompt(repo, settings),
        Question::Review { branch, base } => gather::review_prompt(repo, settings, branch, base),
    }
    .map_err(RunError::Failed)?;
    if cancel.is_cancelled() {
        return Err(RunError::Cancelled);
    }
    let out = run::run(&Job {
        repo,
        command: &command,
        input: prompt.text.as_bytes(),
        timeout,
        cancel,
    })?;
    let text = clean_answer(&out);
    if text.is_empty() {
        return Err(RunError::Failed(format!(
            "{} answered nothing.",
            command.program()
        )));
    }
    Ok(Answer {
        text,
        files: prompt.files,
        left_out: prompt.left_out,
    })
}

#[cfg(all(test, unix))]
mod tests;
