//! The change impact graph's analysis (#175 item 1): the Rust items a change touched, read
//! from the files at the change's base and head revisions, and what refers to them by name at
//! those revisions. The items in the diff are observed; every reference is inferred (a name
//! match, which may be another item of the same name), and files in a language with no
//! analyzer here are said to be unknown, not left out.
//!
//! Everything is read at the revisions compared, through `git show`-like reads and
//! `git grep <rev>`: never from the working tree.

pub mod scan;
#[cfg(test)]
mod tests;

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::Path;

use self::scan::{Item, ItemKind};
use super::Provenance;
use crate::git::{self, changed_lines::Hunk};

/// At most so many changed symbols are followed.
pub const MAX_SYMBOLS: usize = 60;
/// At most so many references are kept of each symbol.
pub const MAX_REFS: usize = 40;
/// At most so many changed Rust files are read.
pub const MAX_CHANGED_FILES: usize = 150;
/// At most so many files with references are read to place them (in a test, in which item).
pub const MAX_FILES: usize = 300;
/// At most so many files in other languages are listed.
pub const MAX_OTHER: usize = 40;
/// Bigger files are not read.
const MAX_TEXT: usize = 4 << 20;

/// What a change is compared between: its base and head commits.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Range {
    /// What it is, in words: `commit 1a2b3c4d against its parent`.
    pub label: String,
    pub base: String,
    pub head: String,
    /// The branch or pull request the head was read from, and the commit it was at: once it
    /// points elsewhere, what is shown is stale.
    pub tip: Option<(String, String)>,
}

/// What the graph is asked to be about.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Ask {
    /// A commit against its first parent.
    Commit(String),
    /// The checked-out branch against where it left `base` (the default branch when `None`).
    Branch { base: Option<String> },
    /// A pull request: its head against where it left its base branch.
    Pr {
        number: u64,
        head: String,
        base: String,
    },
}

/// Works out the commits `ask` compares.
pub fn resolve(repo: &Path, ask: &Ask) -> Result<Range, String> {
    match ask {
        Ask::Commit(rev) => {
            let head = git::resolve(repo, rev)?;
            let base = match git::resolve(repo, &format!("{head}^1")) {
                Ok(parent) => parent,
                Err(_) => empty_tree(repo)?,
            };
            Ok(Range {
                label: format!("commit {} against its parent", &head[..head.len().min(8)]),
                base,
                head,
                tip: None,
            })
        }
        Ask::Branch { base } => {
            let head = git::resolve(repo, "HEAD")?;
            let branch = git::run_ok_codes(
                Some(repo),
                &["symbolic-ref", "--short", "-q", "HEAD"],
                &[0, 1],
            )?
            .trim()
            .to_owned();
            let candidates: Vec<String> = match base {
                Some(b) => vec![b.clone(), format!("origin/{b}")],
                None => [
                    "origin/HEAD",
                    "main",
                    "master",
                    "origin/main",
                    "origin/master",
                ]
                .map(str::to_owned)
                .to_vec(),
            };
            let (base_name, _, base) = super::fork_point(repo, &head, &candidates)
                .ok_or("no default branch to compare with (main, master or origin/HEAD)")?;
            let name = if branch.is_empty() {
                "HEAD".to_owned()
            } else {
                branch
            };
            Ok(Range {
                label: format!("{name} since it left {base_name}"),
                base,
                head: head.clone(),
                tip: Some((name, head)),
            })
        }
        Ask::Pr { number, head, base } => {
            let head = git::resolve(repo, head).map_err(|_| {
                format!(
                    "the head of #{number} isn't in this repository: fetch it to read its \
                     impact (nothing is fetched on its own)"
                )
            })?;
            let (_, _, merge_base) =
                super::fork_point(repo, &head, &[base.clone(), format!("origin/{base}")])
                    .ok_or_else(|| format!("its base branch {base} isn't here"))?;
            Ok(Range {
                label: format!("#{number} since it left {base}"),
                base: merge_base,
                head: head.clone(),
                tip: Some((format!("#{number}"), head)),
            })
        }
    }
}

/// The empty tree: what a root commit is compared with.
fn empty_tree(repo: &Path) -> Result<String, String> {
    git::run_input(Some(repo), &["hash-object", "-t", "tree", "--stdin"], "")
        .map(|s| s.trim().to_owned())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Change {
    Added,
    Removed,
    Modified,
}

impl Change {
    pub fn word(self) -> &'static str {
        match self {
            Change::Added => "added",
            Change::Removed => "removed",
            Change::Modified => "modified",
        }
    }
}

