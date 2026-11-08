//! Change coupling (#175 item 5): which files (or folders) the history shows changing in the
//! same commits, how often, and which of them a change at hand leaves out.
//!
//! Co-change is a correlation, not a dependency: two files changed together often may well be
//! independent, and files that depend on each other may never change together. Every pair here
//! is a hint read from the history ([`super::Evidence::Inferred`]), with the commits behind it.

use std::collections::{HashMap, HashSet};

use super::history::History;

/// What is counted, and what is left out.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Options {
    /// Files (`None`) or their folders this many levels down.
    pub depth: Option<usize>,
    /// Pairs changed together fewer times than this aren't kept: too few to say anything.
    pub min_support: u32,
    /// Commits changing more files than this are bulk changes (a reformat, a rename of a
    /// folder, a vendored update) and aren't counted.
    pub bulk: usize,
    /// How many of the strongest pairs the network shows.
    pub top: usize,
    /// Only files under this folder (`""`: all).
    pub filter: String,
    /// Only commits made at or after this time (seconds since the epoch).
    pub since: Option<i64>,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            depth: None,
            min_support: 3,
            bulk: 40,
            top: 40,
            filter: String::new(),
            since: None,
        }
    }
}

/// How many distinct pairs are counted at most; beyond it, pairs first seen later are dropped.
pub const MAX_PAIRS: usize = 200_000;
/// How many commits behind each pair are kept to show.
pub const MAX_PAIR_COMMITS: usize = 200;

/// A file or folder, and in how many of the counted commits it changed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Unit {
    pub name: String,
    pub changes: u32,
}

/// Two units changed together: `a` and `b` index [`Coupling::units`], `a < b` by name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pair {
    pub a: usize,
    pub b: usize,
    /// In how many counted commits both changed.
    pub support: u32,
    /// Those commits (indices into [`History::commits`], newest first), at most
    /// [`MAX_PAIR_COMMITS`].
    pub commits: Vec<usize>,
}

/// What the history says of the units changing together.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Coupling {
    /// Every unit changed in a counted commit, by name.
    pub units: Vec<Unit>,
    /// The pairs changed together at least `min_support` times, strongest first.
    pub pairs: Vec<Pair>,
    /// How many of [`Self::pairs`] the network shows: the first ones.
    pub shown: usize,
    /// Commits counted.
    pub counted: usize,
    /// Commits left out as bulk changes.
    pub bulk: usize,
    /// Merges and commits with nothing under the filter, which count nothing.
    pub skipped: usize,
    /// Commits older than [`Options::since`].
    pub older: usize,
    /// Pairs not counted because [`MAX_PAIRS`] was reached.
    pub pairs_dropped: bool,
    pub options: Options,
}

impl Coupling {
    /// How likely `of` changed when `pair`'s other unit did: support over its changes.
    pub fn confidence(&self, pair: &Pair, given: usize) -> f32 {
        let changes = self.units.get(given).map_or(0, |u| u.changes);
        if changes == 0 {
            0.0
        } else {
            pair.support as f32 / changes as f32
        }
    }

    /// The stronger of the pair's two confidences.
    pub fn strength(&self, pair: &Pair) -> f32 {
        self.confidence(pair, pair.a)
            .max(self.confidence(pair, pair.b))
    }

    /// Why what is shown is less than what is there, when it is.
    pub fn cut(&self) -> Option<String> {
        let mut why = Vec::new();
        if self.shown < self.pairs.len() {
            why.push(format!(
                "only the {} strongest of {} pairs are drawn",
                self.shown,
                self.pairs.len()
            ));
        }
        if self.pairs_dropped {
            why.push(format!("pairs beyond the first {MAX_PAIRS} aren't counted"));
        }
        (!why.is_empty()).then(|| why.join("; "))
    }

    pub fn unit_index(&self, name: &str) -> Option<usize> {
        self.units
            .binary_search_by(|u| u.name.as_str().cmp(name))
            .ok()
    }

    /// The unit `path` counts as with these options, if it is counted at all.
    pub fn unit_of<'a>(&self, path: &'a str) -> Option<&'a str> {
        unit_of(&self.options, path)
    }
}

/// The unit (file or folder) `path` counts as, or `None` when the filter leaves it out.
pub fn unit_of<'a>(options: &Options, path: &'a str) -> Option<&'a str> {
    if !under(&options.filter, path) {
        return None;
    }
    Some(match options.depth {
        None => path,
        Some(depth) => {
            let folder = super::folder_at(path, depth);
            if folder.is_empty() { "/" } else { folder }
        }
    })
}

