//! The tab's activity log: each operation it ran, with every git command of it, how long it
//! took, how it ended and all it printed. Shown in the Activity panel (see [`view`]), and
//! where an error in the toolbar is read in full.

mod view;

use std::collections::VecDeque;
use std::time::{Duration, Instant};

use super::notice::operation::Failure;
use crate::git::Recorded;

/// How many operations the log keeps; older ones are dropped.
pub(super) const MAX_ENTRIES: usize = 100;

/// The most of one command's output kept: its start and, mostly, its end (where git says
/// what went wrong).
pub(super) const MAX_OUTPUT: usize = 64 * 1024;

/// How much of the start of a long output is kept, of [`MAX_OUTPUT`].
const KEEP_HEAD: usize = 8 * 1024;

/// How an operation ended.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Outcome {
    Done,
    /// What it failed with.
    Failed(String),
    Cancelled,
}

/// One operation of the log.
#[derive(Clone, Debug)]
pub(super) struct Entry {
    /// Its number in the log, never reused.
    pub(super) id: u64,
    pub(super) label: String,
    /// When it ended.
    pub(super) at: Instant,
    pub(super) took: Duration,
    pub(super) steps: Vec<Recorded>,
    /// Each step's command line, as shown (a long list of paths shortened).
    pub(super) lines: Vec<String>,
    pub(super) outcome: Outcome,
    /// Nobody asked for it (the background fetch).
    pub(super) background: bool,
    /// How many times in a row it failed the same way (a background fetch while offline).
    pub(super) repeats: u32,
    /// Optional operation-owned source, retired if retained raw diagnostics change.
    failure_presentation: Option<Failure>,
}

impl Entry {
    fn display_error(&self, raw: &str) -> String {
        self.failure_presentation
            .as_ref()
            .and_then(|source| source.display(raw))
            .unwrap_or_else(|| display_workflow_error(raw))
    }

    /// The whole entry as text, for the clipboard: each command, how it ended, its output.
    pub(super) fn as_text(&self) -> String {
        let mut text = format!(
            "{} ({}): {}\n",
            self.label,
            seconds(self.took),
            outcome_text(&self.outcome)
        );
        for step in &self.steps {
            text.push_str(&format!(
                "\n$ {}\n[{} · {}]\n",
                command_line(&step.args),
                exit_text(step.code),
                seconds(step.took)
            ));
            if !step.output.is_empty() {
                text.push_str(&step.output);
                text.push('\n');
            }
        }
        text
    }
}

/// The log of a tab, newest last.
#[derive(Default)]
pub(super) struct ActivityLog {
    entries: VecDeque<Entry>,
    next_id: u64,
    /// The panel is open.
    pub(super) open: bool,
    /// An entry to open and scroll to (an error clicked in the toolbar).
    pub(super) reveal: Option<u64>,
    /// The toolbar message an operation left, and its entry: a click on that message opens it.
    pub(super) status_link: Option<(String, u64)>,
    /// The entry just added, until `op_done` has set its message.
    pending_link: Option<u64>,
}

impl ActivityLog {
    /// Adds an operation that ended, dropping the oldest past [`MAX_ENTRIES`]; its id.
    pub(super) fn push(
        &mut self,
        label: &str,
        took: Duration,
        steps: Vec<Recorded>,
        outcome: Outcome,
        background: bool,
    ) -> u64 {
        self.push_presented(label, took, steps, outcome, background, None)
    }

    pub(in crate::repo) fn push_presented(
        &mut self,
        label: &str,
        took: Duration,
        steps: Vec<Recorded>,
        outcome: Outcome,
        background: bool,
        presentation: Option<Failure>,
    ) -> u64 {
        let outcome = match outcome {
            Outcome::Failed(why) => Outcome::Failed(bounded(&why)),
            other => other,
        };
        let presentation = match &outcome {
            Outcome::Failed(raw) => presentation.and_then(|source| source.matching(raw)),
            _ => None,
        };
        // The same background failure again (offline): counted, not added.
        if background
            && let Some(last) = self.entries.back_mut()
            && last.background
            && last.label == label
            && last.outcome == outcome
            && matches!(outcome, Outcome::Failed(_))
        {
            // A repeated plain error replaces the old source even when raw text is equal.
            last.failure_presentation = presentation;
            last.repeats += 1;
            last.at = Instant::now();
            last.took = took;
            return last.id;
        }
        let id = self.next_id;
        self.next_id += 1;
        let steps: Vec<Recorded> = steps
            .into_iter()
            .map(|mut s| {
                s.output = bounded(&s.output);
                s
            })
            .collect();
        let lines = steps.iter().map(|s| short_command_line(&s.args)).collect();
        self.entries.push_back(Entry {
            id,
            label: label.to_owned(),
            at: Instant::now(),
            took,
            steps,
            lines,
            outcome,
            background,
            repeats: 1,
            failure_presentation: presentation,
        });
        while self.entries.len() > MAX_ENTRIES {
            self.entries.pop_front();
        }
        id
    }

