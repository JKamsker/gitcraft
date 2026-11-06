//! The order the palette lists its entries in for what is typed.

use std::cmp::Reverse;

use super::items::Item;
use super::recent::Recent;
use super::score::{self, Match};

/// How many of the entries listed only once something is typed (checking out each branch) are
/// listed at most.
const TYPED_ONLY: usize = 5;

/// An entry listed: which, and the characters of its label that matched.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Shown {
    pub index: usize,
    pub positions: Vec<usize>,
}

/// The entries matching `typed`, best first. With nothing typed: what is selected, then what
/// was run lately (the latest first), then the rest in their order. With something typed: by
/// how well they match, what was run lately scoring higher; of equal scores, the selection's
/// first, then the shorter label.
pub(crate) fn rank(items: &[Item], typed: &str, recent: &Recent) -> Vec<Shown> {
    let query = score::query(typed);
    if query.is_empty() {
        return untyped(items, recent);
    }
    let mut found: Vec<(i32, usize, Match)> = items
        .iter()
        .enumerate()
        .filter_map(|(k, item)| {
            let m = score::score(&item.folded, &query)?;
            let bonus = recent.bonus(&item.key);
            Some((m.score + bonus, k, m))
        })
        .collect();
    // The selection's entries win a tie, never over a better match.
    found.sort_by_key(|&(score, k, _)| {
        let item = &items[k];
        (Reverse(score), !item.selection, item.len, k)
    });
    let mut typed_only = 0;
    found
        .into_iter()
        .filter(|(_, k, _)| {
            if !items[*k].typed_only {
                return true;
            }
            typed_only += 1;
            typed_only <= TYPED_ONLY
        })
        .map(|(_, index, m)| Shown {
            index,
            positions: m.positions,
        })
        .collect()
}

fn untyped(items: &[Item], recent: &Recent) -> Vec<Shown> {
    let mut order: Vec<(u8, usize, usize)> = items
        .iter()
        .enumerate()
        .filter(|(_, item)| !item.typed_only)
        .map(|(k, item)| {
            let (group, rank) = match (item.selection, recent.rank(&item.key)) {
                (true, _) => (0, 0),
                (false, Some(r)) => (1, r),
                (false, None) => (2, 0),
            };
            (group, rank, k)
        })
        .collect();
    order.sort_unstable();
    order
        .into_iter()
        .map(|(_, _, index)| Shown {
            index,
            positions: Vec::new(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::palette::items::{AppCommand, Entry};

    fn item(label: &str) -> Item {
        Item::new(label, "", Entry::App(AppCommand::Settings))
    }

    fn labels<'a>(items: &'a [Item], typed: &str, recent: &Recent) -> Vec<&'a str> {
        rank(items, typed, recent)
            .into_iter()
            .map(|s| items[s.index].label.as_str())
            .collect()
    }

    fn sample() -> Vec<Item> {
        let mut items: Vec<Item> = [
            "Fetch",
            "Pull",
            "Pull (rebase)",
            "Create branch",
            "Check out a branch…",
            "Merge into dev…",
            "Rebase dev onto…",
            "Interactive rebase dev onto…",
        ]
        .into_iter()
        .map(item)
        .collect();
        let mut picked = item("Cherry-pick a301da9");
        picked.selection = true;
        items.push(picked);
        items
    }

    #[test]
    fn reb_ranks_rebase_first() {
        let items = sample();
        let got = labels(&items, "reb", &Recent::default());
        assert_eq!(got[0], "Rebase dev onto…");
        assert!(got.contains(&"Pull (rebase)"));
        assert!(!got.contains(&"Fetch"));
    }

    #[test]
    fn with_nothing_typed_the_selection_then_the_recent_ones_come_first() {
        let items = sample();
        let mut recent = Recent::default();
        recent.used("Merge into dev…");
        recent.used("Pull");
        let got = labels(&items, "", &recent);
        assert_eq!(
            got[..4],
            ["Cherry-pick a301da9", "Pull", "Merge into dev…", "Fetch"]
        );
        assert_eq!(got.len(), items.len());
    }

    #[test]
    fn the_selection_wins_a_tie_but_not_over_a_better_match() {
        let mut items: Vec<Item> = ["Revert b76ec404", "Rebase main onto…", "Push", "Pull"]
            .into_iter()
            .map(item)
            .collect();
        items[0].selection = true;
        assert_eq!(
            labels(&items, "reb", &Recent::default())[0],
            "Rebase main onto…"
        );
        assert_eq!(
            labels(&items, "rev", &Recent::default())[0],
            "Revert b76ec404"
        );
        items[3].selection = true;
        assert_eq!(labels(&items, "pu", &Recent::default())[0], "Pull");
    }

    #[test]
    fn a_recent_entry_wins_over_an_equal_match() {
        let items: Vec<Item> = ["Push", "Pull"].into_iter().map(item).collect();
        assert_eq!(labels(&items, "pu", &Recent::default())[0], "Push");
        let mut recent = Recent::default();
        recent.used("Pull");
        assert_eq!(labels(&items, "pu", &recent)[0], "Pull");
    }

    #[test]
    fn entries_known_by_a_key_are_remembered_by_it() {
        let mut items: Vec<Item> = ["Merge into main…", "Fetch"]
            .into_iter()
            .map(item)
            .collect();
        items[0].key = "pick:merge".to_owned();
        let mut recent = Recent::default();
        recent.used("pick:merge");
        assert_eq!(labels(&items, "", &recent)[0], "Merge into main…");
    }

    #[test]
    fn branches_listed_only_when_typed_are_few_and_never_listed_untyped() {
        let mut items = sample();
        for k in 0..5000 {
            let mut b = item(&format!("Check out feature/branch-{k}"));
            b.typed_only = true;
            items.push(b);
        }
        assert_eq!(labels(&items, "", &Recent::default()).len(), sample().len());
        let got = labels(&items, "feature", &Recent::default());
        assert_eq!(got.len(), TYPED_ONLY);
        let got = labels(&items, "branch-4999", &Recent::default());
        assert_eq!(got[0], "Check out feature/branch-4999");
    }

    #[test]
    fn nothing_matches_an_impossible_query() {
        let items = sample();
        assert!(labels(&items, "zzz", &Recent::default()).is_empty());
        assert!(labels(&[], "a", &Recent::default()).is_empty());
        assert!(labels(&[], "", &Recent::default()).is_empty());
    }

    #[test]
    fn ten_thousand_branches_rank_quickly() {
        let items: Vec<Item> = (0..10_000)
            .map(|k| item(&format!("team-{}/feature/branch-number-{k}", k % 37)))
            .collect();
        let started = std::time::Instant::now();
        for typed in ["f", "fe", "feat", "featnum", "team-3/branch-number-99"] {
            assert!(!rank(&items, typed, &Recent::default()).is_empty());
        }
        // Generous for a debug build on a slow machine; it takes a few milliseconds in release.
        assert!(started.elapsed() < std::time::Duration::from_secs(3));
    }
}
