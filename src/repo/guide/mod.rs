//! Guidance in a repository tab: the first steps of a new repository in the graph area, a
//! "Did you know" line under the graph, and a hint the first time something related is done.
//! What is shown is decided in pure functions ([`first_steps`], [`tips`], [`hints`]); this
//! module holds the tab's part of it and carries out what the guide asks for.

pub(crate) mod first_steps;
pub(crate) mod hints;
#[cfg(test)]
mod tests;
mod tip_line;
pub(crate) mod tips;
mod view;

use std::time::Instant;

use super::branch_visibility::{Change, Visibility};
use super::{RepoTab, WIP_HASH};
use crate::settings::Settings;
use first_steps::{Facts, FirstSteps};
pub(crate) use hints::Hint;

/// The guide's state in a tab.
#[derive(Debug, Default)]
pub(crate) struct Guide {
    pub hints: hints::Hints,
    /// How many times "Next tip" was clicked in this tab: the tip shown is the day's plus that.
    pub tip_offset: u64,
    /// Put the cursor in the commit summary the next time it is drawn.
    pub focus_summary: bool,
    /// The first steps shown under the first commit were put away, in this tab.
    pub steps_hidden: bool,
}

/// What the guide asks the tab to do.
#[derive(Clone, Copy, Debug)]
pub(crate) enum GuideAct {
    /// Bring the commit message fields into view, the summary focused.
    StartCommit,
    /// Show this hint, if it never was.
    Hint(Hint),
}

/// The hint for a change of what the graph shows: hiding a branch tells of soloing one.
pub(crate) fn visibility_hint(change: &Change, before: &Visibility) -> Option<Hint> {
    match change {
        Change::Toggle(full) if before.shows(full) => Some(Hint::SoloBranch),
        _ => None,
    }
}

impl RepoTab {
    pub(super) fn guide_act(&mut self, act: GuideAct, ctx: &egui::Context) {
        match act {
            GuideAct::StartCommit => self.start_first_commit(ctx),
            GuideAct::Hint(hint) => self.offer_hint(hint, ctx),
        }
    }

    /// Shows `hint` unless it was shown before, tips are off or another one is up.
    pub(super) fn offer_hint(&mut self, hint: Hint, ctx: &egui::Context) {
        let mut settings = Settings::get(ctx);
        if self.guide.hints.offer(hint, &mut settings, Instant::now()) {
            settings.set(ctx);
            ctx.request_repaint();
        }
    }

    /// The changes row selected, the right pane open on it and the cursor in the summary.
    fn start_first_commit(&mut self, ctx: &egui::Context) {
        let Some(row) = self.commits.iter().position(|c| c.hash == WIP_HASH) else {
            self.status = Some((
                "Nothing to commit yet: add or change files in the repository's folder".to_owned(),
                true,
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
        self.guide.focus_summary = true;
    }

    /// The first steps the graph area shows, for a repository as good as new.
    pub(super) fn first_steps(&self) -> Option<FirstSteps> {
        // Branches hidden from the graph leave out commits it has.
        if self.opening() || self.load_error.is_some() || self.visibility.is_filtering() {
            return None;
        }
        let commits = if self.refs.head.is_empty() {
            0
        } else {
            self.history.len().max(1)
        };
        first_steps::first_steps(&Facts::read(commits, &self.refs, &self.work, &self.remotes))
    }
}
