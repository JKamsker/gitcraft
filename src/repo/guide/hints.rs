//! First-time hints: a line the first time something related is done, about what else it can
//! do. Each shows at most once ever (the ones seen are kept in the settings), none while tips
//! are turned off, and only one at a time.

use std::time::{Duration, Instant};

use crate::settings::Settings;

/// How long a hint stays up, unless closed before.
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
            Hint::CompareCommits => {
                format!("Tip: {cmd}+click another commit to compare the two")
            }
            Hint::DoubleClickPill => "Tip: double-click a pill to check that branch out".into(),
            Hint::SoloBranch => "Tip: Solo a branch from its menu to see only it".into(),
            Hint::ChangeNavigation => {
                "Tip: F7 or Alt+↓ goes to the next change, Shift+F7 or Alt+↑ to the previous one"
                    .into()
            }
        }
    }
}

/// The hint up in a tab, if any.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Hints {
    up: Option<(Hint, Instant)>,
}

impl Hints {
    /// The hint showing at `now`: one closed or up for [`HINT_FOR`] is gone.
    pub(crate) fn current(&self, now: Instant) -> Option<(Hint, Duration)> {
        let (hint, since) = self.up?;
        let left = HINT_FOR.checked_sub(now.saturating_duration_since(since))?;
        (!left.is_zero()).then_some((hint, left))
    }

    /// Shows `hint` at `now` if it has never been shown, tips are on, and no other hint is up;
    /// then counts it as seen in `settings`. Whether it was shown.
    pub(crate) fn offer(&mut self, hint: Hint, settings: &mut Settings, now: Instant) -> bool {
        if !settings.tips || settings.seen_hints.contains(hint.id()) || self.current(now).is_some()
        {
            return false;
        }
        settings.seen_hints.insert(hint.id().to_owned());
        self.up = Some((hint, now));
        true
    }

    pub(crate) fn close(&mut self) {
        self.up = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_distinct_and_texts_say_something() {
        let mut ids: Vec<&str> = Hint::ALL.iter().map(|h| h.id()).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), Hint::ALL.len());
        for hint in Hint::ALL {
            let text = hint.text();
            assert!(text.starts_with("Tip: "), "{text}");
            assert!(text.chars().count() < 100, "{text}");
        }
        assert!(
            Hint::CompareCommits
                .text()
                .contains(crate::manual::command())
        );
    }

    #[test]
    fn a_hint_shows_once_ever() {
        let mut settings = Settings::default();
        let mut hints = Hints::default();
        let now = Instant::now();
        assert!(hints.offer(Hint::CompareCommits, &mut settings, now));
        assert!(settings.seen_hints.contains("compare-commits"));
        hints.close();
        assert!(!hints.offer(Hint::CompareCommits, &mut settings, now));
        // Another tab (with hints of its own) doesn't show it again either.
        assert!(!Hints::default().offer(Hint::CompareCommits, &mut settings, now));
    }

    #[test]
    fn hints_do_not_stack_and_one_waiting_is_not_counted_as_seen() {
        let mut settings = Settings::default();
        let mut hints = Hints::default();
        let now = Instant::now();
        assert!(hints.offer(Hint::DoubleClickPill, &mut settings, now));
        assert!(!hints.offer(Hint::CompareCommits, &mut settings, now));
        assert!(!settings.seen_hints.contains("compare-commits"));
        assert_eq!(
            hints.current(now).map(|(h, _)| h),
            Some(Hint::DoubleClickPill)
        );
        // Once the first is gone, by itself, the other can show.
        let later = now + HINT_FOR;
        assert_eq!(hints.current(later), None);
        assert!(hints.offer(Hint::CompareCommits, &mut settings, later));
    }

    #[test]
    fn no_hints_while_tips_are_dismissed() {
        let mut settings = Settings {
            tips: false,
            ..Settings::default()
        };
        let mut hints = Hints::default();
        for hint in Hint::ALL {
            assert!(!hints.offer(hint, &mut settings, Instant::now()));
        }
        assert!(settings.seen_hints.is_empty());
    }

    #[test]
    fn a_hint_counts_down_while_up() {
        let mut settings = Settings::default();
        let mut hints = Hints::default();
        let now = Instant::now();
        hints.offer(Hint::SoloBranch, &mut settings, now);
        let left = hints
            .current(now + Duration::from_secs(5))
            .map(|(_, left)| left);
        assert_eq!(left, Some(HINT_FOR - Duration::from_secs(5)));
    }
}
