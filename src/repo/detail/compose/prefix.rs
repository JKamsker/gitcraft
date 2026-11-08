//! The type / scope prefix of a summary: Conventional Commits (`feat(ui)!: …`) or Gitmoji
//! (`✨ …`). Reading the one a summary has, and writing another in its place.

use std::sync::OnceLock;

use regex::Regex;

/// The Conventional Commits types, with what each is for.
pub const TYPES: &[(&str, &str)] = &[
    ("feat", "repo-core-0992"),
    ("fix", "repo-core-0994"),
    ("docs", "repo-core-0996"),
    ("style", "repo-core-0998"),
    ("refactor", "repo-core-1000"),
    ("perf", "repo-core-1002"),
    ("test", "repo-core-1004"),
    ("build", "repo-core-1006"),
    ("ci", "repo-core-1008"),
    ("chore", "repo-core-1010"),
    ("revert", "repo-core-1011"),
];

/// The Gitmoji most used, with what each is for.
pub const GITMOJI: &[(&str, &str)] = &[
    ("✨", "repo-core-1012"),
    ("🐛", "repo-core-1013"),
    ("🚑️", "repo-core-1014"),
    ("📝", "repo-core-1015"),
    ("🎨", "repo-core-1016"),
    ("♻️", "repo-core-1017"),
    ("⚡️", "repo-core-1018"),
    ("🔥", "repo-core-1019"),
    ("✅", "repo-core-1020"),
    ("🔧", "repo-core-1021"),
    ("👷", "repo-core-1022"),
    ("💚", "repo-core-1023"),
    ("⬆️", "repo-core-1024"),
    ("⬇️", "repo-core-1025"),
    ("🔒️", "repo-core-1026"),
    ("💄", "repo-core-1027"),
    ("🌐", "repo-core-1028"),
    ("🚧", "repo-core-1029"),
    ("🏷️", "repo-core-1030"),
    ("🗑️", "repo-core-1031"),
    ("⏪️", "repo-core-1032"),
    ("🔀", "repo-core-1033"),
    ("📦️", "repo-core-1034"),
    ("🚀", "repo-core-1035"),
    ("🩹", "repo-core-1036"),
];

/// A Conventional Commits prefix, as read from a summary.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Conventional {
    pub kind: String,
    pub scope: String,
    pub breaking: bool,
}

impl Conventional {
    /// `type(scope)!: `, or nothing without a type.
    pub fn render(&self) -> String {
        let kind = self.kind.trim();
        if kind.is_empty() {
            return String::new();
        }
        let scope = clean_scope(&self.scope);
        let scope = if scope.is_empty() {
            String::new()
        } else {
            format!("({scope})")
        };
        let bang = if self.breaking { "!" } else { "" };
        format!("{kind}{scope}{bang}: ")
    }
}

/// A scope as it may stand between the parentheses: no parentheses, no line breaks.
fn clean_scope(scope: &str) -> String {
    scope
        .chars()
        .filter(|c| !matches!(c, '(' | ')' | '\n' | '\r'))
        .collect::<String>()
        .trim()
        .to_owned()
}

fn conventional_re() -> Option<&'static Regex> {
    static RE: OnceLock<Option<Regex>> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"^([A-Za-z][\w-]*)(?:\(([^()\r\n]*)\))?(!)?:(?: |$)").ok())
        .as_ref()
}

/// The Conventional Commits prefix `summary` starts with, and how many bytes it takes.
pub fn parse_conventional(summary: &str) -> Option<(Conventional, usize)> {
    let caps = conventional_re()?.captures(summary)?;
    let all = caps.get(0)?;
    Some((
        Conventional {
            kind: caps.get(1)?.as_str().to_owned(),
            scope: caps.get(2).map_or("", |m| m.as_str()).trim().to_owned(),
            breaking: caps.get(3).is_some(),
        },
        all.end(),
    ))
}

/// The Gitmoji `summary` starts with (from [`GITMOJI`], with or without its variation
/// selector), and how many bytes it takes with the spaces after it.
pub fn parse_gitmoji(summary: &str) -> Option<(&'static str, usize)> {
    GITMOJI.iter().find_map(|(emoji, _)| {
        let bare = emoji.trim_end_matches('\u{fe0f}');
        let rest = summary.strip_prefix(emoji).or_else(|| {
            summary
                .strip_prefix(bare)
                .map(|r| r.strip_prefix('\u{fe0f}').unwrap_or(r))
        })?;
        let trimmed = rest.trim_start_matches(' ');
        Some((*emoji, summary.len() - trimmed.len()))
    })
}

/// `summary` with `prefix` (rendered) in place of the Conventional Commits prefix it had.
pub fn with_conventional(summary: &str, prefix: &Conventional) -> String {
    let rest = parse_conventional(summary)
        .map(|(_, end)| end)
        .or_else(|| unfinished_prefix(summary))
        .map_or(summary, |end| &summary[end..]);
    format!("{}{}", prefix.render(), rest.trim_start())
}

/// How long a prefix still being typed is, when the summary is only that: `fix(g` or
/// `feat(ui)` (with a type that could be one, no colon yet).
fn unfinished_prefix(summary: &str) -> Option<usize> {
    let (kind, rest) = summary.split_once('(')?;
    let kind_ok = kind.chars().next().is_some_and(|c| c.is_ascii_alphabetic())
        && kind
            .chars()
            .all(|c| c.is_alphanumeric() || c == '-' || c == '_');
    let scope = rest.strip_suffix(')').unwrap_or(rest);
    let scope_ok = !scope.contains(['(', ')', ':', '\n', '\r']);
    (kind_ok && scope_ok).then_some(summary.len())
}

