//! What git leaves of a bisect, read as text: its terms, its refs, its log, the counts of
//! `rev-list --bisect-vars`, and the pathspec it was started with.

use super::{Marks, Outcome, Terms};

/// The terms of `BISECT_TERMS` (the bad one first, then the good one); git's own when the
/// file is missing or not as git writes it.
pub(super) fn terms(text: Option<&str>) -> Terms {
    let mut lines = text.unwrap_or_default().lines().map(str::trim);
    match (lines.next(), lines.next()) {
        (Some(bad), Some(good)) if !bad.is_empty() && !good.is_empty() && bad != good => Terms {
            bad: bad.to_owned(),
            good: good.to_owned(),
        },
        _ => Terms::default(),
    }
}

/// The marks in `for-each-ref --format='%(objectname) %(refname)' refs/bisect/`:
/// `refs/bisect/<bad>`, `refs/bisect/<good>-<hash>` and `refs/bisect/skip-<hash>`.
pub(super) fn marks(text: &str, terms: &Terms) -> Marks {
    let good_prefix = format!("{}-", terms.good);
    let mut marks = Marks::default();
    for line in text.lines() {
        let Some((hash, name)) = line.trim().split_once(' ') else {
            continue;
        };
        let Some(name) = name.strip_prefix("refs/bisect/") else {
            continue;
        };
        let hash = hash.to_owned();
        if name == terms.bad {
            marks.bad = Some(hash);
        } else if name.starts_with(&good_prefix) {
            marks.good.push(hash);
        } else if name.starts_with("skip-") {
            marks.skipped.push(hash);
        }
    }
    marks
}

/// How the bisect ended, as `git bisect log` says it: after the last command in it, the first
/// bad commit, or the commits it could be when only skipped ones were left (`bad`: the
/// bisect's word for bad, which git says them with). A mark given after the end (which git
/// takes) starts it again, so what was said before it no longer holds.
pub(super) fn outcome(log: &str, bad: &str) -> Option<Outcome> {
    let first = format!("# first {bad} commit: ");
    let possible_first = format!("# possible first {bad} commit: ");
    let after_last_command = log
        .lines()
        .rev()
        .take_while(|line| !line.starts_with("git bisect "))
        .collect::<Vec<_>>();
    let mut possible = Vec::new();
    // Back in the order git wrote them.
    for line in after_last_command.into_iter().rev() {
        if let Some(hash) = bracketed_hash(line, &first) {
            return Some(Outcome::FirstBad(hash));
        }
        if let Some(hash) = bracketed_hash(line, &possible_first) {
            possible.push(hash);
        }
    }
    (!possible.is_empty()).then_some(Outcome::OnlySkipped(possible))
}

/// The hash in `<prefix>[<hash>] <subject>`.
fn bracketed_hash(line: &str, prefix: &str) -> Option<String> {
    let rest = line.strip_prefix(prefix)?.strip_prefix('[')?;
    let (hash, _) = rest.split_once(']')?;
    hash.chars()
        .all(|c| c.is_ascii_hexdigit())
        .then(|| hash.to_owned())
        .filter(|h| !h.is_empty())
}

/// What `rev-list --bisect-vars` says that is used here.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct Vars {
    /// Commits that are still to be tested after the one picked (`bisect_nr`).
    pub(super) left: usize,
    /// Commits that can still be the first bad one, the bad one itself included
    /// (`bisect_all`).
    pub(super) candidates: usize,
}

/// The counts in `rev-list --bisect-vars`' `name=value` lines; `None` when one is missing.
pub(super) fn vars(text: &str) -> Option<Vars> {
    let value = |name: &str| {
        text.lines().find_map(|line| {
            let (key, value) = line.trim().split_once('=')?;
            (key == name).then(|| value.trim_matches('\'').parse::<usize>().ok())?
        })
    };
    Some(Vars {
        left: value("bisect_nr")?,
        candidates: value("bisect_all")?,
    })
}

/// About how many more commits are tested before the first bad one is known, with
/// `candidates` commits that can still be it: git's own estimate (`estimate_bisect_steps`),
/// which counts the halvings left, one fewer when the range is just over a power of two.
pub fn steps_left(candidates: usize) -> u32 {
    if candidates < 3 {
        return 0;
    }
    let n = candidates.ilog2();
    let whole = 1usize << n;
    let over = candidates - whole;
    if whole < 3 * over { n } else { n - 1 }
}

/// The paths of `BISECT_NAMES`, written by git as shell words in single quotes (`'a b'
/// 'it'\''s'`), a `!` escaped with a backslash between quotes.
pub(super) fn names(text: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut word: Option<String> = None;
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        match c {
            '\'' => {
                let w = word.get_or_insert_with(String::new);
                for q in chars.by_ref() {
                    if q == '\'' {
                        break;
                    }
                    w.push(q);
                }
            }
            '\\' => {
                if let Some(escaped) = chars.next() {
                    word.get_or_insert_with(String::new).push(escaped);
                }
            }
            c if c.is_whitespace() => words.extend(word.take()),
            c => word.get_or_insert_with(String::new).push(c),
        }
    }
    words.extend(word);
    words
}

/// Whether git's answer to `bisect log` (or any bisect command) says no bisect is going on.
pub(super) fn not_bisecting(err: &str) -> bool {
    err.contains("We are not bisecting")
        || err.contains("You need to start by \"git bisect start\"")
}

