//! Two versions of a series of commits side by side, as `git range-diff` pairs them: which
//! commits stayed the same, which changed, which were added or dropped, and how the patch of
//! each pair changed (the diff of the two diffs).

use std::path::Path;

use super::{FS, NO_SIGNATURE, RS, UTF8, run};

/// More commits than this on either side are not compared: pairing is quadratic in them.
pub const MAX_SERIES: usize = 1000;

/// A series of commits: those `tip` has and `base` hasn't (`base..tip`), as full hashes.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Series {
    pub base: String,
    pub tip: String,
}

impl Series {
    pub fn new(base: &str, tip: &str) -> Self {
        Self {
            base: base.to_owned(),
            tip: tip.to_owned(),
        }
    }

    /// `base..tip`, as git takes it.
    fn range(&self) -> String {
        format!("{}..{}", self.base, self.tip)
    }
}

/// How a commit of the old series relates to one of the new.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PairKind {
    /// The same patch (and message): `=`.
    Same,
    /// The patch or the message changed: `!`.
    Changed,
    /// Only in the new series: `>`.
    Added,
    /// Only in the old series: `<`.
    Dropped,
}

/// One commit of a series.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SeriesCommit {
    pub hash: String,
    pub summary: String,
    /// Where it is in its series, from 1, oldest first.
    pub index: usize,
    /// It changes no file (its patch became empty, say once its change was upstream).
    pub empty: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pair {
    pub kind: PairKind,
    pub old: Option<SeriesCommit>,
    pub new: Option<SeriesCommit>,
    /// It moved: its place among the commits of both series isn't where it was.
    pub moved: bool,
    /// How the patch changed, as range-diff prints it with its indent taken off: each line
    /// starts with the outer mark (` `, `-`, `+` or `@`), then the line of the patch.
    pub body: Vec<String>,
}

/// The two series paired, in the order of the new one, each dropped commit where it was.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RangeDiff {
    pub pairs: Vec<Pair>,
}

/// How many pairs are of each kind; an unchanged pair that moved counts as moved, not the
/// same, so that the counts add up to the pairs.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Counts {
    pub same: usize,
    pub changed: usize,
    pub added: usize,
    pub dropped: usize,
    pub moved: usize,
}

impl Counts {
    /// Nothing changed at all.
    pub fn identical(&self) -> bool {
        self.changed == 0 && self.added == 0 && self.dropped == 0 && self.moved == 0
    }
}

impl RangeDiff {
    pub fn counts(&self) -> Counts {
        let mut counts = Counts::default();
        for pair in &self.pairs {
            match pair.kind {
                PairKind::Same if pair.moved => counts.moved += 1,
                PairKind::Same => counts.same += 1,
                PairKind::Changed => counts.changed += 1,
                PairKind::Added => counts.added += 1,
                PairKind::Dropped => counts.dropped += 1,
            }
        }
        counts
    }
}

/// Compares the series `old` with `new` (`git range-diff old new`).
pub fn range_diff(repo: &Path, old: &Series, new: &Series) -> Result<RangeDiff, String> {
    // Counted first: listing a huge series diffs every commit of it.
    for (side, series) in [("old", old), ("new", new)] {
        let n = series_len(repo, series)?;
        if n > MAX_SERIES {
            return Err(crate::i18n::message(
                "backend-the-side-version-has-n-commits-more-than-the-max-series-gitcraft-a7d6fa",
                &[
                    ("side", (side).to_string().into()),
                    ("count", (n).into()),
                    ("max_series", (MAX_SERIES).into()),
                ],
            ));
        }
    }
    let old_commits = series_commits(repo, old)?;
    let new_commits = series_commits(repo, new)?;
    // range-diff refuses a range without commits: everything on the other side is then added
    // or dropped, nothing to pair.
    if old_commits.is_empty() || new_commits.is_empty() {
        return Ok(one_sided(old_commits, new_commits));
    }
    let out = run(
        Some(repo),
        &[
            "range-diff",
            "--no-color",
            "--no-abbrev",
            &old.range(),
            &new.range(),
        ],
    )?;
    let raw = parse(&out);
    Ok(resolve(raw, &old_commits, &new_commits))
}

/// How many commits `series` has, merges left out.
fn series_len(repo: &Path, series: &Series) -> Result<usize, String> {
    for rev in [&series.base, &series.tip] {
        super::not_an_option(rev)?;
    }
    let out = run(
        Some(repo),
        &["rev-list", "--count", "--no-merges", &series.range(), "--"],
    )?;
    out.trim().parse().map_err(|_| {
        crate::i18n::message(
            "backend-git-counted-commits-35b029",
            &[("detail", format!("{:?}", out.trim()).into())],
        )
    })
}

