//! How a source of the library stands, in words: scanning, waiting, when it was last
//! scanned and what it found, or why it is partial or unavailable.

use super::model::{Outcome, ScanRecord};
use super::scheduler::Activity;
use crate::app::repo_scan::{self, Stopped};

/// The state a source is shown in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::app) enum State {
    Scanning,
    Waiting,
    Never,
    Done,
    /// Stopped early: there may be more.
    Partial,
    /// Its last scan is older than the refresh interval.
    Stale,
    /// It couldn't be looked in.
    Unavailable,
    Off,
}

impl State {
    /// Whether it says something the user may want to act on (shown in the warning colour).
    pub fn is_problem(self) -> bool {
        matches!(self, State::Partial | State::Unavailable)
    }
}

/// The state of a source with `last` scan, `activity` and `enabled`, at `now`.
pub(in crate::app) fn state(
    enabled: bool,
    activity: Option<Activity>,
    last: Option<&ScanRecord>,
    now: i64,
) -> State {
    match (enabled, activity, last) {
        (false, _, _) => State::Off,
        (_, Some(Activity::Running(_)), _) => State::Scanning,
        (_, Some(Activity::Queued), _) => State::Waiting,
        (_, None, None) => State::Never,
        (_, None, Some(l)) => match &l.outcome {
            Outcome::Failed(_) => State::Unavailable,
            Outcome::Partial(_) => State::Partial,
            Outcome::Complete if now - l.at >= super::STALE_AFTER => State::Stale,
            Outcome::Complete => State::Done,
        },
    }
}

/// Why a scan stopped, in a few words.
fn stopped_words(stopped: Stopped) -> &'static str {
    crate::i18n::text(match stopped {
        Stopped::Repos => "interaction-library-limit-repos",
        Stopped::Folders | Stopped::Entries => "interaction-library-limit-folders",
        Stopped::Time => "interaction-library-limit-time",
        Stopped::Asked => "interaction-library-limit-asked",
    })
}

/// The line under a source: what it is doing, or how its last scan went and when.
pub(in crate::app) fn line(
    enabled: bool,
    activity: Option<Activity>,
    last: Option<&ScanRecord>,
    now: i64,
) -> String {
    use crate::i18n::{message, text};
    if let Some(Activity::Running(p)) = activity.filter(|_| enabled) {
        return message(
            "interaction-scan-progress",
            &[
                ("folders", p.folders.into()),
                ("repositories", p.repos.into()),
            ],
        );
    }
    let when = |l: &ScanRecord| crate::git::relative_time(l.at, now);
    match (state(enabled, activity, last, now), last) {
        (State::Off, _) => text("interaction-library-off").to_owned(),
        (State::Waiting, _) => text("interaction-library-waiting").to_owned(),
        (State::Never, _) | (_, None) => text("interaction-library-never").to_owned(),
        (_, Some(l)) => match &l.outcome {
            Outcome::Failed(raw) => message(
                "interaction-library-unavailable",
                &[
                    (
                        "reason",
                        repo_scan::display_error(raw)
                            .unwrap_or_else(|| raw.clone())
                            .into(),
                    ),
                    ("when", when(l).into()),
                ],
            ),
            Outcome::Partial(stopped) => message(
                "interaction-library-partial",
                &[
                    ("found", l.found.into()),
                    ("why", stopped_words(*stopped).into()),
                    ("when", when(l).into()),
                ],
            ),
            Outcome::Complete => message(
                "interaction-library-done",
                &[("found", l.found.into()), ("when", when(l).into())],
            ),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::repo_scan::Progress;

    fn rec(outcome: Outcome, at: i64) -> ScanRecord {
        ScanRecord {
            at,
            found: 3,
            outcome,
        }
    }

    #[test]
    fn states_and_lines() {
        crate::i18n::set_language("en-US");
        let now = 10_000;
        let done = rec(Outcome::Complete, now - 30);
        let running = Some(Activity::Running(Progress {
            folders: 4,
            repos: 1,
        }));
        assert_eq!(state(false, running, Some(&done), now), State::Off);
        assert_eq!(state(true, running, Some(&done), now), State::Scanning);
        assert_eq!(
            state(true, Some(Activity::Queued), None, now),
            State::Waiting
        );
        assert_eq!(state(true, None, None, now), State::Never);
        assert_eq!(state(true, None, Some(&done), now), State::Done);
        let old = rec(Outcome::Complete, now - super::super::STALE_AFTER);
        assert_eq!(state(true, None, Some(&old), now), State::Stale);
        let cut = rec(Outcome::Partial(Stopped::Repos), now);
        assert_eq!(state(true, None, Some(&cut), now), State::Partial);
        let gone = rec(Outcome::Failed("/x isn't there any more.".into()), now);
        assert_eq!(state(true, None, Some(&gone), now), State::Unavailable);
        assert!(State::Unavailable.is_problem() && !State::Done.is_problem());

        assert_eq!(
            line(true, running, None, now),
            "Looked in 4 folders, found 1 repositories…"
        );
        assert_eq!(
            line(true, None, Some(&done), now),
            "3 repositories · just now"
        );
        assert_eq!(
            line(true, None, Some(&cut), now),
            "Partial: repository limit reached · 3 found · just now"
        );
        assert_eq!(
            line(true, None, Some(&gone), now),
            "Unavailable: /x isn't there any more. · just now"
        );
        assert_eq!(line(false, None, Some(&done), now), "Off");
        crate::i18n::set_language("de");
        assert!(line(true, None, None, now).starts_with("Noch nicht"));
        crate::i18n::set_language("en-US");
    }
}
