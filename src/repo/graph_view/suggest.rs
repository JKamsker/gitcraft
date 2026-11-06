//! What the graph's search field suggests while an operator is typed: the operators
//! themselves, then authors, paths or dates for the one at the cursor. Pure: the field
//! (`search.rs`) draws them and the tab loads their sources in the background.

use std::ops::Range;
use std::sync::Arc;

use crate::git::search_query::{Operator, Person, date_suggestions, quote_value, token_at};

/// The most suggestions listed at once.
pub(in crate::repo) const MAX_SHOWN: usize = 8;
/// The most paths kept for suggestions: a huge tree is cut short, not read whole each time.
const MAX_PATHS: usize = 200_000;

/// Something suggestions are drawn from, read once the search needs it.
#[derive(Default)]
pub(in crate::repo) enum Source<T> {
    #[default]
    Idle,
    Loading,
    Ready(Arc<T>),
    Failed(String),
}

/// Which source an operator needs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::repo) enum SourceKind {
    Authors,
    Committers,
    Paths,
}

/// The files and folders of the checked-out commit, with their lower-case form to match on.
pub(in crate::repo) struct PathIndex {
    entries: Vec<(String, String)>,
}

impl PathIndex {
    /// Every file in `files` and every folder above one (`src/`), sorted.
    pub(in crate::repo) fn new(files: Vec<String>) -> PathIndex {
        let mut all: Vec<String> = Vec::new();
        let mut folders = std::collections::BTreeSet::new();
        for file in files.into_iter().take(MAX_PATHS) {
            for (i, _) in file.match_indices('/') {
                folders.insert(file[..=i].to_owned());
            }
            all.push(file);
        }
        all.extend(folders);
        all.sort();
        PathIndex {
            entries: all
                .into_iter()
                .map(|p| {
                    let lower = p.to_lowercase();
                    (p, lower)
                })
                .collect(),
        }
    }
}

/// The suggestions' state for one tab's search.
#[derive(Default)]
pub(in crate::repo) struct SuggestState {
    /// The highlighted suggestion, once the arrows moved to one.
    pub(in crate::repo) chosen: Option<usize>,
    /// How many were listed last frame: the keys are the list's while it has any.
    pub(in crate::repo) shown: usize,
    /// What the list was for (the piece at the cursor), so that a new one starts unhighlighted.
    pub(in crate::repo) listed_for: String,
    /// The query the list was closed for (Esc): it stays closed until the query changes.
    pub(in crate::repo) dismissed: Option<String>,
    /// Where the list was drawn last frame: a click on it takes the focus off the field
    /// before the click is over, and the list must stay until it is.
    pub(in crate::repo) popup: Option<egui::Rect>,
    pub(in crate::repo) authors: Source<Vec<Person>>,
    pub(in crate::repo) committers: Source<Vec<Person>>,
    pub(in crate::repo) paths: Source<PathIndex>,
}

impl SuggestState {
    pub(in crate::repo) fn source_idle(&self, kind: SourceKind) -> bool {
        match kind {
            SourceKind::Authors => matches!(self.authors, Source::Idle),
            SourceKind::Committers => matches!(self.committers, Source::Idle),
            SourceKind::Paths => matches!(self.paths, Source::Idle),
        }
    }
}

/// One suggestion: what it shows, and what replaces the piece at the cursor.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::repo) struct Item {
    pub(in crate::repo) label: String,
    pub(in crate::repo) detail: String,
    pub(in crate::repo) insert: String,
    /// An operator's name: its value comes next, so no space after it.
    pub(in crate::repo) operator: bool,
}

/// What to list for the piece at the cursor.
#[derive(Debug, Default, PartialEq, Eq)]
pub(in crate::repo) struct Listing {
    pub(in crate::repo) items: Vec<Item>,
    /// The piece the items replace, in bytes.
    pub(in crate::repo) range: Range<usize>,
    /// Shown instead of items: still loading, or why they couldn't be read.
    pub(in crate::repo) status: Option<String>,
    /// A source to read for the list.
    pub(in crate::repo) load: Option<SourceKind>,
    /// What the list is for, to tell one list from the next.
    pub(in crate::repo) key: String,
}