/// The commits of `series`, oldest first, as range-diff takes them (merges left out).
fn series_commits(repo: &Path, series: &Series) -> Result<Vec<SeriesCommit>, String> {
    // Ranges built from hashes the callers resolved: nothing here reads as an option.
    for rev in [&series.base, &series.tip] {
        super::not_an_option(rev)?;
    }
    let format = format!("--format={RS}%H{FS}%s");
    let out = run(
        Some(repo),
        &[
            "log",
            NO_SIGNATURE,
            UTF8,
            "--no-merges",
            "--reverse",
            // As range-diff orders (and numbers) a series.
            "--date-order",
            "--shortstat",
            &format,
            &series.range(),
            "--",
        ],
    )?;
    Ok(parse_series(&out))
}

/// `log`'s records: hash, summary and, unless the commit changes nothing, its short stat.
fn parse_series(out: &str) -> Vec<SeriesCommit> {
    out.split(RS)
        .filter_map(|record| {
            let (head, rest) = record.split_once('\n').unwrap_or((record, ""));
            let (hash, summary) = head.split_once(FS)?;
            let hash = hash.trim();
            (!hash.is_empty()).then(|| (hash.to_owned(), summary.to_owned(), rest))
        })
        .enumerate()
        .map(|(i, (hash, summary, rest))| SeriesCommit {
            hash,
            summary,
            index: i + 1,
            empty: !rest.contains("changed"),
        })
        .collect()
}

/// Every commit added (`old` empty) or dropped (`new` empty).
fn one_sided(old: Vec<SeriesCommit>, new: Vec<SeriesCommit>) -> RangeDiff {
    let dropped = old.into_iter().map(|c| Pair {
        kind: PairKind::Dropped,
        old: Some(c),
        new: None,
        moved: false,
        body: Vec::new(),
    });
    let added = new.into_iter().map(|c| Pair {
        kind: PairKind::Added,
        old: None,
        new: Some(c),
        moved: false,
        body: Vec::new(),
    });
    RangeDiff {
        pairs: dropped.chain(added).collect(),
    }
}

/// A pair as range-diff prints it: positions (from 1) and hashes as written.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RawPair {
    pub kind: PairKind,
    pub old: Option<(usize, String)>,
    pub new: Option<(usize, String)>,
    pub summary: String,
    pub body: Vec<String>,
}

/// Reads `git range-diff --no-color` output: a line per pair, each changed pair followed by
/// the diff of its patches indented by four spaces.
pub(crate) fn parse(out: &str) -> Vec<RawPair> {
    let mut pairs: Vec<RawPair> = Vec::new();
    for line in out.split_terminator('\n') {
        // A carriage return ends a pair's line only; in a patch it is part of the file's line.
        if let Some(pair) = header(line.strip_suffix('\r').unwrap_or(line)) {
            pairs.push(pair);
        } else if let Some(last) = pairs.last_mut() {
            // Only what follows a pair belongs to it; a blank line keeps its place.
            let body = line.strip_prefix("    ").unwrap_or(line.trim_start());
            last.body.push(body.to_owned());
        }
    }
    for pair in &mut pairs {
        while pair.body.last().is_some_and(|l| l.trim().is_empty()) {
            pair.body.pop();
        }
    }
    pairs
}

/// A pair's line: `1:  abc = 1:  def Summary`. The numbers are padded on the left to the width
/// of the largest; what follows a pair is indented by four spaces, which no pair's line is.
fn header(line: &str) -> Option<RawPair> {
    let indent = line.len() - line.trim_start_matches(' ').len();
    if indent >= 4 {
        return None;
    }
    let rest = &line[indent..];
    let (old, rest) = side(rest)?;
    let rest = rest.strip_prefix(' ')?;
    let mut chars = rest.chars();
    let kind = match chars.next()? {
        '=' => PairKind::Same,
        '!' => PairKind::Changed,
        '<' => PairKind::Dropped,
        '>' => PairKind::Added,
        _ => return None,
    };
    let rest = chars.as_str().strip_prefix(' ')?;
    let rest = rest.trim_start_matches(' ');
    let (new, rest) = side(rest)?;
    let summary = rest.strip_prefix(' ').unwrap_or(rest).to_owned();
    // What the mark says each side has, it has.
    let fits = match kind {
        PairKind::Same | PairKind::Changed => old.is_some() && new.is_some(),
        PairKind::Dropped => old.is_some() && new.is_none(),
        PairKind::Added => old.is_none() && new.is_some(),
    };
    fits.then_some(RawPair {
        kind,
        old,
        new,
        summary,
        body: Vec::new(),
    })
}

