//! Keeps what the tabs have typed and not used on disk (see [`crate::drafts`]), and says on
//! start when the last run ended without closing.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use super::panels::Toast;
use super::{App, Tab};
use crate::drafts::{self, RepoDrafts};
use crate::repo::DraftShape;

/// What the start says after a crash.
pub(super) const RESTORED: &str =
    "gitcraft closed unexpectedly: your drafts and tabs were restored";

/// How long, in seconds, that is shown.
const RESTORED_FOR: u64 = 15;

/// How long the exit waits for the last drafts to be written.
const EXIT_WAIT: Duration = Duration::from_secs(2);

impl App {
    /// Says so, once, when the last run ended without closing: on the first frame, which can be
    /// a while after the start, and for long enough to be read.
    fn say_if_unclean(&mut self) {
        if std::mem::take(&mut self.session.unclean) {
            self.toasts
                .push(Toast::for_secs(RESTORED.to_owned(), false, RESTORED_FOR));
        }
    }

    /// Saves what tab `i` has typed now, as it is being closed.
    pub(super) fn save_closing_tab(&self, i: usize) {
        if let Some(Tab::Repo(r)) = self.tabs.get(i) {
            save_now(r);
        }
    }

    /// Saves what every repository tab has typed now, as they are all being closed (a switch
    /// of profiles).
    pub(super) fn save_all_tabs(&self) {
        self.repo_tabs().for_each(save_now);
    }

    /// Saves the drafts when it is time to, and says when saving failed. Every frame.
    pub(super) fn keep_drafts(&mut self, ctx: &egui::Context, focused: bool) {
        self.say_if_unclean();
        let now = Instant::now();
        let shapes = self.draft_shapes();
        if self.keeper.due(now, focused, &shapes) {
            let all = self.all_drafts();
            self.keeper.save(now, all, shapes);
        } else {
            ctx.request_repaint_after(self.keeper.next_in(now));
        }
        if let Some(notice) = drafts::take_notice() {
            self.toasts.push(Toast::new(notice.text, notice.error));
        }
    }

    /// A normal close: the last drafts are written, and the marker of this run goes.
    pub(super) fn close_drafts(&mut self) {
        let now = Instant::now();
        let (all, shapes) = (self.all_drafts(), self.draft_shapes());
        self.keeper.save(now, all, shapes);
        drafts::flush(EXIT_WAIT);
        self.session.end();
    }

    fn repo_tabs(&self) -> impl Iterator<Item = &crate::repo::RepoTab> {
        self.tabs.iter().filter_map(|tab| match tab {
            Tab::Repo(r) => Some(&**r),
            _ => None,
        })
    }

    fn all_drafts(&self) -> Vec<(PathBuf, RepoDrafts)> {
        self.repo_tabs().flat_map(|r| r.all_drafts()).collect()
    }

    fn draft_shapes(&self) -> Vec<(PathBuf, DraftShape)> {
        self.repo_tabs().flat_map(|r| r.draft_shapes()).collect()
    }
}

/// Saves what `tab` and its parked worktrees have typed, now: the tab is about to go, and the
/// last few seconds of typing would otherwise be lost (what was saved before would come back).
pub(super) fn save_now(tab: &crate::repo::RepoTab) {
    for (path, drafts) in tab.all_drafts() {
        drafts::save(&path, drafts);
    }
}

/// Keeps no drafts and no session marker in this process: for tools that drive the window
/// (`examples/ui_shot.rs`), which must not touch the user's.
pub fn keep_no_drafts() {
    drafts::turn_off();
}
