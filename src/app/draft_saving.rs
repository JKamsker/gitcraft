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

/// Translates only presentation of known notices; the writer/session values stay raw.
pub(super) fn display_notice(raw: &str) -> Option<String> {
    if raw == RESTORED {
        return Some(crate::i18n::text("interaction-draft-restored").to_owned());
    }
    if let Some(name)=raw.strip_prefix("Some of what you typed in ").and_then(|s|s.strip_suffix(" is too long to keep in case gitcraft closes: not all of it would come back after a crash")) {
        return Some(crate::i18n::message("interaction-draft-cut",&[("name",name.into())]));
    }
    raw.strip_prefix("Couldn't save what you typed in ")
        .map(|detail| {
            crate::i18n::message("interaction-draft-failed", &[("detail", detail.into())])
        })
}

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
        self.tabs
            .iter()
            .chain(self.windows.parked.values().flat_map(|w| &w.tabs))
            .filter_map(|tab| match tab {
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

#[cfg(test)]
mod localization_tests {
    use super::*;

    #[test]
    fn actual_notice_paint_keeps_raw_session_and_worker_values_and_opaque_details() {
        crate::i18n::set_language("de");
        let raw = RESTORED.to_owned();
        assert_eq!(
            super::super::panels::display_app_message(&raw),
            "gitcraft wurde unerwartet beendet: Deine Entwürfe und Tabs wurden wiederhergestellt"
        );
        assert_eq!(
            raw,
            "gitcraft closed unexpectedly: your drafts and tabs were restored"
        );
        let name = "日本語 is too long: literal";
        let cut = format!(
            "Some of what you typed in {name} is too long to keep in case gitcraft closes: not all of it would come back after a crash"
        );
        assert_eq!(
            display_notice(&cut),
            Some(format!(
                "Ein Teil deiner Eingaben in {name} ist zu lang für die Wiederherstellung nach einem Absturz: Nicht alles könnte danach wiederhergestellt werden"
            ))
        );
        let detail = "日本語: C:\\path: external\ndiagnostic";
        let failed = format!("Couldn't save what you typed in {detail}");
        assert_eq!(
            display_notice(&failed),
            Some(format!(
                "Deine Eingaben konnten nicht gespeichert werden: {detail}"
            ))
        );
        assert_eq!(
            display_notice("Some of what you typed in external unknown detail"),
            None
        );
        assert_eq!(
            super::super::panels::display_app_message("opaque external 日本語"),
            "opaque external 日本語"
        );
        crate::i18n::set_language("en-US");
        for original in [&raw, &cut, &failed] {
            assert_eq!(display_notice(original).as_ref(), Some(original));
        }
    }
}
