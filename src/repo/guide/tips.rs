//! The "Did you know" line: one of the manual's tips, a different one each day, short enough
//! for one line.

use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};

/// Longer points are cut at the end of a sentence that fits, or left out.
pub(crate) const MAX_TIP_CHARS: usize = 95;

/// `point`, or its sentences that fit in `max` characters; `None` when not even the first one
/// does (or it is blank).
pub(crate) fn fit(point: &str, max: usize) -> Option<String> {
    let point = point.trim();
    if point.is_empty() {
        return None;
    }
    if point.chars().count() <= max {
        return Some(point.to_owned());
    }
    // The last end of a sentence (a full stop before a space) within `max` characters.
    let chars: Vec<(usize, char)> = point.char_indices().take(max + 1).collect();
    let end = (0..chars.len().min(max)).rev().find_map(|n| {
        let (at, c) = chars[n];
        let next = chars.get(n + 1).map(|&(_, next)| next);
        (c == '.' && next == Some(' ')).then_some(at + 1)
    })?;
    Some(point[..end].to_owned())
}

/// The tips from `points`, each fitted to [`MAX_TIP_CHARS`], with `Ctrl` written the way this
/// system says it.
pub(crate) fn candidates<'a>(points: impl IntoIterator<Item = &'a str>) -> Vec<String> {
    points
        .into_iter()
        .map(|p| p.replace("Ctrl", crate::manual::command()))
        .filter_map(|p| fit(&p, MAX_TIP_CHARS))
        .collect()
}

/// The manual's tips, made once.
pub(crate) fn manual_tips() -> &'static [String] {
    static TIPS: OnceLock<Vec<String>> = OnceLock::new();
    TIPS.get_or_init(|| candidates(crate::manual::tips().iter().copied()))
}

/// The tip for `seed`: the same one for the same seed, the next one for the next seed.
pub(crate) fn pick(tips: &[String], seed: u64) -> Option<&str> {
    if tips.is_empty() {
        return None;
    }
    let at = (seed % tips.len() as u64) as usize;
    tips.get(at).map(String::as_str)
}

/// The day `now` is in, counted from 1970 in UTC (there is no time-zone library to tell the
/// local day by): the tip changes once a day, at midnight UTC.
pub(crate) fn day_seed(now: SystemTime) -> u64 {
    now.duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() / 86_400)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn a_short_point_is_kept_whole() {
        assert_eq!(fit("  Short one.  ", 20).as_deref(), Some("Short one."));
        assert_eq!(fit("", 20), None);
        assert_eq!(fit("   ", 20), None);
    }

    #[test]
    fn a_long_point_is_cut_at_the_last_sentence_that_fits() {
        let point = "First part here. Second part here. Third part that is far too long.";
        assert_eq!(
            fit(point, 40).as_deref(),
            Some("First part here. Second part here.")
        );
        assert_eq!(fit(point, 20).as_deref(), Some("First part here."));
        // Not even the first sentence fits: left out.
        assert_eq!(fit(point, 10), None);
        // A full stop inside a word (a file name) is no end of a sentence.
        assert_eq!(
            fit("Edit .gitignore and README.md to taste, then", 20),
            None
        );
    }

    #[test]
    fn cutting_counts_characters_not_bytes() {
        let point = "Ümlaut “quoted” → here. Then much more text that won't fit.";
        assert_eq!(fit(point, 30).as_deref(), Some("Ümlaut “quoted” → here."));
    }

    #[test]
    fn the_manual_gives_tips_that_fit_on_one_line() {
        let tips = manual_tips();
        assert!(tips.len() > 10, "only {} tips", tips.len());
        // Every one fits whole: none is cut or left out.
        assert_eq!(tips.len(), crate::manual::tips().len());
        for tip in tips {
            assert!(!tip.trim().is_empty());
            assert!(tip.chars().count() <= MAX_TIP_CHARS, "too long: {tip}");
            assert!(
                cfg!(target_os = "macos") || !tip.contains("Cmd"),
                "written for macOS: {tip}"
            );
        }
        for (tip, written) in tips.iter().zip(crate::manual::tips()) {
            assert_eq!(*tip, written.replace("Ctrl", crate::manual::command()));
        }
    }

    #[test]
    fn picking_is_deterministic_never_empty_and_walks_every_tip() {
        let tips = manual_tips();
        for seed in [0, 1, 7, 20_000, u64::MAX] {
            let tip = pick(tips, seed);
            assert!(tip.is_some_and(|t| !t.is_empty()));
            assert_eq!(pick(tips, seed), tip);
        }
        let mut seen: Vec<&str> = (0..tips.len() as u64)
            .filter_map(|s| pick(tips, s))
            .collect();
        seen.sort_unstable();
        seen.dedup();
        assert_eq!(seen.len(), tips.len());
        assert_ne!(pick(tips, 3), pick(tips, 4));
        assert_eq!(pick(&[], 3), None);
        let one = ["Only tip.".to_owned()];
        assert_eq!(pick(&one, u64::MAX), Some("Only tip."));
    }

    #[test]
    fn the_seed_changes_once_a_day() {
        let day = |secs| day_seed(UNIX_EPOCH + Duration::from_secs(secs));
        assert_eq!(day(0), 0);
        assert_eq!(day(86_399), 0);
        assert_eq!(day(86_400), 1);
        assert_eq!(day_seed(UNIX_EPOCH - Duration::from_secs(5)), 0);
    }
}
