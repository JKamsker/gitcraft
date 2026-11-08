//! First-time hints: a line the first time something related is done, about what else it can
//! do. Each shows at most once ever (the ones seen are kept in the settings once drawn), none
//! while tips are turned off, and only one at a time.

use std::time::{Duration, Instant};

use crate::settings::Settings;

/// How long a hint waits before it shows: long enough for the second click of a
/// double-click, which takes back the hint about double-clicking.
pub(crate) const HINT_SETTLE: Duration = Duration::from_millis(450);

/// How long a hint stays up once shown, unless closed before.
pub(crate) const HINT_FOR: Duration = Duration::from_secs(20);

/// The hints there are.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Hint {
    /// After selecting a commit.
    CompareCommits,
    /// After a single click on a branch pill.
    DoubleClickPill,
    /// After hiding a branch from the graph.
    SoloBranch,
    /// After opening a diff.
    ChangeNavigation,
}

impl Hint {
    #[cfg(test)]
    pub(crate) const ALL: [Hint; 4] = [
        Hint::CompareCommits,
        Hint::DoubleClickPill,
        Hint::SoloBranch,
        Hint::ChangeNavigation,
    ];

    /// What the settings keep it as, once seen: never changes.
    pub(crate) fn id(self) -> &'static str {
        match self {
            Hint::CompareCommits => "compare-commits",
            Hint::DoubleClickPill => "double-click-pill",
            Hint::SoloBranch => "solo-branch",
            Hint::ChangeNavigation => "change-navigation",
        }
    }

    /// The line shown.
    pub(crate) fn text(self) -> String {
        let cmd = crate::manual::command();
        match self {
            Hint::CompareCommits => crate::i18n::message(
                "repo-workflows-guide-text-414ed52e",
                &[("cmd", (cmd).into())],
            ),
            Hint::DoubleClickPill => crate::i18n::text("repo-workflows-guide-text-e94f4321").into(),
            Hint::SoloBranch => crate::i18n::text("repo-workflows-guide-text-aa7b18c5").into(),
            Hint::ChangeNavigation => {
                crate::i18n::text("repo-workflows-guide-text-c09ad9f4").into()
            }
        }
    }
}

/// Where the hint of a tab stands at some moment.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum HintState {
    None,
    /// Offered, shown after this long.
    Settling(Duration),
    /// Shown, for this much longer.
    Showing(Hint, Duration),
}

/// The hint of a tab, if any: which, when it was offered, and whether it was drawn.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Hints {
    up: Option<(Hint, Instant, bool)>,
}

impl Hints {
    /// Where the hint stands at `now`: one closed, or up for [`HINT_FOR`], is gone.
    pub(crate) fn state(&self, now: Instant) -> HintState {
        let Some((hint, since, _)) = self.up else {
            return HintState::None;
        };
        let age = now.saturating_duration_since(since);
        if let Some(wait) = HINT_SETTLE.checked_sub(age).filter(|w| !w.is_zero()) {
            return HintState::Settling(wait);
        }
        match (HINT_SETTLE + HINT_FOR).checked_sub(age) {
            Some(left) if !left.is_zero() => HintState::Showing(hint, left),
            _ => HintState::None,
        }
    }

    /// Puts up `hint` at `now` if it has never been shown, tips are on, and no other hint is
    /// up. Whether it was put up.
    pub(crate) fn offer(&mut self, hint: Hint, settings: &Settings, now: Instant) -> bool {
        let busy = self.state(now) != HintState::None;
        if !settings.tips || settings.seen_hints.contains(hint.id()) || busy {
            return false;
        }
        self.up = Some((hint, now, false));
        true
    }

    /// The hint up is being drawn: counts it as seen in `settings`. Whether that changed them.
    pub(crate) fn drawn(&mut self, settings: &mut Settings) -> bool {
        let Some((hint, _, drawn)) = &mut self.up else {
            return false;
        };
        *drawn = true;
        settings.seen_hints.insert(hint.id().to_owned())
    }

    /// Takes `hint` back if it is up but not drawn yet (the click it was for was the first of
    /// a double-click): it stays unseen.
    pub(crate) fn withdraw(&mut self, hint: Hint) {
        if matches!(self.up, Some((h, _, false)) if h == hint) {
            self.up = None;
        }
    }