/// The suggestions for the piece of `query` at `cursor` (in characters), if any.
pub(in crate::repo) fn listing(query: &str, cursor: usize, state: &SuggestState) -> Listing {
    let Some(token) = token_at(query, cursor) else {
        return Listing::default();
    };
    let key = query[token.span.clone()].to_owned();
    let mut out = Listing {
        range: token.span.clone(),
        key,
        ..Listing::default()
    };
    match token.op {
        None => out.items = operators(&token.value),
        Some(_) if token.negated => {}
        op => fill(&mut out, op, &token.value, state),
    }
    // Nothing to offer when the value already is the one suggestion.
    if let [only] = out.items.as_slice()
        && only.insert == query[token.span.clone()]
    {
        out.items.clear();
    }
    out
}

fn fill(out: &mut Listing, op: Option<Operator>, value: &str, state: &SuggestState) {
    match op {
        Some(Operator::Author) => people(out, "author", value, &state.authors, SourceKind::Authors),
        Some(Operator::Committer) => people(
            out,
            "committer",
            value,
            &state.committers,
            SourceKind::Committers,
        ),
        Some(Operator::Path) => paths(out, value, &state.paths),
        Some(op @ (Operator::After | Operator::Before)) => out.items = dates(op, value),
        _ => {}
    }
}

/// The operators whose name starts with `word` (two letters at least: one would match
/// half of every search).
fn operators(word: &str) -> Vec<Item> {
    let lower = word.to_lowercase();
    if lower.chars().count() < 2 {
        return Vec::new();
    }
    let mut out = Vec::new();
    for op in Operator::ALL {
        let name = format!("{}:", op.name());
        if name.starts_with(&lower) {
            out.push(Item {
                label: name.clone(),
                detail: op.help().to_owned(),
                insert: name.clone(),
                operator: true,
            });
        }
        if op.negatable() && format!("-{name}").starts_with(&lower) {
            out.push(Item {
                label: format!("-{name}"),
                detail: "leave merge commits out".to_owned(),
                insert: format!("-{name}"),
                operator: true,
            });
        }
    }
    out
}

/// Waits for (or starts reading) `source`; `None` until it is there.
fn ready<'a, T>(
    out: &mut Listing,
    source: &'a Source<T>,
    kind: SourceKind,
    what: &str,
) -> Option<&'a T> {
    match source {
        Source::Ready(items) => Some(items),
        Source::Idle => {
            out.load = Some(kind);
            out.status = Some(format!("Reading {what}…"));
            None
        }
        Source::Loading => {
            out.status = Some(format!("Reading {what}…"));
            None
        }
        Source::Failed(e) => {
            out.status = Some(format!("Couldn't read the {what}: {e}"));
            None
        }
    }
}

fn people(
    out: &mut Listing,
    name: &str,
    value: &str,
    source: &Source<Vec<Person>>,
    kind: SourceKind,
) {
    let what = if kind == SourceKind::Authors {
        "authors"
    } else {
        "committers"
    };
    let Some(people) = ready(out, source, kind, what) else {
        return;
    };
    let needle = value.to_lowercase();
    let matches = |p: &&Person, prefix: bool| {
        let (n, e) = (p.name.to_lowercase(), p.email.to_lowercase());
        if prefix {
            n.starts_with(&needle) || e.starts_with(&needle)
        } else {
            n.contains(&needle) || e.contains(&needle)
        }
    };
    let first: Vec<&Person> = people
        .iter()
        .filter(|p| matches(p, true))
        .take(MAX_SHOWN)
        .collect();
    let rest = people
        .iter()
        .filter(|p| !matches(p, true) && matches(p, false));
    out.items = first
        .into_iter()
        .chain(rest)
        .take(MAX_SHOWN)
        .map(|p| Item {
            label: p.name.clone(),
            detail: format!(
                "{} · {} commit{}",
                p.email,
                p.commits,
                if p.commits == 1 { "" } else { "s" }
            ),
            insert: format!("{name}:{}", quote_value(&p.name)),
            operator: false,
        })
        .collect();
    if out.items.is_empty() {
        out.status = Some(format!("No {what} match"));
    }
}

