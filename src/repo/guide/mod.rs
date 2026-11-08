//! Guidance in a repository tab: the first steps of a new repository in the graph area, a
//! "Did you know" line under the graph, and a hint the first time something related is done.
//! What is shown is decided in pure functions ([`first_steps`], [`tips`], [`hints`],
//! [`empty_commit`]); this module holds the tab's part of it and carries out what the guide
//! asks for.

pub(crate) mod empty_commit;
pub(crate) mod first_steps;
pub(crate) mod hints;
#[cfg(test)]
mod tests;
mod tip_line;
pub(crate) mod tips;
mod view;

use std::time::Instant;

use super::branch_visibility::{Change, Visibility, local_ref};
use super::{RepoTab, WIP_HASH};
use crate::settings::Settings;
use first_steps::{Facts, FirstSteps};
pub(crate) use hints::Hint;

/// How many frames after it was asked for the commit summary still takes the focus: later,
/// it would take it from wherever the cursor went since.
const FOCUS_WITHIN_FRAMES: u64 = 3;

/// The guide's state in a tab.
#[derive(Debug, Default)]
pub(crate) struct Guide {
    pub hints: hints::Hints,
    /// How many times "Next tip" was clicked in this tab: the tip shown is the day's plus that.
    pub tip_offset: u64,
    /// Put the cursor in the commit summary, asked for on this frame.
    pub focus_summary: Option<u64>,
    /// The first steps shown under the first commit were put away, in this tab.
    pub steps_hidden: bool,
}

impl Guide {
    /// Whether the commit summary drawn on `frame` takes the focus (asked for just before);
    /// the request is used up either way.
    pub(crate) fn take_focus_summary(&mut self, frame: u64) -> bool {
        self.focus_summary
            .take()
            .is_some_and(|asked| frame <= asked + FOCUS_WITHIN_FRAMES)
    }
}

/// What the guide asks the tab to do.
#[derive(Clone, Copy, Debug)]
pub(crate) enum GuideAct {
    /// Bring the commit message fields into view, the summary focused.
    StartCommit,
    /// Ask before an empty first commit.
    EmptyCommit,
    /// Show this hint, if it never was.
    Hint(Hint),
    /// Take this hint back if it isn't shown yet.
    Withdraw(Hint),
}

/// The hint for a change of what the graph shows: hiding a branch (not the checked-out one,
/// which always shows, nor one taken out of the soloed ones) tells of soloing one.
pub(crate) fn visibility_hint(
    change: &Change,
    before: &Visibility,
    current: Option<&str>,
) -> Option<Hint> {
    match change {
        Change::Toggle(full)
            if !before.is_soloing()
                && before.shows(full)
                && current.is_none_or(|c| local_ref(c) != *full) =>
        {
            Some(Hint::SoloBranch)
        }
        _ => None,
    }
}

impl RepoTab {
    pub(super) fn guide_act(&mut self, act: GuideAct, ctx: &egui::Context) {
        match act {
            GuideAct::StartCommit => self.start_first_commit(ctx),
            GuideAct::EmptyCommit => self.ask_empty_commit(ctx),
            GuideAct::Hint(hint) => self.offer_hint(hint, ctx),
            GuideAct::Withdraw(hint) => self.guide.hints.withdraw(hint),
        }
    }

    /// Shows `hint` unless it was shown before, tips are off or another one is up.
    pub(super) fn offer_hint(&mut self, hint: Hint, ctx: &egui::Context) {
        if self
            .guide
            .hints
            .offer(hint, &Settings::get(ctx), Instant::now())
        {
            ctx.request_repaint();
        }
    }

    /// After row `row` was selected: a commit, with another one to compare it with, tells of
    /// comparing.
    pub(super) fn hint_after_select(&mut self, row: usize, ctx: &egui::Context) {
        if self.history.len() >= 2 && !self.is_wip(row) && self.stash_at(row).is_none() {
            self.offer_hint(Hint::CompareCommits, ctx);
        }
    }

    /// Why the undo on top must not run now, said in the status line; `true` when refused.
    pub(super) fn refuse_undo(&mut self) -> bool {
        let refusal = (self.timeline.current_entry().and_then(|e| e.undo.as_ref()))
            .and_then(|entry| empty_commit::undo_refusal(entry, &self.refs));
        let refused = refusal.is_some();
        if let Some(why) = refusal {
            self.status = Some((why, true));
        }
        refused
    }

    /// The changes row selected, the right pane open on it and the cursor in the summary.
    fn start_first_commit(&mut self, ctx: &egui::Context) {
        let Some(row) = self.commits.iter().position(|c| c.hash == WIP_HASH) else {
            self.status = Some((
                crate::i18n::text("repo-workflows-guide-start-first-commit-881f72cb").to_owned(),
                false,
            ));
            return;
        };
        self.close_pr();
        self.ci_view = None;
        self.compare = None;
        self.select(row, ctx);
        self.scroll_to = Some(row);
        let mut settings = Settings::get(ctx);
        if !settings.show_detail {
            settings.show_detail = true;
            settings.set(ctx);
        }
        self.guide.focus_summary = Some(ctx.cumulative_frame_nr());
    }

    /// The first steps the graph area shows, for a repository as good as new.
    pub(super) fn first_steps(&self) -> Option<FirstSteps> {
        // Branches hidden from the graph leave out commits it has.
        if self.opening() || self.load_error.is_some() || self.visibility.is_filtering() {
            return None;
        }
        first_steps::first_steps(&Facts::read(
            self.history.len(),
            &self.refs,
            &self.work,
            &self.remotes,
            &self.identity,
        ))
    }
}