/// One side of a pair's line, `12:  abc123` or `-:  -------`, and what follows it.
fn side(text: &str) -> Option<(Option<(usize, String)>, &str)> {
    let (number, rest) = text.split_once(':')?;
    let rest = rest.trim_start_matches(' ');
    let end = rest.find(' ').unwrap_or(rest.len());
    let (hash, rest) = rest.split_at(end);
    if number == "-" {
        return (!hash.is_empty() && hash.bytes().all(|b| b == b'-')).then_some((None, rest));
    }
    let n: usize = number.parse().ok()?;
    let hex = hash.len() >= 4 && hash.bytes().all(|b| b.is_ascii_hexdigit());
    (n > 0 && hex).then(|| (Some((n, hash.to_owned())), rest))
}

/// The pairs read with their commits as `log` listed them (by hash, or by position when a hash
/// isn't found), and which of them moved.
fn resolve(raw: Vec<RawPair>, old: &[SeriesCommit], new: &[SeriesCommit]) -> RangeDiff {
    let find = |list: &[SeriesCommit], side: &Option<(usize, String)>, summary: &str| {
        let (n, hash) = side.as_ref()?;
        let found = list
            .iter()
            .find(|c| c.hash.starts_with(hash.as_str()))
            .or_else(|| list.get(n.checked_sub(1)?))
            .cloned()
            // Numbered as range-diff numbers it, which what moved is worked out from.
            .map(|c| SeriesCommit { index: *n, ..c });
        Some(found.unwrap_or_else(|| SeriesCommit {
            hash: hash.clone(),
            summary: summary.to_owned(),
            index: *n,
            empty: false,
        }))
    };
    let mut pairs: Vec<Pair> = raw
        .into_iter()
        .map(|p| Pair {
            kind: p.kind,
            old: find(old, &p.old, &p.summary),
            new: find(new, &p.new, &p.summary),
            moved: false,
            body: p.body,
        })
        .collect();
    mark_moved(&mut pairs);
    RangeDiff { pairs }
}

/// Marks the pairs in both series whose place changed: the fewest that, left out, leave the
/// others in the order they were (those off a longest increasing run of old positions).
fn mark_moved(pairs: &mut [Pair]) {
    // In the new series' order: the pairs are.
    let both: Vec<(usize, usize)> = pairs
        .iter()
        .enumerate()
        .filter(|(_, p)| p.new.is_some())
        .filter_map(|(i, p)| Some((i, p.old.as_ref()?.index)))
        .collect();
    let kept = longest_increasing(&both.iter().map(|(_, old)| *old).collect::<Vec<_>>());
    for (k, (i, _)) in both.iter().enumerate() {
        if !kept.contains(&k) {
            pairs[*i].moved = true;
        }
    }
}

/// The places in `values` of one longest strictly increasing run of them.
fn longest_increasing(values: &[usize]) -> std::collections::HashSet<usize> {
    // tails[l]: the place of the smallest last value of a run of length l + 1.
    let mut tails: Vec<usize> = Vec::new();
    let mut before: Vec<Option<usize>> = vec![None; values.len()];
    for (i, &v) in values.iter().enumerate() {
        let at = tails.partition_point(|&t| values[t] < v);
        before[i] = at.checked_sub(1).map(|p| tails[p]);
        if at == tails.len() {
            tails.push(i);
        } else {
            tails[at] = i;
        }
    }
    let mut kept = std::collections::HashSet::new();
    let mut at = tails.last().copied();
    while let Some(i) = at {
        kept.insert(i);
        at = before[i];
    }
    kept
}

/// The whole patch of one commit (its message, then its diff), for a commit only one series
/// has: as range-diff would print it against nothing, each line marked `mark` (`+` or `-`).
pub fn lone_patch(repo: &Path, hash: &str, mark: char) -> Result<Vec<String>, String> {
    super::not_an_option(hash)?;
    let out = run(
        Some(repo),
        &[
            "show",
            NO_SIGNATURE,
            UTF8,
            "--no-color",
            "--no-ext-diff",
            "--format=%B",
            hash,
            "--",
        ],
    )?;
    Ok(out
        .trim_end()
        .split('\n')
        .map(|line| format!("{mark}{}", line.strip_suffix('\r').unwrap_or(line)))
        .collect())
}

#[cfg(test)]
mod tests;