fn paths(out: &mut Listing, value: &str, source: &Source<PathIndex>) {
    let Some(index) = ready(out, source, SourceKind::Paths, "files") else {
        return;
    };
    let needle = value.to_lowercase();
    // Paths that start with it, then those with a file or folder name that does, then the rest.
    let mut ranked: [Vec<&str>; 3] = [Vec::new(), Vec::new(), Vec::new()];
    for (path, lower) in &index.entries {
        let rank = if lower.starts_with(&needle) {
            0
        } else if lower
            .trim_end_matches('/')
            .rsplit('/')
            .next()
            .is_some_and(|name| name.starts_with(&needle))
        {
            1
        } else if lower.contains(&needle) {
            2
        } else {
            continue;
        };
        if ranked[rank].len() < MAX_SHOWN {
            ranked[rank].push(path);
        }
        if ranked[0].len() >= MAX_SHOWN {
            break;
        }
    }
    out.items = ranked
        .into_iter()
        .flatten()
        .take(MAX_SHOWN)
        .map(|path| Item {
            label: path.to_owned(),
            detail: if path.ends_with('/') {
                "folder"
            } else {
                "file"
            }
            .to_owned(),
            insert: format!("path:{}", quote_value(path)),
            operator: false,
        })
        .collect();
    if out.items.is_empty() {
        out.status = Some("No file or folder matches".to_owned());
    }
}

fn dates(op: Operator, value: &str) -> Vec<Item> {
    let needle = value.to_lowercase();
    date_suggestions()
        .into_iter()
        .filter(|d| d.value.starts_with(&needle))
        .take(MAX_SHOWN)
        .map(|d| Item {
            insert: format!("{}:{}", op.name(), d.value),
            label: d.value,
            detail: d.detail,
            operator: false,
        })
        .collect()
}