/// Where something is, at a revision.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Span {
    pub rev: String,
    pub path: String,
    /// The line its name is on.
    pub line: u32,
    pub start: u32,
    pub end: u32,
}

impl Span {
    fn contains(&self, rev: &str, path: &str, line: u32) -> bool {
        self.rev == rev && self.path == path && (self.start..=self.end).contains(&line)
    }
}

/// An item the change touched.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Symbol {
    pub kind: ItemKind,
    pub name: String,
    /// Its name within its file's items (`Repo::new`).
    pub path: String,
    pub change: Change,
    pub public: bool,
    pub test: bool,
    /// Where it is at the head (`None`: removed) and at the base (`None`: added).
    pub head: Option<Span>,
    pub base: Option<Span>,
    /// Lines the change added or removed in it.
    pub lines: u32,
    /// It has more references than are kept.
    pub refs_cut: bool,
    /// Other definitions found with its name: references may be to them.
    pub same_named: u32,
}

impl Symbol {
    /// Where it is: at the head, else (removed) at the base.
    pub fn at(&self) -> Option<&Span> {
        self.head.as_ref().or(self.base.as_ref())
    }
}

/// Code that refers to changed symbols: an item (or, where no item encloses it, a file).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Site {
    pub rev: String,
    pub path: String,
    /// The item's name within its file, or the file's path.
    pub label: String,
    /// The item's line, or the first reference's.
    pub line: u32,
    pub test: bool,
    /// Its file is one the change changed.
    pub in_diff: bool,
    /// It is itself a changed symbol.
    pub changed: bool,
    /// Its file was read: where it is (test or not, which item) is known, not guessed from its
    /// path.
    pub read: bool,
}

/// A line that names a changed symbol: the evidence of an edge.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Reference {
    pub symbol: usize,
    pub site: usize,
    pub rev: String,
    pub path: String,
    pub line: u32,
    pub text: String,
}

/// A changed file no analyzer here reads.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OtherFile {
    pub path: String,
    pub status: char,
    /// `no analyzer for .ts files`.
    pub why: String,
}

/// What a change touched and what refers to it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Impact {
    pub range: Range,
    pub symbols: Vec<Symbol>,
    pub sites: Vec<Site>,
    pub refs: Vec<Reference>,
    pub other: Vec<OtherFile>,
    /// Changed Rust files and how many changed lines in them are in no item (`use` lines,
    /// blank lines between items).
    pub rust_files: Vec<(String, u32)>,
    /// Name matches dropped as being in a comment or a string.
    pub in_comments: u32,
    /// Why it is incomplete, each limit reached.
    pub partial: Vec<String>,
    pub collected: i64,
}

impl Impact {
    pub fn provenance(&self) -> Provenance {
        let mut p = Provenance::new(
            "git diff -U0 · Rust items · git grep at the revisions",
            self.range.label.clone(),
        )
        .tip("head", self.range.head.clone())
        .tip("base", self.range.base.clone());
        p.collected = self.collected;
        if !self.partial.is_empty() {
            p = p.partial(self.partial.join("; "));
        }
        p
    }
}

/// An item of a changed file the change touched: at the head, at the base, or both.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Touched {
    pub head: Option<usize>,
    pub base: Option<usize>,
    pub lines: u32,
}

impl Touched {
    pub fn change(&self) -> Change {
        match (self.head, self.base) {
            (Some(_), Some(_)) => Change::Modified,
            (Some(_), None) => Change::Added,
            _ => Change::Removed,
        }
    }
}

