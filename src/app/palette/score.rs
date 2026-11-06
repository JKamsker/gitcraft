//! How well a palette entry matches what is typed: the letters typed must appear in the
//! entry in order; matches at the start of words, letters typed one after the other and a match
//! at the very start score higher, so that `reb` puts _Rebase…_ above _Pull (rebase)_ and `cp`
//! finds _Cherry-pick_.

/// Points for each letter matched.
const MATCH: i32 = 16;
/// A letter matched at the start of a word (`Check out`, `cherry-pick`, `feat/x`, `camelCase`).
const WORD_START: i32 = 24;
/// The first letter of the entry matched: what is typed is its beginning.
const PREFIX: i32 = 24;
/// A letter matched right after the one matched before it.
const CONSECUTIVE: i32 = 20;
/// Letters skipped between two matched ones: the first of them, and each one after.
const GAP_OPEN: i32 = 3;
const GAP_EXTEND: i32 = 1;
/// Longer than this, an entry or a query is matched letter by letter without looking for the
/// best of several ways to match (a branch name pasted whole is still found).
const MAX_CELLS: usize = 64 * 1024;

/// How an entry matched: its score, and the characters (indices into its `chars()`) matched.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Match {
    pub score: i32,
    pub positions: Vec<usize>,
}

/// A text folded for matching: each character lowercased (one character for one, so indices
/// stay those of the original), with whether a word starts there.
pub(crate) struct Folded {
    chars: Vec<char>,
    starts: Vec<bool>,
}

impl Folded {
    pub(crate) fn new(text: &str) -> Self {
        let original: Vec<char> = text.chars().collect();
        let starts = (0..original.len())
            .map(|i| word_start(&original, i))
            .collect();
        Self {
            chars: original.into_iter().map(fold).collect(),
            starts,
        }
    }
}

/// What is typed, folded like the entries; spaces are left out (they only separate words).
pub(crate) fn query(typed: &str) -> Vec<char> {
    typed
        .chars()
        .filter(|c| !c.is_whitespace())
        .map(fold)
        .collect()
}

/// `c` lowercased, as one character: `İ` lowercases to two, and is matched by the first.
fn fold(c: char) -> char {
    c.to_lowercase().next().unwrap_or(c)
}

/// A word starts at `i`: the first character, one after a separator, or an uppercase letter
/// after a lowercase one.
fn word_start(chars: &[char], i: usize) -> bool {
    let Some(&c) = chars.get(i) else {
        return false;
    };
    if !c.is_alphanumeric() {
        return false;
    }
    match i.checked_sub(1).and_then(|p| chars.get(p)) {
        None => true,
        Some(&prev) => {
            !prev.is_alphanumeric()
                || (prev.is_lowercase() && c.is_uppercase())
                || (prev.is_alphabetic() != c.is_alphabetic())
        }
    }
}

/// The best match of `query` in `text`, or `None` when its letters are not all in it in order.
/// An empty query matches everything with a score of 0.
pub(crate) fn score(text: &Folded, query: &[char]) -> Option<Match> {
    if query.is_empty() {
        return Some(Match {
            score: 0,
            positions: Vec::new(),
        });
    }
    if !is_subsequence(&text.chars, query) {
        return None;
    }
    if text.chars.len().saturating_mul(query.len()) > MAX_CELLS {
        return Some(greedy(text, query));
    }
    Some(best(text, query))
}

fn is_subsequence(text: &[char], query: &[char]) -> bool {
    let mut it = text.iter();
    query.iter().all(|q| it.any(|c| c == q))
}

/// Points for matching the character at `j`, whatever came before it.
fn letter(text: &Folded, j: usize) -> i32 {
    let mut s = MATCH;
    if text.starts[j] {
        s += WORD_START;
    }
    if j == 0 {
        s += PREFIX;
    }
    s
}

/// The best-scoring way to match, over every one (a table of the best score with the `i`th
/// letter typed matched at each `j`th character, and where the letter before it was matched).
fn best(text: &Folded, query: &[char]) -> Match {
    let (n, m) = (query.len(), text.chars.len());
    const NONE: i32 = i32::MIN / 2;
    let mut table = vec![NONE; n * m];
    let mut from = vec![usize::MAX; n * m];
    for (i, &q) in query.iter().enumerate() {
        // The best of the row above, at least two characters back, less the gap since it.
        let mut run = NONE;
        let mut run_at = usize::MAX;
        for j in 0..m {
            if i > 0 && j >= 2 {
                let back = table[(i - 1) * m + j - 2];
                let carried = run - GAP_EXTEND;
                if back > NONE && back - GAP_OPEN >= carried {
                    run = back - GAP_OPEN;
                    run_at = j - 2;
                } else {
                    run = carried;
                }
            }
            if text.chars[j] != q {
                continue;
            }
            let here = letter(text, j);
            let at = i * m + j;
            if i == 0 {
                // Letters skipped before the first match cost a little, so earlier is better.
                table[at] = here - (j as i32).min(10) * GAP_EXTEND;
                continue;
            }
            let next_to = if j >= 1 {
                table[(i - 1) * m + j - 1]
            } else {
                NONE
            };
            let joined = if next_to > NONE {
                next_to + CONSECUTIVE
            } else {
                NONE
            };
            if joined > NONE && joined >= run {
                table[at] = here + joined;
                from[at] = j - 1;
            } else if run > NONE / 2 && run_at != usize::MAX {
                table[at] = here + run;
                from[at] = run_at;
            }
        }
    }
    let last = (n - 1) * m;
    let (mut j, score) = (0..m)
        .map(|j| (j, table[last + j]))
        .max_by_key(|&(j, s)| (s, std::cmp::Reverse(j)))
        .unwrap_or((0, NONE));
    let mut positions = vec![0; n];
    for i in (0..n).rev() {
        positions[i] = j;
        j = from[i * m + j];
    }
    Match { score, positions }
}