/// `query` with the piece at `range` (bytes) replaced by `item`, and where the cursor goes
/// (in characters): after it, past a space unless the item is an operator's name.
pub(in crate::repo) fn accept(query: &str, range: &Range<usize>, item: &Item) -> (String, usize) {
    let before = &query[..range.start];
    let after = &query[range.end..];
    let space = if item.operator || after.starts_with(char::is_whitespace) {
        ""
    } else {
        " "
    };
    let head = format!("{before}{}{space}", item.insert);
    let mut cursor = head.chars().count();
    if !item.operator && after.starts_with(char::is_whitespace) {
        cursor += 1;
    }
    (format!("{head}{after}"), cursor)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn person(name: &str, email: &str, commits: usize) -> Person {
        Person {
            name: name.to_owned(),
            email: email.to_owned(),
            commits,
        }
    }

    fn state() -> SuggestState {
        SuggestState {
            authors: Source::Ready(Arc::new(vec![
                person("Alice Smith", "alice@example.com", 30),
                person("Zoë Ünal", "zoe@example.com", 4),
                person("Bob", "bob@alice.dev", 2),
            ])),
            paths: Source::Ready(Arc::new(PathIndex::new(vec![
                "README.md".to_owned(),
                "src/main.rs".to_owned(),
                "src/git/mod.rs".to_owned(),
                "docs/Read me.md".to_owned(),
            ]))),
            ..SuggestState::default()
        }
    }

    fn labels(l: &Listing) -> Vec<&str> {
        l.items.iter().map(|i| i.label.as_str()).collect()
    }

    #[test]
    fn a_word_suggests_the_operators_it_starts() {
        let s = state();
        assert_eq!(labels(&listing("au", 2, &s)), ["author:"]);
        assert_eq!(labels(&listing("fix me", 6, &s)), ["message:", "merges:"]);
        assert_eq!(labels(&listing("me", 2, &s)), ["message:", "merges:"]);
        assert_eq!(labels(&listing("-me", 3, &s)), ["-merges:"]);
        assert_eq!(labels(&listing("COMM", 4, &s)), ["committer:"]);
        // One letter is most searches: nothing.
        assert!(listing("a", 1, &s).items.is_empty());
        // In the whitespace between pieces: nothing.
        assert!(listing("au  x", 3, &s).items.is_empty());
    }

    #[test]
    fn authors_are_suggested_by_name_or_email() {
        let s = state();
        let l = listing("fix author:", 11, &s);
        assert_eq!(labels(&l), ["Alice Smith", "Zoë Ünal", "Bob"]);
        assert_eq!(l.range, 4..11);
        // Starting with it comes first, then containing it (Bob's email has “alice”).
        assert_eq!(
            labels(&listing("author:ali", 10, &s)),
            ["Alice Smith", "Bob"]
        );
        assert_eq!(labels(&listing("author:ZOË", 10, &s)), ["Zoë Ünal"]);
        let l = listing("author:zo", 9, &s);
        assert_eq!(l.items[0].insert, "author:\"Zoë Ünal\"");
        assert_eq!(l.items[0].detail, "zoe@example.com · 4 commits");
        let l = listing("author:\"Zoë", 11, &s);
        assert_eq!(labels(&l), ["Zoë Ünal"]);
        let l = listing("author:nobody", 13, &s);
        assert_eq!(l.status.as_deref(), Some("No authors match"));
    }

    #[test]
    fn sources_are_read_when_first_needed() {
        let s = SuggestState::default();
        let l = listing("committer:", 10, &s);
        assert_eq!(l.load, Some(SourceKind::Committers));
        assert_eq!(l.status.as_deref(), Some("Reading committers…"));
        let s = SuggestState {
            paths: Source::Failed("not a repository".to_owned()),
            ..SuggestState::default()
        };
        let l = listing("path:x", 6, &s);
        assert_eq!(l.load, None);
        assert_eq!(
            l.status.as_deref(),
            Some("Couldn't read the files: not a repository")
        );
    }

    #[test]
    fn paths_and_their_folders_are_suggested() {
        let s = state();
        assert_eq!(
            labels(&listing("path:src", 8, &s)),
            ["src/", "src/git/", "src/git/mod.rs", "src/main.rs"]
        );
        // By file name too, after the paths that start with it.
        assert_eq!(labels(&listing("path:main", 9, &s)), ["src/main.rs"]);
        assert_eq!(
            labels(&listing("path:read", 9, &s)),
            ["README.md", "docs/Read me.md"]
        );
        let l = listing("path:docs/r", 11, &s);
        assert_eq!(l.items[0].insert, "path:\"docs/Read me.md\"");
    }

    #[test]
    fn dates_are_suggested() {
        let s = state();
        let l = listing("after:", 6, &s);
        assert_eq!(l.items.len(), MAX_SHOWN);
        assert_eq!(l.items[0].insert, "after:today");
        assert_eq!(labels(&listing("before:y", 8, &s)), ["yesterday"]);
        assert_eq!(labels(&listing("before:1", 8, &s)), ["1w", "1m", "1y"]);
        // Nothing for operators whose values can't be listed.
        assert!(listing("message:x", 9, &s).items.is_empty());
        assert!(listing("-merges:", 8, &s).items.is_empty());
    }

    #[test]
    fn the_only_suggestion_already_typed_is_not_offered() {
        let s = state();
        assert!(listing("after:yesterday", 15, &s).items.is_empty());
        assert!(listing("author:Bob", 10, &s).items.is_empty());
    }

    #[test]
    fn accepting_replaces_the_piece_at_the_cursor() {
        let item = |insert: &str, operator| Item {
            label: String::new(),
            detail: String::new(),
            insert: insert.to_owned(),
            operator,
        };
        assert_eq!(
            accept("fix au", &(4..6), &item("author:", true)),
            ("fix author:".to_owned(), 11)
        );
        assert_eq!(
            accept(
                "author:zo fix",
                &(0..9),
                &item("author:\"Zoë Ünal\"", false)
            ),
            ("author:\"Zoë Ünal\" fix".to_owned(), 18)
        );
        assert_eq!(
            accept("é path:sr", &(3..10), &item("path:src/", false)),
            ("é path:src/ ".to_owned(), 12)
        );
    }

    #[test]
    fn folders_are_listed_once() {
        let index = PathIndex::new(vec!["a/b/c.txt".to_owned(), "a/b/d.txt".to_owned()]);
        let paths: Vec<&str> = index.entries.iter().map(|(p, _)| p.as_str()).collect();
        assert_eq!(paths, ["a/", "a/b/", "a/b/c.txt", "a/b/d.txt"]);
    }
}