/// `summary` with `emoji` in place of the Gitmoji it started with.
pub fn with_gitmoji(summary: &str, emoji: &str) -> String {
    let rest = parse_gitmoji(summary).map_or(summary, |(_, end)| &summary[end..]);
    format!("{emoji} {}", rest.trim_start())
}

/// The types and scopes of `subjects` (earlier commits) that have a Conventional Commits
/// prefix, each once, the most used first: `(types, scopes)`.
pub fn used_prefixes(subjects: &[String]) -> (Vec<String>, Vec<String>) {
    let mut kinds: Vec<(String, usize)> = Vec::new();
    let mut scopes: Vec<(String, usize)> = Vec::new();
    let bump = |list: &mut Vec<(String, usize)>, word: &str| match list
        .iter_mut()
        .find(|(w, _)| w == word)
    {
        Some((_, n)) => *n += 1,
        None => list.push((word.to_owned(), 1)),
    };
    for subject in subjects {
        let Some((c, _)) = parse_conventional(subject) else {
            continue;
        };
        bump(&mut kinds, &c.kind);
        for scope in c.scope.split(',').map(str::trim).filter(|s| !s.is_empty()) {
            bump(&mut scopes, scope);
        }
    }
    let ranked = |mut list: Vec<(String, usize)>| {
        // Stable: the most recent first among those used as often.
        list.sort_by_key(|w| std::cmp::Reverse(w.1));
        list.into_iter().map(|(w, _)| w).collect::<Vec<_>>()
    };
    (ranked(kinds), ranked(scopes))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_conventional_prefixes() {
        fn read(s: &str) -> Option<(Conventional, &str)> {
            parse_conventional(s).map(|(c, end)| (c, &s[end..]))
        }
        let (c, rest) = read("feat(ui)!: dark mode").unwrap_or_default();
        assert_eq!(
            (c.kind.as_str(), c.scope.as_str(), c.breaking),
            ("feat", "ui", true)
        );
        assert_eq!(rest, "dark mode");
        let (c, rest) = read("fix: ünïcode käse").unwrap_or_default();
        assert_eq!((c.kind.as_str(), c.scope.as_str()), ("fix", ""));
        assert_eq!(rest, "ünïcode käse");
        assert!(read("Merge branch 'x'").is_none());
        assert!(read("").is_none());
        assert!(read("feat(a(b)): x").is_none());
        assert!(read(": x").is_none());
        // Not a link nor a word with no space after its colon.
        assert!(read("https://host is down").is_none());
        assert!(read("fix:x").is_none());
        assert!(read("fix:").is_some());
        let (c, _) = read("build(deps, ci): bump").unwrap_or_default();
        assert_eq!(c.scope, "deps, ci");
    }

    #[test]
    fn writes_a_prefix_in_place_of_the_old_one() {
        let feat = Conventional {
            kind: "feat".into(),
            scope: " app shell ".into(),
            breaking: false,
        };
        assert_eq!(
            with_conventional("fix(x)!: thing", &feat),
            "feat(app shell): thing"
        );
        assert_eq!(
            with_conventional("  thing", &feat),
            "feat(app shell): thing"
        );
        assert_eq!(with_conventional("", &feat), "feat(app shell): ");
        let bare = Conventional {
            kind: "docs".into(),
            scope: "(a)\nb".into(),
            breaking: true,
        };
        assert_eq!(with_conventional("x", &bare), "docs(ab)!: x");
        // A prefix still being typed is replaced whole.
        assert_eq!(with_conventional("fix(g", &feat), "feat(app shell): ");
        assert_eq!(with_conventional("fix(ui)", &feat), "feat(app shell): ");
        assert_eq!(
            with_conventional("fix(a) b", &feat),
            "feat(app shell): fix(a) b"
        );
        assert_eq!(
            with_conventional("https://x", &feat),
            "feat(app shell): https://x"
        );
        // No type: the prefix is taken out.
        assert_eq!(with_conventional("feat: x", &Conventional::default()), "x");
    }

    #[test]
    fn writes_a_gitmoji_in_place_of_the_old_one() {
        assert_eq!(with_gitmoji("🐛 fix it", "✨"), "✨ fix it");
        assert_eq!(with_gitmoji("fix it", "🐛"), "🐛 fix it");
        // With or without the variation selector.
        assert_eq!(with_gitmoji("♻️ tidy", "✨"), "✨ tidy");
        assert_eq!(with_gitmoji("♻ tidy", "✨"), "✨ tidy");
        assert_eq!(with_gitmoji("", "✨"), "✨ ");
        assert_eq!(parse_gitmoji("plain"), None);
    }

    #[test]
    fn counts_the_prefixes_used_before() {
        let subjects: Vec<String> = [
            "feat(ui): a",
            "fix(git): b",
            "feat(ui, git): c",
            "feat: d",
            "no prefix",
            "chore(release)!: e",
        ]
        .map(str::to_owned)
        .to_vec();
        let (kinds, scopes) = used_prefixes(&subjects);
        assert_eq!(kinds, ["feat", "fix", "chore"]);
        assert_eq!(scopes, ["ui", "git", "release"]);
        assert_eq!(used_prefixes(&[]), (vec![], vec![]));
    }
}