/// The items `hunks` touch: each added line in the innermost item around it at the head, each
/// removed one in the innermost at the base; an item of one side is paired with the item of
/// the same kind and name in the other. With how many changed lines are in no item.
pub fn touched(hunks: &[Hunk], head: &[Item], base: &[Item]) -> (Vec<Touched>, u32) {
    let mut head_hits: BTreeMap<usize, u32> = BTreeMap::new();
    let mut base_hits: BTreeMap<usize, u32> = BTreeMap::new();
    let mut outside = 0;
    for hunk in hunks {
        let sides = [
            (hunk.new_lines(), head, &mut head_hits),
            (hunk.old_lines(), base, &mut base_hits),
        ];
        for (lines, items, hits) in sides {
            let Some((first, last)) = lines else {
                continue;
            };
            for line in first..=last {
                match scan::innermost(items, line) {
                    Some(i) => *hits.entry(i).or_default() += 1,
                    None => outside += 1,
                }
            }
        }
    }
    let same = |a: &Item, b: &Item| a.kind == b.kind && a.path == b.path;
    let mut out: Vec<Touched> = Vec::new();
    for (&h, &n) in &head_hits {
        let counterpart = base
            .iter()
            .enumerate()
            .filter(|(_, b)| same(&head[h], b))
            .min_by_key(|(i, _)| !base_hits.contains_key(i))
            .map(|(i, _)| i);
        let lines = n + counterpart.and_then(|b| base_hits.remove(&b)).unwrap_or(0);
        out.push(Touched {
            head: Some(h),
            base: counterpart,
            lines,
        });
    }
    for (b, n) in base_hits {
        let counterpart = head.iter().position(|h| same(h, &base[b]));
        if let Some(t) = out
            .iter_mut()
            .find(|t| t.head.is_some() && t.head == counterpart)
        {
            t.lines += n;
            continue;
        }
        out.push(Touched {
            head: counterpart,
            base: Some(b),
            lines: n,
        });
    }
    (out, outside)
}

/// Whether a file's path says it holds tests: under a `tests` folder, or named `tests.rs` or
/// `*_tests.rs`.
pub fn test_path(path: &str) -> bool {
    let name = path.rsplit('/').next().unwrap_or(path);
    path.starts_with("tests/")
        || path.contains("/tests/")
        || name == "tests.rs"
        || name.ends_with("_tests.rs")
        || name.ends_with("_test.rs")
}

/// Whether `word` is in `line` as a whole word.
pub fn has_word(line: &str, word: &str) -> bool {
    let ident = |c: char| c == '_' || c.is_alphanumeric();
    line.match_indices(word).any(|(i, _)| {
        let before = line[..i].chars().next_back();
        let after = line[i + word.len()..].chars().next();
        !before.is_some_and(ident) && !after.is_some_and(ident)
    })
}

/// `text` cut to `max` characters, with an ellipsis.
fn cut_to(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_owned();
    }
    let mut out: String = text.chars().take(max - 1).collect();
    out.push('…');
    out
}

/// One line `git grep` found.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Hit {
    pub path: String,
    pub line: u32,
    pub text: String,
}

/// What `git grep -n -z <rev>` printed: `rev:path\0line\0text` lines.
pub fn parse_grep(out: &str, rev: &str) -> Vec<Hit> {
    let prefix = format!("{rev}:");
    out.lines()
        .filter_map(|l| {
            let mut parts = l.splitn(3, '\0');
            let path = parts.next()?;
            let path = path.strip_prefix(&prefix).unwrap_or(path).to_owned();
            let line = parts.next()?.parse().ok()?;
            let text = parts.next().unwrap_or("").to_owned();
            Some(Hit { path, line, text })
        })
        .collect()
}

/// The lines of the Rust files at `rev` with `name` as a whole word.
pub fn grep(repo: &Path, rev: &str, name: &str) -> Result<Vec<Hit>, String> {
    if name.starts_with('-') || rev.starts_with('-') {
        return Err("not a name".to_owned());
    }
    let out = git::run_ok_codes(
        Some(repo),
        &[
            "-c",
            "core.quotePath=false",
            "grep",
            "--no-color",
            "-n",
            "-z",
            "-I",
            "-w",
            "-F",
            // No more lines of a file than a symbol keeps references.
            &format!("--max-count={}", MAX_REFS + 1),
            "-e",
            name,
            rev,
            "--",
            "*.rs",
        ],
        &[0, 1],
    )?;
    Ok(parse_grep(&out, rev))
}

/// A file read at a revision: its items, and its lines with comments and strings blanked.
struct Read {
    items: Vec<Item>,
    masked: Vec<String>,
}

impl Read {
    fn new(text: &str) -> Self {
        Self {
            items: scan::scan(text),
            masked: scan::mask(text).lines().map(str::to_owned).collect(),
        }
    }
}

