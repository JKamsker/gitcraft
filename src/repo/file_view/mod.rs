//! Central pane for one file: its history (every commit that changed it, following renames),
//! its blame (who last changed each line) or the whole file as it is at a commit. Opened from a
//! file's right-click menu and from the list of all the files of a commit.

mod view;
use view::error;

mod loading;

mod inputs;
use inputs::*;
pub(super) use inputs::{blame_input, content_key};

mod blame;
mod history;
mod line_history;

#[cfg(test)]
mod tests;

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

// Shared across repository tabs, and never reused when a view is closed or replaced.
fn next_generation() -> u64 {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    let mut current = NEXT.load(Ordering::Relaxed);
    loop {
        let next = current
            .checked_add(1)
            .unwrap_or_else(|| std::process::abort());
        match NEXT.compare_exchange_weak(current, next, Ordering::Relaxed, Ordering::Relaxed) {
            Ok(previous) => return previous,
            Err(observed) => current = observed,
        }
    }
}

use egui::{Stroke, Ui};

use super::file_text::{self, FileText};
use super::find::Source;
use super::widgets;
use super::{Action, RepoTab};
use crate::forge::links::{Lines, Target};
use crate::git::{self, FileCommit};
use crate::icons;
use crate::theme::{self, Tokens};

pub(crate) use blame::{BlameData, load_blame};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FileTab {
    History,
    Blame,
    /// The whole file.
    File,
}

pub(crate) struct FileView {
    pub generation: u64,
    pub path: String,
    /// The commit history, blame and the file are at; HEAD and the working tree when `None`.
    pub rev: Option<String>,
    pub tab: FileTab,
    pub history: Option<Result<Vec<FileCommit>, String>>,
    pub blame: Option<Result<BlameData, String>>,
    /// Blame also depends on configuration, even when file content has a fixed OID.
    pub blame_generation: u64,
    /// A successful previous result remains visible until its replacement arrives.
    pub blame_needs_refresh: bool,
    pub text: Option<Result<FileText, String>>,
    /// The history of only these lines (where their numbers count from, and the lines), not
    /// of the whole file.
    pub lines: Option<(git::LinesAt, git::LineRange)>,
    pub line_inputs: Option<(String, String, git::LineRange)>,
    pub line_input_check: u64,
    history_input: Option<String>,
    text_input: Option<String>,
    observed_inputs: Option<FileInputs>,
    input_check: u64,
    probe: ProbeState,
    pending: [Option<u64>; 3],
    retry_load: Option<Instant>,
    line_observed: Option<(String, String, git::LineRange)>,
    blame_input: Option<String>,
    observed_blame: Option<String>,
    /// A line to scroll to once the blame is read, and to mark (1 is the first).
    pub goto_line: std::cell::Cell<Option<usize>>,
    pub marked_line: Option<usize>,
}

#[derive(Default)]
struct ProbeState {
    active: Option<u64>,
    dirty: bool,
    retry: Option<Instant>,
}

enum ProbeInputs {
    File(FileInputs),
    Lines(Result<(String, String, git::LineRange), String>),
}

#[derive(Clone, Default)]
pub(super) struct FileInputs {
    history: Option<String>,
    text: Option<String>,
    blame: Option<String>,
}

fn pane_index(pane: FileTab) -> usize {
    match pane {
        FileTab::History => 0,
        FileTab::Blame => 1,
        FileTab::File => 2,
    }
}

impl FileView {
    pub fn new(path: &str, rev: Option<String>, tab: FileTab) -> Self {
        Self {
            generation: next_generation(),
            path: path.to_owned(),
            rev,
            tab,
            history: None,
            blame: None,
            blame_generation: next_generation(),
            blame_needs_refresh: false,
            text: None,
            lines: None,
            line_inputs: None,
            line_input_check: 0,
            history_input: None,
            text_input: None,
            observed_inputs: None,
            input_check: 0,
            probe: ProbeState::default(),
            pending: [None; 3],
            retry_load: None,
            line_observed: None,
            blame_input: None,
            observed_blame: None,
            goto_line: Default::default(),
            marked_line: None,
        }
    }

    /// Invalidate references that can move, including cached failures. A full object ID
    /// identifies immutable content; abbreviations and revision expressions do not.
    pub fn invalidate_mutable(&mut self) -> bool {
        if self.rev.as_deref().is_some_and(git::investigations::oid) {
            return false;
        }
        self.generation = next_generation();
        self.history_input = None;
        self.text_input = None;
        self.observed_inputs = None;
        self.line_observed = None;
        self.input_check = next_generation();
        self.history = None;
        self.invalidate_blame();
        self.text = None;
        true
    }

    /// Suspension disconnects the result channel, so old callbacks cannot release
    /// their slots. Retire those request lifetimes before installing a new sender.
    pub(super) fn retire_reads(&mut self) {
        if !self.invalidate_mutable() {
            self.generation = next_generation();
            self.invalidate_blame();
        }
        self.pending = [None; 3];
        self.probe = ProbeState::default();
        self.retry_load = None;
        self.line_observed = None;
        self.line_inputs = None;
    }

    fn invalidate_blame(&mut self) {
        self.blame_generation = next_generation();
        self.blame_input = None;
        self.observed_blame = None;
        self.blame_needs_refresh = true;
        if !matches!(self.blame, Some(Ok(_))) {
            self.blame = None;
        }
    }

    fn reuses_line_inputs(
        &self,
        inputs: &Result<(String, String, git::LineRange), String>,
    ) -> bool {
        (matches!(self.history, Some(Ok(_)))
            || (self.history.is_none() && self.pending[0].is_some()))
            && inputs.is_ok()
            && inputs.as_ref().ok() == self.line_inputs.as_ref()
    }

    /// Whether `tab` still has to be loaded.
    pub fn needs(&self, tab: FileTab) -> bool {
        match tab {
            FileTab::History => self.history.is_none(),
            FileTab::Blame => self.blame.is_none() || self.blame_needs_refresh,
            FileTab::File => self.text.is_none(),
        }
    }
}
