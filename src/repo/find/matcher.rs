//! What Find looks for: the text typed, as it is or as a regular expression, with or without
//! matching case and only as whole words; and where it is found in a line.

use std::ops::Range;

use regex::{Regex, RegexBuilder};

/// Longer patterns aren't compiled: a regular expression this big is a mistake, and building it
/// would take long on the window's thread.
const MAX_PATTERN: usize = 1000;
/// Bounds what one pattern may compile to (the default is 10 MiB).
const SIZE_LIMIT: usize = 2 << 20;

/// What was typed into the find bar and its three toggles.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub(crate) struct Query {
    pub text: String,
    pub case_sensitive: bool,
    pub whole_word: bool,
    pub regex: bool,
}

/// A query made ready to search lines with.
#[derive(Clone, Debug)]
pub(crate) struct Matcher {
    re: Regex,
}

impl Matcher {
    /// The matcher for `query`; `Ok(None)` when there's nothing to look for, `Err` with a
    /// message for the find bar when the regular expression is not valid.
    pub(crate) fn new(query: &Query) -> Result<Option<Self>, String> {
        if query.text.is_empty() {
            return Ok(None);
        }
        if query.text.chars().count() > MAX_PATTERN {
            return Err(format!(
                "The text to find is too long (more than {MAX_PATTERN} characters)"
            ));
        }
        let body = if query.regex {
            query.text.clone()
        } else {
            regex::escape(&query.text)
        };
        // Half word boundaries: a match that begins or ends with a symbol (`-x`, `a.`) still
        // counts as a whole word when nothing that is part of a word touches it.
        let pattern = if query.whole_word {
            format!(r"\b{{start-half}}(?:{body})\b{{end-half}}")
        } else {
            body
        };
        RegexBuilder::new(&pattern)
            .case_insensitive(!query.case_sensitive)
            .size_limit(SIZE_LIMIT)
            .build()
            .map(|re| Some(Self { re }))
            .map_err(|e| regex_error(&e))
    }

    /// Where the query is in `line`, as ranges of characters (not bytes), in order, none
    /// overlapping: after a match the search goes on where it ended, as editors do. Empty
    /// matches (`^`, `x*`) are left out, there would be nothing to show.
    #[cfg(test)]
    pub(crate) fn find(&self, line: &str) -> Vec<Range<usize>> {
        self.find_limited(line, usize::MAX)
    }
    pub(crate) fn find_limited(&self, line: &str, limit: usize) -> Vec<Range<usize>> {
        let mut out = Vec::new();
        // Bytes to characters, counted on from the previous match rather than from the start.
        let (mut byte, mut chars) = (0usize, 0usize);
        let mut to_char = |at: usize| {
            chars += line.get(byte..at).map_or(0, |s| s.chars().count());
            byte = at;
            chars
        };
        for m in self.re.find_iter(line) {
            if m.is_empty() {
                continue;
            }
            let start = to_char(m.start());
            let end = to_char(m.end());
            out.push(start..end);
            if out.len() >= limit {
                break;
            }
        }
        out
    }
}

/// A one-line message for a regular expression that doesn't compile.
fn regex_error(e: &regex::Error) -> String {
    match e {
        regex::Error::CompiledTooBig(_) => {
            "The regular expression is too big; try a simpler one".to_owned()
        }
        // The syntax error's own text ends with a line saying what is wrong, below the pattern
        // and a caret.
        _ => {
            let text = e.to_string();
            let why = text
                .lines()
                .rev()
                .find_map(|l| l.trim().strip_prefix("error:"))
                .map(str::trim)
                .unwrap_or("it can't be read");
            format!("Not a valid regular expression: {why}")
        }
    }
}

#[cfg(test)]
// Expected matches are lists of ranges, some of one.
#[allow(clippy::single_range_in_vec_init)]
mod tests {
    use super::*;

    fn query(text: &str) -> Query {
        Query {
            text: text.to_owned(),
            ..Query::default()
        }
    }

    fn find(q: &Query, line: &str) -> Vec<Range<usize>> {
        match Matcher::new(q) {
            Ok(Some(m)) => m.find(line),
            _ => Vec::new(),
        }
    }