/// Each letter typed matched as early as it can be, scored the same way.
fn greedy(text: &Folded, query: &[char]) -> Match {
    let mut positions = Vec::with_capacity(query.len());
    let mut start = 0;
    for q in query {
        if let Some(k) = text.chars[start..].iter().position(|c| c == q) {
            positions.push(start + k);
            start += k + 1;
        }
    }
    let mut score = 0;
    for (k, &j) in positions.iter().enumerate() {
        score += letter(text, j);
        match k.checked_sub(1).map(|p| positions[p]) {
            Some(prev) if prev + 1 == j => score += CONSECUTIVE,
            Some(prev) => score -= GAP_OPEN + (j - prev - 2) as i32 * GAP_EXTEND,
            None => score -= (j as i32).min(10) * GAP_EXTEND,
        }
    }
    Match { score, positions }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(text: &str, typed: &str) -> Option<Match> {
        score(&Folded::new(text), &query(typed))
    }

    /// The characters matched, empty when nothing is.
    fn at(text: &str, typed: &str) -> Vec<usize> {
        s(text, typed).map(|m| m.positions).unwrap_or_default()
    }

    fn rank<'a>(entries: &[&'a str], typed: &str) -> Vec<&'a str> {
        let mut scored: Vec<(i32, usize, &str)> = entries
            .iter()
            .enumerate()
            .filter_map(|(k, e)| s(e, typed).map(|m| (m.score, k, *e)))
            .collect();
        scored.sort_by_key(|&(score, k, e)| (std::cmp::Reverse(score), e.len(), k));
        scored.into_iter().map(|(_, _, e)| e).collect()
    }

    #[test]
    fn letters_must_appear_in_order() {
        assert!(s("Check out main", "cmain").is_some());
        assert!(s("Check out main", "co main").is_some());
        assert!(s("Fetch", "push").is_none());
        assert!(s("ab", "ba").is_none());
        assert!(s("", "a").is_none());
    }

    #[test]
    fn an_empty_query_matches_everything() {
        assert_eq!(s("Fetch", "").map(|m| m.score), Some(0));
        assert_eq!(s("", "").map(|m| m.score), Some(0));
        assert_eq!(s("Fetch", "   ").map(|m| m.score), Some(0));
    }

    #[test]
    fn reb_ranks_rebase_first() {
        let entries = [
            "Pull (rebase)",
            "Create branch",
            "Reset dev to here…",
            "Interactive rebase dev onto…",
            "Rebase dev onto…",
        ];
        assert_eq!(rank(&entries, "reb")[0], "Rebase dev onto…");
    }

    #[test]
    fn word_starts_beat_letters_inside_words() {
        let entries = ["Show CI for HEAD", "Cherry-pick a301da9"];
        assert_eq!(rank(&entries, "cp")[0], "Cherry-pick a301da9");
        let entries = ["Search commits", "New terminal", "Toggle terminal"];
        assert_eq!(rank(&entries, "nt")[0], "New terminal");
    }

    #[test]
    fn consecutive_letters_beat_scattered_ones() {
        let entries = ["Reset dev to here…", "Settings"];
        assert_eq!(rank(&entries, "set")[0], "Settings");
    }

    #[test]
    fn the_best_alignment_is_found_not_the_first() {
        // Greedy matching would take the `p` of `Open` and `u` of `out`.
        assert_eq!(at("Open pull request", "pull"), vec![5, 6, 7, 8]);
    }

    #[test]
    fn camel_case_and_separators_start_words() {
        assert_eq!(at("feat/paletteRanking", "pr"), vec![5, 12]);
        assert_eq!(at("fix_the-bug", "tb"), vec![4, 8]);
    }

    #[test]
    fn case_is_ignored_and_unicode_is_matched_by_character() {
        assert_eq!(
            at("Check out Grüße-ÄÖ", "grüßeäö"),
            vec![10, 11, 12, 13, 14, 16, 17]
        );
        // `İ` lowercases to two characters; the positions stay those of the original.
        assert_eq!(at("İstanbul x", "ix"), vec![0, 9]);
        assert!(s("日本語のブランチ", "ブラ").is_some());
        assert!(s("emoji 🚀 launch", "🚀l").is_some());
    }

    #[test]
    fn spaces_in_the_query_only_separate_words() {
        assert_eq!(s("Check out main", "c o m"), s("Check out main", "com"));
    }

    #[test]
    fn huge_inputs_fall_back_to_matching_letter_by_letter() {
        let long = "x".repeat(100_000) + "needle";
        let m = at(&long, "needle");
        assert_eq!(m.len(), 6);
        assert_eq!(m.first(), Some(&100_000));
        let huge_query = "y".repeat(10_000);
        assert!(s("short", &huge_query).is_none());
        assert!(s(&"y".repeat(20_000), &huge_query).is_some());
    }

    #[test]
    fn positions_are_increasing_and_point_at_the_letters_typed() {
        for (text, typed) in [
            ("Merge into dev…", "mid"),
            ("Changes feature adds to dev", "cfad"),
            ("aaaaaaaaab", "aab"),
            ("a-a-a-a-b", "ab"),
        ] {
            let folded: Vec<char> = text.chars().map(fold).collect();
            let q = query(typed);
            let m = at(text, typed);
            assert!(m.windows(2).all(|w| w[0] < w[1]), "{text}");
            let got: Vec<char> = m.iter().map(|&p| folded[p]).collect();
            assert_eq!(got, q, "{text}");
        }
    }
}
