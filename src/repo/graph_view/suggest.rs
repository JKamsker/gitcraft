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
    /// Whether the list was up last frame (items or a status line): Esc closes it then.
    pub(in crate::repo) open: bool,
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
    /// Bumped when a source changes, so a list worked out before is worked out again.
    pub(in crate::repo) version: u64,
    /// The last list, with the query, cursor and version it is for: it is worked out only
    /// when one of them changes, not every frame.
    pub(in crate::repo) cached: Option<(String, usize, u64, Listing)>,
    /// Bumped when the sources are let go of (the search closed, the repository reloaded):
    /// a read started before then is not taken in.
    pub(in crate::repo) generation: u64,
}

impl SuggestState {
    /// The list for the piece of `query` at `cursor`, from the cache when nothing changed.
    pub(in crate::repo) fn listing_for(&mut self, query: &str, cursor: usize) -> Listing {
        if let Some((q, c, v, listing)) = &self.cached
            && q == query
            && *c == cursor
            && *v == self.version
        {
            return listing.clone();
        }
        let listing = listing(query, cursor, self);
        self.cached = Some((query.to_owned(), cursor, self.version, listing.clone()));
        listing
    }

    /// Lets the sources go, to be read again when next needed (history or files may have
    /// changed); reads still running are not taken in.
    pub(in crate::repo) fn forget_sources(&mut self) {
        self.authors = Source::Idle;
        self.committers = Source::Idle;
        self.paths = Source::Idle;
        self.generation += 1;
        self.version += 1;
    }

    /// Everything back to how a new search starts, reads still running not taken in.
    pub(in crate::repo) fn reset(&mut self) {
        let generation = self.generation + 1;
        let version = self.version + 1;
        *self = SuggestState::default();
        self.generation = generation;
        self.version = version;
    }

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
#[derive(Clone, Debug, Default, PartialEq, Eq)]
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
        // merges: takes no value: a space follows it, as after a value.
        let operator = op != Operator::Merges;
        if name.starts_with(&lower) {
            out.push(Item {
                label: name.clone(),
                detail: op.help().to_owned(),
                insert: name.clone(),
                operator,
            });
        }
        if op.negatable() && format!("-{name}").starts_with(&lower) {
            out.push(Item {
                label: format!("-{name}"),
                detail: crate::i18n::text("repo-core-2729").to_owned(),
                insert: format!("-{name}"),
                operator,
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
            out.status = Some(crate::i18n::message(
                "repo-core-2730",
                &[("what", what.to_string().into())],
            ));
            None
        }
        Source::Loading => {
            out.status = Some(crate::i18n::message(
                "repo-core-2730",
                &[("what", what.to_string().into())],
            ));
            None
        }
        Source::Failed(e) => {
            out.status = Some(crate::i18n::message(
                "repo-core-2731",
                &[
                    ("what", what.to_string().into()),
                    ("e", e.to_string().into()),
                ],
            ));
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
        crate::i18n::text("repo-core-2732")
    } else {
        crate::i18n::text("repo-core-2733")
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
            insert: format!("{name}:{}", quote_value(&person_key(p, people))),
            label: if p.name.is_empty() {
                p.email.clone()
            } else {
                p.name.clone()
            },
            detail: crate::i18n::message(
                "repo-core-2735",
                &[
                    ("email", p.email.to_string().into()),
                    ("commits", (p.commits).into()),
                ],
            ),
            operator: false,
        })
        .collect();
    if out.items.is_empty() {
        out.status = Some(crate::i18n::message(
            "repo-core-2736",
            &[("what", what.to_string().into())],
        ));
    }
}

/// What finds `p` alone, as git matches `--author` (any part of `Name <email>`, any case of
/// A to Z): the name, unless it is empty or in someone else's name or email too (Ann and
/// Anna); then the email in its brackets.
fn person_key(p: &Person, people: &[Person]) -> String {
    let name = p.name.to_lowercase();
    let ambiguous = name.is_empty()
        || people.iter().any(|other| {
            !std::ptr::eq(other, p)
                && (other.name.to_lowercase().contains(&name)
                    || other.email.to_lowercase().contains(&name))
        });
    if ambiguous {
        format!("<{}>", p.email)
    } else {
        p.name.clone()
    }
}

/// `path` as a pathspec that means only it: git would read `*`, `?`, `[` and a leading `:`.
fn literal_path(path: &str) -> String {
    if path.starts_with(':') || path.contains(['*', '?', '[', '\\']) {
        format!(":(literal){path}")
    } else {
        path.to_owned()
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
            insert: format!("path:{}", quote_value(&literal_path(path))),
            operator: false,
        })
        .collect();
    if out.items.is_empty() {
        out.status = Some(crate::i18n::text("repo-core-2740").to_owned());
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
#[path = "suggest_tests.rs"]
mod tests;