    pub(crate) fn close(&mut self) {
        self.up = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// After the hint offered at `at` has settled.
    fn shown(at: Instant) -> Instant {
        at + HINT_SETTLE
    }

    #[test]
    fn ids_are_distinct_and_texts_say_something() {
        let mut ids: Vec<&str> = Hint::ALL.iter().map(|h| h.id()).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), Hint::ALL.len());
        for hint in Hint::ALL {
            let text = hint.text();
            assert!(text.starts_with("Hint: "), "{text}");
            assert!(text.chars().count() < 100, "{text}");
        }
        assert!(
            Hint::CompareCommits
                .text()
                .contains(crate::manual::command())
        );
    }

    #[test]
    fn a_hint_shows_once_ever_once_drawn() {
        let mut settings = Settings::default();
        let mut hints = Hints::default();
        let now = Instant::now();
        assert!(hints.offer(Hint::CompareCommits, &settings, now));
        // Not seen until drawn.
        assert!(settings.seen_hints.is_empty());
        assert_eq!(
            hints.state(shown(now)),
            HintState::Showing(Hint::CompareCommits, HINT_FOR)
        );
        assert!(hints.drawn(&mut settings));
        assert!(!hints.drawn(&mut settings));
        assert!(settings.seen_hints.contains("compare-commits"));
        hints.close();
        assert!(!hints.offer(Hint::CompareCommits, &settings, now));
        // Another tab (with hints of its own) doesn't show it again either.
        assert!(!Hints::default().offer(Hint::CompareCommits, &settings, now));
    }

    #[test]
    fn a_hint_never_drawn_can_come_again() {
        let settings = Settings::default();
        let mut hints = Hints::default();
        let now = Instant::now();
        assert!(hints.offer(Hint::SoloBranch, &settings, now));
        let later = shown(now) + HINT_FOR;
        assert_eq!(hints.state(later), HintState::None);
        assert!(hints.offer(Hint::SoloBranch, &settings, later));
    }

    #[test]
    fn hints_do_not_stack_and_one_waiting_is_not_counted_as_seen() {
        let mut settings = Settings::default();
        let mut hints = Hints::default();
        let now = Instant::now();
        assert!(hints.offer(Hint::DoubleClickPill, &settings, now));
        assert!(!hints.offer(Hint::CompareCommits, &settings, now));
        hints.drawn(&mut settings);
        assert!(!settings.seen_hints.contains("compare-commits"));
        // Once the first is gone, by itself, the other can show.
        let later = shown(now) + HINT_FOR;
        assert_eq!(hints.state(later), HintState::None);
        assert!(hints.offer(Hint::CompareCommits, &settings, later));
    }

    #[test]
    fn a_double_click_takes_back_the_hint_of_its_first_click() {
        let mut settings = Settings::default();
        let mut hints = Hints::default();
        let now = Instant::now();
        hints.offer(Hint::DoubleClickPill, &settings, now);
        assert!(matches!(hints.state(now), HintState::Settling(_)));
        hints.withdraw(Hint::DoubleClickPill);
        assert_eq!(hints.state(shown(now)), HintState::None);
        assert!(settings.seen_hints.is_empty());
        // Once drawn, it stays.
        hints.offer(Hint::DoubleClickPill, &settings, now);
        hints.drawn(&mut settings);
        hints.withdraw(Hint::DoubleClickPill);
        assert!(matches!(
            hints.state(shown(now)),
            HintState::Showing(Hint::DoubleClickPill, _)
        ));
    }

    #[test]
    fn no_hints_while_tips_are_dismissed() {
        let settings = Settings {
            tips: false,
            ..Settings::default()
        };
        let mut hints = Hints::default();
        for hint in Hint::ALL {
            assert!(!hints.offer(hint, &settings, Instant::now()));
        }
    }

    #[test]
    fn a_hint_counts_down_while_up() {
        let settings = Settings::default();
        let mut hints = Hints::default();
        let now = Instant::now();
        hints.offer(Hint::SoloBranch, &settings, now);
        assert_eq!(
            hints.state(shown(now) + Duration::from_secs(5)),
            HintState::Showing(Hint::SoloBranch, HINT_FOR - Duration::from_secs(5))
        );
    }
}