    pub(super) fn entries(&self) -> impl DoubleEndedIterator<Item = &Entry> {
        self.entries.iter()
    }

    pub(super) fn len(&self) -> usize {
        self.entries.len()
    }

    /// The newest entry.
    #[cfg(test)]
    pub(super) fn last(&self) -> Option<&Entry> {
        self.entries.back()
    }

    pub(super) fn clear(&mut self) {
        self.entries.clear();
    }

    /// Opens the panel on the entry `id` (if it is still kept).
    pub(super) fn reveal(&mut self, id: u64) {
        self.open = true;
        self.reveal = self.entries.iter().any(|e| e.id == id).then_some(id);
    }
}

/// `output` cut to [`MAX_OUTPUT`]: its start and its end, with how much was left out between.
pub(super) fn bounded(output: &str) -> String {
    if output.len() <= MAX_OUTPUT {
        return output.to_owned();
    }
    let head_end = floor_char(output, KEEP_HEAD);
    let tail_start = ceil_char(output, output.len() - (MAX_OUTPUT - KEEP_HEAD));
    let left_out = tail_start - head_end;
    crate::i18n::message(
        "repo-workflows-activity-bounded-62a56981",
        &[
            ("output-head-end-1", (&output[..head_end]).clone().into()),
            ("left-out", (left_out).clone().into()),
            (
                "output-tail-start-2",
                (&output[tail_start..]).clone().into(),
            ),
        ],
    )
}

fn floor_char(s: &str, mut i: usize) -> usize {
    while !s.is_char_boundary(i) {
        i -= 1;
    }
    i
}

fn ceil_char(s: &str, mut i: usize) -> usize {
    while !s.is_char_boundary(i) {
        i += 1;
    }
    i
}

/// The most arguments a command line shows in the panel; the rest are counted.
const SHOWN_ARGS: usize = 24;

/// [`command_line`] with a long list of arguments (a `git add` of thousands of paths) cut,
/// saying how many more there are.
pub(super) fn short_command_line(args: &[String]) -> String {
    if args.len() <= SHOWN_ARGS {
        return command_line(args);
    }
    crate::i18n::message(
        "repo-workflows-activity-short-command-line-c85cf117",
        &[
            (
                "command-line-args-shown-args-1",
                (command_line(&args[..SHOWN_ARGS])).clone().into(),
            ),
            (
                "args-len-shown-args-2",
                (args.len() - SHOWN_ARGS).clone().into(),
            ),
        ],
    )
}

/// `git` and `args` as they would be typed in a shell (a word with spaces or quotes quoted).
/// Secrets were hidden when the call was recorded (see [`crate::git::redact`]).
pub(super) fn command_line(args: &[String]) -> String {
    let words: Vec<String> = args.iter().map(|a| shell_word(a)).collect();
    format!("git {}", words.join(" "))
}

fn shell_word(word: &str) -> String {
    let plain = !word.is_empty()
        && word
            .chars()
            .all(|c| c.is_alphanumeric() || "-_./:=@,+%^~".contains(c));
    if plain {
        word.to_owned()
    } else {
        format!("'{}'", word.replace('\'', r"'\''"))
    }
}

pub(super) fn exit_text(code: Option<i32>) -> String {
    match code {
        Some(0) => crate::i18n::text("repo-workflows-activity-exit-text-c22995ad").to_owned(),
        Some(c) => crate::i18n::message(
            "repo-workflows-activity-exit-text-bd2f2ddf",
            &[("c", (c).clone().into())],
        ),
        None => crate::i18n::text("repo-workflows-activity-exit-text-64df366e").to_owned(),
    }
}

pub(super) fn outcome_text(outcome: &Outcome) -> &'static str {
    match outcome {
        Outcome::Done => crate::i18n::text("repo-workflows-activity-outcome-text-a4c3ed04"),
        Outcome::Failed(_) => crate::i18n::text("repo-workflows-activity-outcome-text-5d28a90f"),
        Outcome::Cancelled => crate::i18n::text("repo-workflows-activity-outcome-text-8b47045e"),
    }
}