/// Whether `path` is in folder `filter` (`""`: everything is).
fn under(filter: &str, path: &str) -> bool {
    let filter = filter.trim_matches('/');
    filter.is_empty()
        || path == filter
        || path
            .strip_prefix(filter)
            .is_some_and(|rest| rest.starts_with('/'))
}

/// Counts what changed together in `history`.
pub fn analyze(history: &History, options: &Options) -> Coupling {
    let mut out = Coupling {
        options: options.clone(),
        ..Coupling::default()
    };
    let mut ids: HashMap<String, usize> = HashMap::new();
    let mut names: Vec<String> = Vec::new();
    let mut changes: Vec<u32> = Vec::new();
    let mut pairs: HashMap<(usize, usize), (u32, Vec<usize>)> = HashMap::new();
    for (index, commit) in history.commits.iter().enumerate() {
        if options.since.is_some_and(|since| commit.time < since) {
            out.older += 1;
            continue;
        }
        if commit.is_merge() || commit.files.is_empty() {
            out.skipped += 1;
            continue;
        }
        if commit.files.len() > options.bulk {
            out.bulk += 1;
            continue;
        }
        let mut units: Vec<usize> = commit
            .files
            .iter()
            .filter_map(|f| unit_of(options, &f.path))
            .map(|name| {
                *ids.entry(name.to_owned()).or_insert_with(|| {
                    names.push(name.to_owned());
                    changes.push(0);
                    names.len() - 1
                })
            })
            .collect();
        units.sort_unstable();
        units.dedup();
        if units.is_empty() {
            out.skipped += 1;
            continue;
        }
        out.counted += 1;
        for &u in &units {
            changes[u] += 1;
        }
        for (i, &a) in units.iter().enumerate() {
            for &b in &units[i + 1..] {
                if !pairs.contains_key(&(a, b)) && pairs.len() >= MAX_PAIRS {
                    out.pairs_dropped = true;
                    continue;
                }
                let entry = pairs.entry((a, b)).or_default();
                entry.0 += 1;
                if entry.1.len() < MAX_PAIR_COMMITS {
                    entry.1.push(index);
                }
            }
        }
    }
    // The units by name, so they can be found and are listed the same every time.
    let mut order: Vec<usize> = (0..names.len()).collect();
    order.sort_by(|&x, &y| names[x].cmp(&names[y]));
    let mut rank = vec![0; names.len()];
    for (new, &old) in order.iter().enumerate() {
        rank[old] = new;
    }
    out.units = order
        .iter()
        .map(|&old| Unit {
            name: names[old].clone(),
            changes: changes[old],
        })
        .collect();
    let mut pairs: Vec<Pair> = pairs
        .into_iter()
        .filter(|(_, (support, _))| *support >= options.min_support.max(1))
        .map(|((a, b), (support, commits))| {
            let (a, b) = (rank[a], rank[b]);
            Pair {
                a: a.min(b),
                b: a.max(b),
                support,
                commits,
            }
        })
        .collect();
    pairs.sort_by(|x, y| {
        y.support
            .cmp(&x.support)
            .then_with(|| out.strength(y).total_cmp(&out.strength(x)))
            .then_with(|| (x.a, x.b).cmp(&(y.a, y.b)))
    });
    out.pairs = pairs;
    out.shown = out.pairs.len().min(options.top);
    out
}

/// A unit the history couples with a change, which the change doesn't touch.
#[derive(Clone, Debug, PartialEq)]
pub struct LeftOut {
    /// Index into [`Coupling::units`].
    pub unit: usize,
    /// The changed unit it is coupled with most strongly.
    pub with: usize,
    /// How often the two changed together.
    pub support: u32,
    /// How often it changed when `with` did.
    pub confidence: f32,
    /// The pair behind it, an index into [`Coupling::pairs`].
    pub pair: usize,
}