/// Whether a mark's error is git saying that only skipped commits are left: the end of the
/// bisect (exit code 2), not a failure.
pub(super) fn only_skipped_left(err: &str) -> bool {
    err.contains("There are only 'skip'ped commits left to test")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terms_come_from_the_file_or_are_git_s_own() {
        let custom = terms(Some("new\nold\n"));
        assert_eq!((custom.bad.as_str(), custom.good.as_str()), ("new", "old"));
        for broken in [None, Some(""), Some("bad\n"), Some("same\nsame\n")] {
            assert_eq!(terms(broken), Terms::default(), "{broken:?}");
        }
    }

    #[test]
    fn marks_are_read_from_the_bisect_refs() {
        let text = concat!(
            "aaa refs/bisect/bad\n",
            "bbb refs/bisect/good-bbb\n",
            "ccc refs/bisect/good-ccc\n",
            "ddd refs/bisect/skip-ddd\n",
            "eee refs/heads/main\n",
        );
        let m = marks(text, &Terms::default());
        assert_eq!(m.bad.as_deref(), Some("aaa"));
        assert_eq!(m.good, ["bbb", "ccc"]);
        assert_eq!(m.skipped, ["ddd"]);
        // With other terms, `bad` and `good-` mean nothing.
        let terms = Terms {
            bad: "new".to_owned(),
            good: "old".to_owned(),
        };
        let m = marks("fff refs/bisect/new\nggg refs/bisect/old-ggg\n", &terms);
        assert_eq!(m.bad.as_deref(), Some("fff"));
        assert_eq!(m.good, ["ggg"]);
        assert_eq!(marks(text, &terms).bad, None);
    }

    #[test]
    fn the_log_says_the_first_bad_commit() {
        let log = concat!(
            "# bad: [aaa] c10\n",
            "# good: [bbb] c1\n",
            "git bisect start 'HEAD' 'HEAD~9'\n",
            "# bad: [ccc] c5\n",
            "git bisect bad ccc\n",
            "# first bad commit: [ccc] c5\n",
        );
        assert_eq!(
            outcome(log, "bad"),
            Some(Outcome::FirstBad("ccc".to_owned()))
        );
        // Still going.
        assert_eq!(
            outcome(&log[..log.find("# first").unwrap_or(0)], "bad"),
            None
        );
    }

    /// git says it with the bisect's own words.
    #[test]
    fn the_log_says_it_in_the_bisect_s_terms() {
        let log = "git bisect broken ccc\n# first broken commit: [ccc] c5\n";
        assert_eq!(
            outcome(log, "broken"),
            Some(Outcome::FirstBad("ccc".to_owned()))
        );
        assert_eq!(outcome(log, "bad"), None);
        let log = "# only skipped commits left to test\n# possible first new commit: [ddd] c7\n";
        assert_eq!(
            outcome(log, "new"),
            Some(Outcome::OnlySkipped(vec!["ddd".to_owned()]))
        );
    }

    #[test]
    fn a_mark_after_the_end_starts_it_again() {
        let log = concat!(
            "git bisect bad ccc\n",
            "# first bad commit: [ccc] c5\n",
            "# good: [ccc] c5\n",
            "git bisect good ccc\n",
        );
        assert_eq!(outcome(log, "bad"), None);
    }

    #[test]
    fn the_log_says_what_it_could_be_when_only_skipped_ones_are_left() {
        let log = concat!(
            "git bisect skip ddd\n",
            "# only skipped commits left to test\n",
            "# possible first bad commit: [ddd] c7\n",
            "# possible first bad commit: [eee] c6\n",
        );
        assert_eq!(
            outcome(log, "bad"),
            Some(Outcome::OnlySkipped(vec![
                "ddd".to_owned(),
                "eee".to_owned()
            ]))
        );
    }

    #[test]
    fn a_bracket_holds_a_hash_or_nothing_is_taken() {
        assert_eq!(
            bracketed_hash("# first bad commit: [] x", "# first bad commit: "),
            None
        );
        assert_eq!(
            bracketed_hash("# first bad commit: [not hex] x", "# first bad commit: "),
            None
        );
    }

    #[test]
    fn bisect_vars_give_the_counts() {
        let text = concat!(
            "bisect_rev='1718e18a8a37c24249c7cd0f4313aa8bb6c6d7b9'\n",
            "bisect_nr=1\n",
            "bisect_good=1\n",
            "bisect_bad=1\n",
            "bisect_all=4\n",
            "bisect_steps=1\n",
        );
        assert_eq!(
            vars(text),
            Some(Vars {
                left: 1,
                candidates: 4
            })
        );
        assert_eq!(vars("bisect_nr=1\n"), None);
        assert_eq!(vars(""), None);
    }

    /// The same numbers git prints ("roughly N steps").
    #[test]
    fn steps_left_are_git_s_estimate() {
        for (candidates, steps) in [
            (0, 0),
            (1, 0),
            (2, 0),
            (3, 1),
            (4, 1),
            (5, 1),
            (6, 2),
            (8, 2),
            (9, 2),
            (11, 3),
            (16, 3),
            (100, 6),
            (1024, 9),
            (1500, 10),
        ] {
            assert_eq!(steps_left(candidates), steps, "{candidates}");
        }
    }

    #[test]
    fn bisect_names_are_shell_words() {
        assert_eq!(names(""), Vec::<String>::new());
        assert_eq!(names("'src/a.rs'\n"), ["src/a.rs"]);
        assert_eq!(names(" 'a b' 'it'\\''s' 'x'\\!'y'"), ["a b", "it's", "x!y"]);
    }

    #[test]
    fn git_s_answers_are_told_apart() {
        assert!(not_bisecting("error: We are not bisecting."));
        assert!(not_bisecting("You need to start by \"git bisect start\"\n"));
        assert!(!not_bisecting("fatal: not a git repository"));
        assert!(only_skipped_left(
            "There are only 'skip'ped commits left to test.\nThe first bad commit could be any of:"
        ));
        assert!(!only_skipped_left("error: Bad rev input: x"));
    }
}
