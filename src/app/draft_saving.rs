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

/// How long the exit waits for the last drafts to be written.
const EXIT_WAIT: Duration = Duration::from_secs(2);

impl App {
    /// Says so when the last run ended without closing.
    pub(super) fn say_if_unclean(&mut self) {
        if self.session.unclean {
            self.toasts.push(Toast::new(RESTORED.to_owned(), false));
        }
    }

    /// Saves the drafts when it is time to, and says when saving failed. Every frame.
    pub(super) fn keep_drafts(&mut self, ctx: &egui::Context, focused: bool) {
        let now = Instant::now();
        let shapes = self.draft_shapes();
        if self.keeper.due(now, focused, &shapes) {
            let all = self.all_drafts();
            self.keeper.save(now, all, shapes);
        } else {
            ctx.request_repaint_after(self.keeper.next_in(now));
        }
        if let Some(e) = drafts::take_error() {
            self.toasts.push(Toast::new(e, true));
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
