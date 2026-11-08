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
    note_presentation: Option<(String, MessageNote)>,
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
            &[("secs", (secs).into())],
        )
    } else {
        crate::i18n::message(
            "repo-workflows-ai-elapsed-fba4d59d",
            &[
                ("secs-60-1", (secs / 60).into()),
                ("secs-60-2", (secs % 60).into()),
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

#[derive(Clone)]
enum MessageNote {
    Empty,
    Coverage { files: usize, left_out: usize },
}
impl MessageNote {
    fn raw(&self) -> String {
        match self {
            Self::Empty => "The tool's answer had no summary line.".into(),
            Self::Coverage { files, left_out } => {
                let whole = files.saturating_sub(*left_out);
                format!(
                    "The tool saw {whole} of {files} file{} in full; the rest only in part or by name (Settings → AI sets how much is sent).",
                    if *files == 1 { "" } else { "s" }
                )
            }
        }
    }
    fn display(&self) -> String {
        match self {
            Self::Empty => crate::i18n::text("repo-workflows-ai-message-answered-00f5dcad").into(),
            Self::Coverage { files, left_out } => coverage(*files, *left_out).unwrap_or_default(),
        }
    }
}
impl AiState {
    fn remember_message_note(&mut self, note: MessageNote, error: bool) {
        let raw = note.raw();
        self.note = Some((raw.clone(), error));
        self.note_presentation = Some((raw, note));
    }
    fn display_message_note(&self, raw: &str) -> String {
        match &self.note_presentation {
            Some((old, note)) if old == raw => note.display(),
            _ => raw.into(),
        }
    }
}

#[cfg(test)]
mod fluent_r1_note_tests {
    use super::*;
    #[test]
    fn fluent_partition_r1_ai_message_note_retains_numeric_coverage_and_raw_failure()
    -> Result<(), String> {
        for note in [
            MessageNote::Empty,
            MessageNote::Coverage {
                files: 1234,
                left_out: 1,
            },
        ] {
            crate::i18n::set_language("de");
            let mut state = AiState::default();
            state.remember_message_note(note, true);
            let raw = state.note.clone().ok_or("missing test value")?.0;
            for lang in ["de", "en-US", "de"] {
                crate::i18n::set_language(lang);
                if lang == "de" {
                    assert_ne!(state.display_message_note(&raw), raw);
                }
                assert_eq!(state.note.as_ref().ok_or("missing test value")?.0, raw);
                assert_eq!(state.requests, 0);
                assert!(state.writing.is_none());
            }
            state.note_presentation = None;
            assert_eq!(state.display_message_note(&raw), raw);
        }
        crate::i18n::set_language("en-US");

        Ok(())
    }
}