/// "0.4 s", "12 s", "3 min 5 s".
pub(super) fn seconds(d: Duration) -> String {
    let ms = d.as_millis();
    match ms {
        0..10_000 => crate::i18n::message(
            "repo-workflows-activity-seconds-6f29d45b",
            &[("d-as-secs-f32-1", (d.as_secs_f32()).clone().into())],
        ),
        10_000..60_000 => crate::i18n::message(
            "repo-workflows-activity-seconds-af37e50f",
            &[("d-as-secs-1", (d.as_secs()).clone().into())],
        ),
        _ => crate::i18n::message(
            "repo-workflows-activity-seconds-fba4d59d",
            &[
                ("d-as-secs-60-1", (d.as_secs() / 60).clone().into()),
                ("d-as-secs-60-2", (d.as_secs() % 60).clone().into()),
            ],
        ),
    }
}

/// Finite workflow-owned control errors: presentation only; unknown diagnostics remain opaque.
pub(in crate::repo) fn display_workflow_error(error: &str) -> String {
    match error {
        "Cancelled before starting" => {
            crate::i18n::text("repo-workflows-local_ci-submit-84f8f92f").to_owned()
        }
        "Cancelled before terminal start" => {
            crate::i18n::text("repo-workflows-devcontainer-terminal-f171832a").to_owned()
        }
        "File history cancelled" => {
            crate::i18n::text("repo-workflows-atlas-new-70ba4563").to_owned()
        }
        "History read cancelled" => {
            crate::i18n::text("repo-workflows-saved_views-poll-dc101692").to_owned()
        }
        "History summary cancelled or exceeded its output limit" => {
            crate::i18n::text("repo-workflows-line_dossier-run-a9f1b338").to_owned()
        }
        "History worker panicked" => {
            crate::i18n::text("repo-workflows-saved_views-poll-33647b3e").to_owned()
        }
        "History worker stopped" => {
            crate::i18n::text("repo-workflows-saved_views-poll-da12ac51").to_owned()
        }
        "Moved-code analysis cancelled or exceeded 40 seconds" => {
            crate::i18n::text("repo-workflows-file_comparison-read-431f5828").to_owned()
        }
        "Moved-code comparison cancelled" => {
            crate::i18n::text("repo-workflows-file_comparison-assemble-56abde67").to_owned()
        }
        "Postcard rendering cancelled" => {
            crate::i18n::text("repo-workflows-postcards-bounded-graph-8e2ed023").to_owned()
        }
        "Summary cancelled" => {
            crate::i18n::text("repo-workflows-line_dossier-run-ae6f0623").to_owned()
        }
        other => crate::git::display_error(&crate::forge::inbox::display_error(other)),
    }
}

#[cfg(test)]
mod tests;

impl super::RepoTab {
    /// Keeps the operation that just ended in the log, with what its git calls did (the
    /// `steps`, from the operation's control). A background fetch that went well is not kept:
    /// one every few minutes would push out what the user did.
    pub(super) fn log_activity(
        &mut self,
        label: &str,
        result: &Result<String, String>,
        cancelled: bool,
        background: bool,
        steps: Vec<Recorded>,
    ) {
        self.log_activity_presented(label, result, cancelled, background, steps, None);
    }

    pub(super) fn log_activity_presented(
        &mut self,
        label: &str,
        result: &Result<String, String>,
        cancelled: bool,
        background: bool,
        steps: Vec<Recorded>,
        presentation: Option<Failure>,
    ) {
        if background && result.is_ok() {
            return;
        }
        let password = self.credentials.as_ref().map(|c| c.password.as_str());
        let outcome = match result {
            Ok(_) => Outcome::Done,
            Err(_) if cancelled => Outcome::Cancelled,
            Err(e) => Outcome::Failed(crate::git::redact(e, password)),
        };
        let took = self.op_started.map(|t| t.elapsed()).unwrap_or_default();
        let id =
            self.activity
                .push_presented(label, took, steps, outcome, background, presentation);
        self.activity.pending_link = Some(id);
    }

    /// Ties the toolbar message the operation just left (by `op_done`) to its entry.
    pub(super) fn link_status(&mut self) {
        if let Some(id) = self.activity.pending_link.take() {
            self.activity.status_link = self.status.as_deref().map(|(msg, _)| (msg.clone(), id));
        }
    }

    /// The entry of the message the toolbar shows, when an operation left it.
    pub(super) fn status_entry(&self) -> Option<u64> {
        let (msg, _) = self.status.as_deref()?;
        let (linked, id) = self.activity.status_link.as_ref()?;
        (linked == msg).then_some(*id)
    }
}
