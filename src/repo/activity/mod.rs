//! The tab's activity log: each operation it ran, with every git command of it, how long it
//! took, how it ended and all it printed. Shown in the Activity panel (see [`view`]), and
//! where an error in the toolbar is read in full.

mod view;

use std::collections::VecDeque;
use std::time::{Duration, Instant};

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
    pub(super) outcome: Outcome,
    /// Nobody asked for it (the background fetch).
    pub(super) background: bool,
}

impl Entry {
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
        let id = self.next_id;
        self.next_id += 1;
        let steps = steps
            .into_iter()
            .map(|mut s| {
                s.output = bounded(&s.output);
                s
            })
            .collect();
        self.entries.push_back(Entry {
            id,
            label: label.to_owned(),
            at: Instant::now(),
            took,
            steps,
            outcome,
            background,
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

    /// Opens the panel on the newest entry.
    pub(super) fn reveal_last(&mut self) {
        self.open = true;
        self.reveal = self.entries.back().map(|e| e.id);
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
    format!(
        "{}\n… {left_out} bytes left out …\n{}",
        &output[..head_end],
        &output[tail_start..]
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

/// `git` and `args` as they would be typed in a shell (a word with spaces or quotes quoted),
/// with any password in a URL hidden.
pub(super) fn command_line(args: &[String]) -> String {
    let words: Vec<String> = args.iter().map(|a| shell_word(&hide_password(a))).collect();
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

/// `https://user:secret@host/x` → `https://user:***@host/x`.
fn hide_password(word: &str) -> String {
    let Some(scheme_end) = word.find("://") else {
        return word.to_owned();
    };
    let rest = &word[scheme_end + 3..];
    let authority_end = rest.find('/').unwrap_or(rest.len());
    let Some(at) = rest[..authority_end].rfind('@') else {
        return word.to_owned();
    };
    match rest[..at].find(':') {
        Some(colon) => format!(
            "{}{}:***{}",
            &word[..scheme_end + 3],
            &rest[..colon],
            &rest[at..]
        ),
        None => word.to_owned(),
    }
}

pub(super) fn exit_text(code: Option<i32>) -> String {
    match code {
        Some(0) => "exit 0".to_owned(),
        Some(c) => format!("exit {c}"),
        None => "did not finish".to_owned(),
    }
}

pub(super) fn outcome_text(outcome: &Outcome) -> &'static str {
    match outcome {
        Outcome::Done => "done",
        Outcome::Failed(_) => "failed",
        Outcome::Cancelled => "cancelled",
    }
}

/// "0.4 s", "12 s", "3 min 5 s".
pub(super) fn seconds(d: Duration) -> String {
    let ms = d.as_millis();
    match ms {
        0..10_000 => format!("{:.1} s", d.as_secs_f32()),
        10_000..60_000 => format!("{} s", d.as_secs()),
        _ => format!("{} min {} s", d.as_secs() / 60, d.as_secs() % 60),
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
        if background && result.is_ok() {
            return;
        }
        let outcome = match result {
            Ok(_) => Outcome::Done,
            Err(_) if cancelled => Outcome::Cancelled,
            Err(e) => Outcome::Failed(e.clone()),
        };
        let took = self.op_started.map(|t| t.elapsed()).unwrap_or_default();
        self.activity.push(label, took, steps, outcome, background);
    }
}