/// Reads files at revisions in one go, those not there (or too big) left out.
fn read_files(
    repo: &Path,
    wanted: &[(String, String)],
) -> Result<HashMap<(String, String), Read>, String> {
    let texts = git::blobs::read(repo, wanted, MAX_TEXT)?;
    Ok(wanted
        .iter()
        .cloned()
        .zip(texts)
        .filter_map(|(key, text)| Some((key, Read::new(&text?))))
        .collect())
}

/// What the change `range` touched in its Rust files and what refers to it, and its other
/// files as unknown.
pub fn analyze(repo: &Path, range: &Range) -> Result<Impact, String> {
    let (base, head) = (range.base.as_str(), range.head.as_str());
    let changed = git::diff_files(repo, base, Some(head))?;
    let in_diff: HashSet<String> = changed
        .iter()
        .flat_map(|f| std::iter::once(f.path.clone()).chain(f.old_path.clone()))
        .collect();
    let mut partial = Vec::new();
    let mut other = Vec::new();
    for file in changed.iter().filter(|f| !f.path.ends_with(".rs")) {
        if other.len() == MAX_OTHER {
            partial.push(format!(
                "only the first {MAX_OTHER} files in other languages are listed"
            ));
            break;
        }
        let name = file.path.rsplit('/').next().unwrap_or(&file.path);
        let why = match name.rsplit_once('.') {
            Some((stem, ext)) if !stem.is_empty() => format!("no analyzer for .{ext} files"),
            _ => "no analyzer for this file".to_owned(),
        };
        other.push(OtherFile {
            path: file.path.clone(),
            status: file.status,
            why,
        });
    }

    let mut files = git::changed_lines::read(repo, base, Some(head), &["*.rs"])?;
    files.retain(|f| !f.binary && (f.path.ends_with(".rs") || f.deleted));
    if files.len() > MAX_CHANGED_FILES {
        partial.push(format!(
            "only {MAX_CHANGED_FILES} of {} changed Rust files are read",
            files.len()
        ));
        files.truncate(MAX_CHANGED_FILES);
    }
    let mut wanted = Vec::new();
    for f in &files {
        if !f.deleted {
            wanted.push((head.to_owned(), f.path.clone()));
        }
        if !f.added {
            let old = f.old_path.clone().unwrap_or_else(|| f.path.clone());
            wanted.push((base.to_owned(), old));
        }
    }
    let mut read = read_files(repo, &wanted)?;
    // Those left unread (too big) have changes in no item known: said, not hidden.
    let unread = wanted.iter().filter(|w| !read.contains_key(*w)).count();
    if unread > 0 {
        partial.push(format!(
            "{unread} versions of changed Rust files weren't read (bigger than {} MB): their \
             changes aren't placed in items",
            MAX_TEXT >> 20
        ));
    }

    let mut symbols = Vec::new();
    let mut rust_files = Vec::new();
    let none = Read {
        items: Vec::new(),
        masked: Vec::new(),
    };
    for f in &files {
        let old = f.old_path.clone().unwrap_or_else(|| f.path.clone());
        let head_read = read
            .get(&(head.to_owned(), f.path.clone()))
            .unwrap_or(&none);
        let base_read = read.get(&(base.to_owned(), old.clone())).unwrap_or(&none);
        let (touched, outside) = touched(&f.hunks, &head_read.items, &base_read.items);
        rust_files.push((f.path.clone(), outside));
        for t in touched {
            let span = |rev: &str, path: &str, item: &Item| Span {
                rev: rev.to_owned(),
                path: path.to_owned(),
                line: item.line,
                start: item.start,
                end: item.end,
            };
            let h = t.head.map(|i| &head_read.items[i]);
            let b = t.base.map(|i| &base_read.items[i]);
            let Some(item) = h.or(b) else {
                continue;
            };
            symbols.push(Symbol {
                kind: item.kind,
                name: item.name.clone(),
                path: item.path.clone(),
                change: t.change(),
                public: item.public,
                test: item.test || test_path(&f.path),
                head: h.map(|i| span(head, &f.path, i)),
                base: b.map(|i| span(base, &old, i)),
                lines: t.lines,
                refs_cut: false,
                same_named: 0,
            });
        }
    }
    if symbols.len() > MAX_SYMBOLS {
        partial.push(format!(
            "only {MAX_SYMBOLS} of {} changed symbols are followed",
            symbols.len()
        ));
        symbols.truncate(MAX_SYMBOLS);
    }

    // The searches: at the head for each, at the base too for what the change removed.
    let mut found: HashMap<(String, String), Vec<Hit>> = HashMap::new();
    for s in &symbols {
        let mut revs = vec![head];
        if s.change == Change::Removed {
            revs.push(base);
        }
        for rev in revs {
            let key = (rev.to_owned(), s.name.clone());
            if let std::collections::hash_map::Entry::Vacant(e) = found.entry(key) {
                e.insert(grep(repo, rev, &s.name)?);
            }
        }
    }
    let mut to_read: Vec<(String, String)> = found
        .iter()
        .flat_map(|((rev, _), hits)| hits.iter().map(move |h| (rev.clone(), h.path.clone())))
        .filter(|key| !read.contains_key(key))
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect();
    if to_read.len() > MAX_FILES {
        partial.push(format!(
            "only {MAX_FILES} of {} files with references are read; the rest are placed by \
             their path alone",
            to_read.len()
        ));
        to_read.truncate(MAX_FILES);
    }
    read.extend(read_files(repo, &to_read)?);

    let mut sites: Vec<Site> = Vec::new();
    let mut site_index: HashMap<(String, String, String, u32), usize> = HashMap::new();
    let mut refs = Vec::new();
    let mut in_comments = 0;
    let mut cut = 0;
    for (si, s) in symbols.iter_mut().enumerate() {
        let mut revs = vec![head];
        if s.change == Change::Removed {
            revs.push(base);
        }
        let mut kept = 0;
        for rev in revs {
            let Some(hits) = found.get(&(rev.to_owned(), s.name.clone())) else {
                continue;
            };
            for hit in hits {
                let own = |span: &Option<Span>| {
                    span.as_ref()
                        .is_some_and(|sp| sp.contains(rev, &hit.path, hit.line))
                };
                if own(&s.head) || own(&s.base) {
                    continue;
                }
                let file = read.get(&(rev.to_owned(), hit.path.clone()));
                let mut site = Site {
                    rev: rev.to_owned(),
                    path: hit.path.clone(),
                    label: hit.path.clone(),
                    line: hit.line,
                    test: test_path(&hit.path),
                    in_diff: in_diff.contains(&hit.path),
                    changed: false,
                    read: file.is_some(),
                };
                if let Some(file) = file {
                    let code = (hit.line as usize)
                        .checked_sub(1)
                        .and_then(|i| file.masked.get(i));
                    if !code.is_some_and(|l| has_word(l, &s.name)) {
                        in_comments += 1;
                        continue;
                    }
                    if file.items.iter().any(|it| {
                        it.name == s.name && it.line == hit.line && it.kind != ItemKind::Impl
                    }) {
                        s.same_named += 1;
                        continue;
                    }
                    site.test |= scan::in_test(&file.items, hit.line);
                    if let Some(i) = scan::innermost(&file.items, hit.line) {
                        site.label = file.items[i].path.clone();
                        site.line = file.items[i].line;
                    }
                }
                if kept == MAX_REFS {
                    s.refs_cut = true;
                    cut += 1;
                    break;
                }
                kept += 1;
                let key = (
                    site.rev.clone(),
                    site.path.clone(),
                    site.label.clone(),
                    site.line,
                );
                let index = *site_index.entry(key).or_insert_with(|| {
                    sites.push(site);
                    sites.len() - 1
                });
                refs.push(Reference {
                    symbol: si,
                    site: index,
                    rev: rev.to_owned(),
                    path: hit.path.clone(),
                    line: hit.line,
                    text: cut_to(hit.text.trim(), 160),
                });
            }
        }
    }
    if cut > 0 {
        partial.push(format!(
            "{cut} symbols have more than {MAX_REFS} references; only the first are kept"
        ));
    }
    // A site that is itself a changed symbol.
    for site in &mut sites {
        site.changed = symbols.iter().any(|s| {
            [&s.head, &s.base]
                .into_iter()
                .flatten()
                .any(|sp| sp.rev == site.rev && sp.path == site.path && sp.line == site.line)
        });
    }
    Ok(Impact {
        range: range.clone(),
        symbols,
        sites,
        refs,
        other,
        rust_files,
        in_comments,
        partial,
        collected: git::now(),
    })
}