    #[test]
    fn nothing_typed_looks_for_nothing() {
        assert!(matches!(Matcher::new(&query("")), Ok(None)));
    }

    #[test]
    fn plain_text_is_taken_literally_and_ignores_case_by_default() {
        assert_eq!(find(&query("a.b"), "aXb a.b A.B"), [4..7, 8..11]);
        assert_eq!(find(&query("(x"), "f(x)"), [1..3]);
        let mut q = query("Foo");
        q.case_sensitive = true;
        assert_eq!(find(&q, "foo Foo FOO"), [4..7]);
    }

    #[test]
    fn matches_never_overlap() {
        assert_eq!(find(&query("aa"), "aaa"), [0..2]);
        assert_eq!(find(&query("aa"), "aaaa"), [0..2, 2..4]);
        let mut q = query("a+");
        q.regex = true;
        assert_eq!(find(&q, "aa b a"), [0..2, 5..6]);
    }

    #[test]
    fn ranges_count_characters_not_bytes() {
        // "ä" and "日本" take more than one byte each; the emoji is two code points.
        assert_eq!(find(&query("x"), "äx 日本x"), [1..2, 5..6]);
        assert_eq!(find(&query("本"), "日本日本"), [1..2, 3..4]);
        assert_eq!(find(&query("👍🏽"), "a👍🏽b"), [1..3]);
    }

    #[test]
    fn case_is_folded_beyond_ascii() {
        assert_eq!(find(&query("straße"), "STRAßE"), [0..6]);
        assert_eq!(find(&query("ärger"), "ÄRGER"), [0..5]);
        assert_eq!(find(&query("σ"), "Σ"), [0..1]);
    }

    #[test]
    fn whole_words_only() {
        let mut q = query("foo");
        q.whole_word = true;
        assert_eq!(
            find(&q, "foo food _foo foo_ (foo) foo"),
            [0..3, 20..23, 25..28]
        );
        // Letters beyond ASCII are part of words too.
        assert_eq!(find(&q, "éfoo fooé foo"), [10..13]);
        // A match beginning or ending with a symbol.
        let mut q = query("-x");
        q.whole_word = true;
        assert_eq!(find(&q, "a -x b-x -xy"), [2..4]);
        // With a regular expression, its alternatives are all tried at each place.
        let mut q = query("a|ab");
        (q.regex, q.whole_word) = (true, true);
        assert_eq!(find(&q, "ab"), [0..2]);
    }

    #[test]
    fn regular_expressions() {
        let mut q = query(r"\d+");
        q.regex = true;
        assert_eq!(find(&q, "a1 b22 c333"), [1..2, 4..6, 8..11]);
        // Empty matches are left out.
        let mut q = query("x*");
        q.regex = true;
        assert_eq!(find(&q, "axxb"), [1..3]);
        let mut q = query("^");
        q.regex = true;
        assert!(find(&q, "abc").is_empty());
        // Typed as text, the same characters are only themselves.
        assert_eq!(find(&query(r"\d+"), r"1 \d+"), [2..5]);
    }

    #[test]
    fn a_broken_regular_expression_says_what_is_wrong() {
        for broken in ["(", "[a-", "a{2,1}", "*"] {
            let mut q = query(broken);
            q.regex = true;
            let err = Matcher::new(&q).err().unwrap_or_default();
            assert!(
                err.starts_with("Not a valid regular expression: ") && !err.contains('\n'),
                "{broken}: {err}"
            );
        }
        // The same text, not as a regular expression, is fine.
        assert!(matches!(Matcher::new(&query("(")), Ok(Some(_))));
    }

    #[test]
    fn huge_patterns_are_refused() {
        let long = "a".repeat(MAX_PATTERN + 1);
        assert!(Matcher::new(&query(&long)).is_err());
        let mut q = query(r"\w{1000}\w{1000}\w{1000}");
        q.regex = true;
        assert!(Matcher::new(&q).is_err_and(|e| e.contains("too big")));
    }

    #[test]
    fn empty_and_huge_lines() {
        assert!(find(&query("a"), "").is_empty());
        let line = "ab".repeat(50_000);
        let found = find(&query("b"), &line);
        assert_eq!(found.len(), 50_000);
        assert_eq!(found.last(), Some(&(99_999..100_000)));
    }
}