/// The units coupled with the changed `paths` that the change leaves out, the likeliest to be
/// missing first. `paths` are files; they are counted as units as the analysis counted them.
pub fn left_out(coupling: &Coupling, paths: &[String]) -> Vec<LeftOut> {
    let changed: HashSet<usize> = paths
        .iter()
        .filter_map(|p| coupling.unit_of(p))
        .filter_map(|name| coupling.unit_index(name))
        .collect();
    let mut best: HashMap<usize, LeftOut> = HashMap::new();
    for (index, pair) in coupling.pairs.iter().enumerate() {
        let (given, other) = match (changed.contains(&pair.a), changed.contains(&pair.b)) {
            (true, false) => (pair.a, pair.b),
            (false, true) => (pair.b, pair.a),
            _ => continue,
        };
        let found = LeftOut {
            unit: other,
            with: given,
            support: pair.support,
            confidence: coupling.confidence(pair, given),
            pair: index,
        };
        match best.get(&other) {
            Some(had) if (had.confidence, had.support) >= (found.confidence, found.support) => {}
            _ => {
                best.insert(other, found);
            }
        }
    }
    let mut list: Vec<LeftOut> = best.into_values().collect();
    list.sort_by(|x, y| {
        y.confidence
            .total_cmp(&x.confidence)
            .then_with(|| y.support.cmp(&x.support))
            .then_with(|| x.unit.cmp(&y.unit))
    });
    list
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::insights::history::{FileChange, HistCommit};

    fn commit(hash: &str, time: i64, files: &[&str]) -> HistCommit {
        HistCommit {
            hash: hash.to_owned(),
            parents: vec!["p".to_owned()],
            time,
            author: "Ann".to_owned(),
            subject: hash.to_owned(),
            files: files
                .iter()
                .map(|p| FileChange {
                    path: (*p).to_owned(),
                    old_path: None,
                    added: Some(1),
                    deleted: Some(0),
                })
                .collect(),
        }
    }

    fn history(commits: Vec<HistCommit>) -> History {
        History {
            rev: "HEAD".to_owned(),
            tip: "t".to_owned(),
            limit: 100,
            truncated: false,
            commits,
        }
    }

    fn opts(min_support: u32) -> Options {
        Options {
            min_support,
            ..Options::default()
        }
    }

    fn pair<'a>(c: &'a Coupling, a: &str, b: &str) -> Option<&'a Pair> {
        let (x, y) = (c.unit_index(a)?, c.unit_index(b)?);
        c.pairs.iter().find(|p| (p.a, p.b) == (x.min(y), x.max(y)))
    }

    #[test]
    fn support_counts_commits_changing_both_and_confidence_divides_by_changes()
    -> Result<(), &'static str> {
        let h = history(vec![
            commit("c4", 4, &["a.rs", "b.rs"]),
            commit("c3", 3, &["a.rs", "b.rs", "c.rs"]),
            commit("c2", 2, &["a.rs"]),
            commit("c1", 1, &["a.rs", "c.rs"]),
        ]);
        let c = analyze(&h, &opts(1));
        assert_eq!(c.counted, 4);
        let ab = pair(&c, "a.rs", "b.rs").ok_or("a-b")?;
        assert_eq!(ab.support, 2);
        assert_eq!(ab.commits, [0, 1]);
        let a = c.unit_index("a.rs").ok_or("a")?;
        let b = c.unit_index("b.rs").ok_or("b")?;
        assert_eq!(c.units[a].changes, 4);
        assert!((c.confidence(ab, a) - 0.5).abs() < 1e-6);
        assert!((c.confidence(ab, b) - 1.0).abs() < 1e-6);
        // Strongest first.
        assert!(c.pairs[0].support >= c.pairs[c.pairs.len() - 1].support);
        assert_eq!(c.shown, c.pairs.len());
        assert_eq!(c.cut(), None);
        Ok(())
    }

    #[test]
    fn bulk_commits_and_merges_are_left_out_and_counted_as_such() {
        let many: Vec<String> = (0..50).map(|i| format!("gen/{i}.rs")).collect();
        let many: Vec<&str> = many.iter().map(String::as_str).collect();
        let mut merge = commit("m", 5, &[]);
        merge.parents.push("q".to_owned());
        let h = history(vec![
            merge,
            commit("bulk", 4, &many),
            commit("c2", 2, &["a.rs", "b.rs"]),
        ]);
        let c = analyze(&h, &opts(1));
        assert_eq!(c.bulk, 1);
        assert_eq!(c.skipped, 1);
        assert_eq!(c.counted, 1);
        assert_eq!(c.pairs.len(), 1);
        assert!(c.unit_index("gen/1.rs").is_none());
    }

    #[test]
    fn pairs_below_the_minimum_support_are_dropped_and_the_top_cut_is_said() {
        let h = history(vec![
            commit("c3", 3, &["a", "b"]),
            commit("c2", 2, &["a", "b"]),
            commit("c1", 1, &["a", "c"]),
        ]);
        let c = analyze(&h, &opts(2));
        assert_eq!(c.pairs.len(), 1);
        assert!(pair(&c, "a", "c").is_none());
        let top = analyze(&h, &Options { top: 1, ..opts(1) });
        assert_eq!(top.pairs.len(), 2);
        assert_eq!(top.shown, 1);
        assert!(top.cut().is_some());
    }

    #[test]
    fn folders_the_filter_and_the_time_range_change_what_is_counted() -> Result<(), &'static str> {
        let h = history(vec![
            commit("c3", 30, &["src/a/x.rs", "src/b/y.rs", "docs/z.md"]),
            commit("c2", 20, &["src/a/x.rs", "src/a/w.rs", "src/b/y.rs"]),
            commit("c1", 10, &["README.md", "src/a/x.rs"]),
        ]);
        let folders = analyze(
            &h,
            &Options {
                depth: Some(2),
                ..opts(1)
            },
        );
        let ab = pair(&folders, "src/a", "src/b").ok_or("folders")?;
        assert_eq!(ab.support, 2);
        // A commit changing two files of a folder counts the folder once.
        let a = folders.unit_index("src/a").ok_or("src/a")?;
        assert_eq!(folders.units[a].changes, 3);
        assert!(folders.unit_index("/").is_some());
        let filtered = analyze(
            &h,
            &Options {
                filter: "src/".to_owned(),
                ..opts(1)
            },
        );
        assert!(filtered.unit_index("docs/z.md").is_none());
        assert!(filtered.unit_index("README.md").is_none());
        // `src2/` isn't under `src`.
        assert!(!under("src", "src2/x.rs"));
        let recent = analyze(
            &h,
            &Options {
                since: Some(20),
                ..opts(1)
            },
        );
        assert_eq!(recent.older, 1);
        assert_eq!(recent.counted, 2);
        Ok(())
    }

    #[test]
    fn left_out_files_are_ranked_by_how_often_they_changed_alongside() {
        let h = history(vec![
            commit("c5", 5, &["api.rs", "client.rs", "docs.md"]),
            commit("c4", 4, &["api.rs", "client.rs"]),
            commit("c3", 3, &["api.rs", "client.rs"]),
            commit("c2", 2, &["api.rs", "docs.md"]),
            commit("c1", 1, &["api.rs", "other.rs"]),
        ]);
        let c = analyze(&h, &opts(1));
        let list = left_out(&c, &["api.rs".to_owned()]);
        let names: Vec<&str> = list.iter().map(|l| c.units[l.unit].name.as_str()).collect();
        assert_eq!(names, ["client.rs", "docs.md", "other.rs"]);
        assert_eq!(list[0].support, 3);
        assert!((list[0].confidence - 0.6).abs() < 1e-6);
        // What the change touches isn't left out; a file the history doesn't know adds nothing.
        let both = left_out(&c, &["api.rs".to_owned(), "client.rs".to_owned()]);
        assert!(both.iter().all(|l| c.units[l.unit].name != "client.rs"));
        assert_eq!(c.units[both[0].unit].name, "docs.md");
        assert!(left_out(&c, &["new.rs".to_owned()]).is_empty());
    }

    #[test]
    fn a_real_history_couples_the_files_changed_together() -> Result<(), String> {
        let repo = crate::git::fixture::Repo::new("insights-coupling")?;
        repo.commit("one", &[("a.rs", Some("1\n")), ("b.rs", Some("1\n"))])?;
        repo.commit("two", &[("a.rs", Some("2\n")), ("b.rs", Some("2\n"))])?;
        repo.commit("three", &[("c.rs", Some("1\n"))])?;
        let h = crate::insights::history::read(repo.path(), "HEAD", 100)?;
        let c = analyze(&h, &opts(2));
        let ab = pair(&c, "a.rs", "b.rs").ok_or("a-b")?;
        assert_eq!(ab.support, 2);
        assert_eq!(h.commits[ab.commits[0]].subject, "two");
        assert_eq!(c.pairs.len(), 1);
        // A new commit moves the tip: what was read is stale, and read again it counts more.
        let mut p = h.provenance();
        repo.commit("four", &[("a.rs", Some("3\n")), ("b.rs", Some("3\n"))])?;
        let head = repo.rev("HEAD")?;
        p.check_tips(|_| Some(head.clone()));
        assert!(p.stale.is_some());
        let again = analyze(
            &crate::insights::history::read(repo.path(), "HEAD", 100)?,
            &opts(2),
        );
        assert_eq!(pair(&again, "a.rs", "b.rs").map(|p| p.support), Some(3));
        Ok(())
    }

    #[test]
    fn no_repository_is_an_error_and_an_empty_history_couples_nothing() {
        let missing = std::path::Path::new("/nonexistent/insights-coupling");
        assert!(crate::insights::history::read(missing, "HEAD", 10).is_err());
        let c = analyze(&history(Vec::new()), &Options::default());
        assert_eq!(c.counted, 0);
        assert!(c.pairs.is_empty());
        assert!(left_out(&c, &["a".to_owned()]).is_empty());
    }
}
