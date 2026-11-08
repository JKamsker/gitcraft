//! What each commit of a rewritten branch became (issue #175, item 10): the two versions paired
//! by `git range-diff`, each pair told apart by whether its patch is the same (`git patch-id`,
//! observed) or only similar (range-diff's own matching, inferred), and what range-diff leaves
//! unpaired looked at again for commits folded into one (squashed) or split into several, which
//! range-diff never reports and which is only ever inferred here from the changed lines they
//! share.
//!
//! Read-only: `range-diff`, `log -p` and `patch-id` read the commits, nothing is written.

use std::collections::HashMap;
use std::path::Path;

use super::{Evidence, Provenance};
use crate::git::{self, Pair, PairKind, RangeDiff, Rewrite, Series, SeriesCommit};

/// Commits of each version whose patch is read to match them; beyond, pairs keep only what
/// range-diff says of them.
pub const MAX_PATCHES: usize = 400;
/// Changed lines of one commit compared with others; a bigger commit is compared by its first.
pub const MAX_LINES: usize = 5000;
/// The share of an unpaired commit's changed lines found in one commit of the other version
/// for it to count as folded into it (or split off it).
const FOLDED: f32 = 0.6;
/// The share one of several commits must hold for a split or squash across them to count.
const PART: f32 = 0.2;

/// What became of a commit, or of a pair of them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Class {
    /// The same patch and message, in the same place.
    Unchanged,
    /// The same patch, in another place.
    Reordered,
    /// The same patch (same patch-id), another message.
    Reworded,
    /// The patch changed: paired by similarity.
    Modified,
    /// Several old commits folded into one new one.
    Squashed,
    /// One old commit split into several new ones.
    Split,
    /// Only in the old version.
    Dropped,
    /// Only in the new version.
    Added,
}

impl Class {
    pub const ALL: [Class; 8] = [
        Class::Unchanged,
        Class::Reordered,
        Class::Reworded,
        Class::Modified,
        Class::Squashed,
        Class::Split,
        Class::Dropped,
        Class::Added,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Class::Unchanged => "unchanged",
            Class::Reordered => "reordered",
            Class::Reworded => "reworded",
            Class::Modified => "modified",
            Class::Squashed => "squashed",
            Class::Split => "split",
            Class::Dropped => "dropped",
            Class::Added => "added",
        }
    }

    /// The letter it is marked with, beside its colour.
    pub fn glyph(self) -> &'static str {
        match self {
            Class::Unchanged => "=",
            Class::Reordered => "R",
            Class::Reworded => "M",
            Class::Modified => "~",
            Class::Squashed => "S",
            Class::Split => "Y",
            Class::Dropped => "-",
            Class::Added => "+",
        }
    }

    /// What it means, for tooltips and the table.
    pub fn meaning(self) -> &'static str {
        match self {
            Class::Unchanged => "the same patch and message, in the same place",
            Class::Reordered => "the same patch and message, in another place",
            Class::Reworded => "the same patch (same patch-id), another message",
            Class::Modified => "the patch changed; range-diff paired them by similarity",
            Class::Squashed => "folded together with other commits into one",
            Class::Split => "split into several commits",
            Class::Dropped => "only in the old version: nothing in the new one matches it",
            Class::Added => "only in the new version: nothing in the old one matches it",
        }
    }

    /// Which wins when a commit has ribbons of several classes.
    fn weight(self) -> u8 {
        match self {
            Class::Unchanged => 0,
            Class::Reordered => 1,
            Class::Reworded => 2,
            Class::Modified => 3,
            Class::Squashed | Class::Split => 4,
            Class::Dropped | Class::Added => 0,
        }
    }
}

/// A commit of one version and what became of it.
#[derive(Clone, Debug, PartialEq)]
pub struct End {
    pub commit: SeriesCommit,
    pub class: Class,
    /// Its place among the commits changed.
    pub moved: bool,
}

/// A ribbon from an old commit to a new one.
#[derive(Clone, Debug, PartialEq)]
pub struct Ribbon {
    /// Places in [`Ribbons::old`] and [`Ribbons::new`].
    pub old: usize,
    pub new: usize,
    pub class: Class,
    /// Observed: the same patch-id (or range-diff's `=`). Inferred: paired by similarity, or by
    /// the changed lines the two share.
    pub evidence: Evidence,
    pub moved: bool,
    /// The range-diff pair it is, when range-diff paired them.
    pub pair: Option<usize>,
    /// Why the two are joined, in a line.
    pub why: String,
}

/// The two versions as ribbons: old commits oldest first, new ones oldest first, what joins
/// them.
#[derive(Clone, Debug, Default)]
pub struct Ribbons {
    pub old: Vec<End>,
    pub new: Vec<End>,
    pub ribbons: Vec<Ribbon>,
    /// What range-diff said, for the patches of its pairs.
    pub diff: RangeDiff,
    /// Both series compared.
    pub series: Option<(Series, Series)>,
    pub provenance: Provenance,
}

impl Ribbons {
    /// How many commits of each class, the old version's unpaired ones as dropped and the new
    /// one's as added.
    pub fn counts(&self) -> Vec<(Class, usize)> {
        Class::ALL
            .iter()
            .map(|&c| {
                let n = match c {
                    Class::Dropped => self.old.iter().filter(|e| e.class == c).count(),
                    Class::Added => self.new.iter().filter(|e| e.class == c).count(),
                    _ => self.ribbons.iter().filter(|r| r.class == c).count(),
                };
                (c, n)
            })
            .filter(|(_, n)| *n > 0)
            .collect()
    }

    /// The ribbons from or to the commit at `at` of a side (`old`: the old version).
    pub fn ribbons_of(&self, old: bool, at: usize) -> impl Iterator<Item = (usize, &Ribbon)> {
        self.ribbons
            .iter()
            .enumerate()
            .filter(move |(_, r)| if old { r.old == at } else { r.new == at })
    }
}

/// A commit's patch as far as matching goes: its patch-id and its changed lines (`+` or `-`,
/// its file, and the line trimmed).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Patch {
    pub id: Option<String>,
    pub lines: Vec<String>,
}

/// What the two versions' pairs are, with the patches read of their commits (by hash): the
/// commits classed and joined.
pub fn classify(diff: &RangeDiff, patches: &HashMap<String, Patch>) -> Ribbons {
    let mut old: Vec<(usize, &SeriesCommit)> = Vec::new();
    let mut new: Vec<(usize, &SeriesCommit)> = Vec::new();
    for (i, pair) in diff.pairs.iter().enumerate() {
        if let Some(c) = &pair.old {
            old.push((i, c));
        }
        if let Some(c) = &pair.new {
            new.push((i, c));
        }
    }
    old.sort_by_key(|(_, c)| c.index);
    new.sort_by_key(|(_, c)| c.index);
    let place =
        |list: &[(usize, &SeriesCommit)], pair: usize| list.iter().position(|(p, _)| *p == pair);
    let mut out = Ribbons {
        old: old
            .iter()
            .map(|(_, c)| End {
                commit: (*c).clone(),
                class: Class::Dropped,
                moved: false,
            })
            .collect(),
        new: new
            .iter()
            .map(|(_, c)| End {
                commit: (*c).clone(),
                class: Class::Added,
                moved: false,
            })
            .collect(),
        diff: diff.clone(),
        ..Ribbons::default()
    };
    for (i, pair) in diff.pairs.iter().enumerate() {
        let (Some(o), Some(n)) = (place(&old, i), place(&new, i)) else {
            continue;
        };
        let (class, evidence, why) = paired(pair, patches);
        out.ribbons.push(Ribbon {
            old: o,
            new: n,
            class,
            evidence,
            moved: pair.moved,
            pair: Some(i),
            why,
        });
    }
    infer_folds(&mut out, patches);
    settle_classes(&mut out);
    out
}

/// What a pair range-diff made is: by its mark, and by whether the two patch-ids are the same.
fn paired(pair: &Pair, patches: &HashMap<String, Patch>) -> (Class, Evidence, String) {
    let id = |c: &Option<SeriesCommit>| {
        c.as_ref()
            .and_then(|c| patches.get(&c.hash))
            .and_then(|p| p.id.clone())
    };
    let (a, b) = (id(&pair.old), id(&pair.new));
    match pair.kind {
        PairKind::Same if pair.moved => (
            Class::Reordered,
            Evidence::Observed,
            "range-diff: the same patch and message, moved".to_owned(),
        ),
        PairKind::Same => (
            Class::Unchanged,
            Evidence::Observed,
            "range-diff: the same patch and message".to_owned(),
        ),
        _ => match (a, b) {
            (Some(a), Some(b)) if a == b && message_changed(&pair.body) => (
                Class::Reworded,
                Evidence::Observed,
                "the same patch-id; range-diff: the message changed".to_owned(),
            ),
            // patch-id doesn't see whitespace: the patch changed in that alone.
            (Some(a), Some(b)) if a == b => (
                Class::Modified,
                Evidence::Observed,
                "the same patch-id and message; range-diff: the patch changed in whitespace"
                    .to_owned(),
            ),
            (Some(_), Some(_)) => (
                Class::Modified,
                Evidence::Inferred,
                "range-diff paired them by similarity; their patch-ids differ".to_owned(),
            ),
            _ => (
                Class::Modified,
                Evidence::Inferred,
                "range-diff paired them by similarity; patch-ids not read".to_owned(),
            ),
        },
    }
}

/// Whether range-diff's `body` of a pair changes a line of its commit message section.
fn message_changed(body: &[String]) -> bool {
    let mut in_message = false;
    for line in body {
        let mut chars = line.chars();
        let mark = chars.next();
        let rest = chars.as_str();
        if rest.trim_start().starts_with("## ") {
            in_message = rest.trim() == "## Commit message ##";
        } else if in_message && matches!(mark, Some('-' | '+')) {
            return true;
        }
    }
    false
}

/// How many of `lines` are found in `other`, each line of `other` counted once.
fn found_in(lines: &[String], other: &HashMap<&str, usize>) -> usize {
    let mut left = other.clone();
    lines
        .iter()
        .filter(|l| match left.get_mut(l.as_str()) {
            Some(n) if *n > 0 => {
                *n -= 1;
                true
            }
            _ => false,
        })
        .count()
}

fn bag<'a>(lists: impl IntoIterator<Item = &'a [String]>) -> HashMap<&'a str, usize> {
    let mut bag: HashMap<&str, usize> = HashMap::new();
    for list in lists {
        for line in list {
            *bag.entry(line.as_str()).or_default() += 1;
        }
    }
    bag
}

/// The commits range-diff left unpaired, joined to those of the other version holding most of
/// their changed lines: an old one folded into a new one paired already is squashed, a new one
/// made of an old one paired already was split off it, one found spread over several of the
/// other side is split (or squashed) across them. Every such ribbon is inferred.
fn infer_folds(out: &mut Ribbons, patches: &HashMap<String, Patch>) {
    let lines_of = |c: &SeriesCommit| -> &[String] {
        patches
            .get(&c.hash)
            .map(|p| p.lines.as_slice())
            .unwrap_or(&[])
    };
    let linked_new = |out: &Ribbons, n: usize| out.ribbons.iter().any(|r| r.new == n);
    let linked_old = |out: &Ribbons, o: usize| out.ribbons.iter().any(|r| r.old == o);
    // The old commits unpaired, against every new one.
    for o in 0..out.old.len() {
        if linked_old(out, o) {
            continue;
        }
        let lines = lines_of(&out.old[o].commit);
        if lines.is_empty() {
            continue;
        }
        let shares: Vec<(usize, f32)> = (0..out.new.len())
            .map(|n| {
                let other = bag([lines_of(&out.new[n].commit)]);
                (n, found_in(lines, &other) as f32 / lines.len() as f32)
            })
            .filter(|(_, s)| *s >= PART)
            .collect();
        let Some(&(best, share)) = shares.iter().max_by(|a, b| a.1.total_cmp(&b.1)) else {
            continue;
        };
        if share >= FOLDED {
            let (class, why) = if linked_new(out, best) {
                (
                    Class::Squashed,
                    format!(
                        "{:.0}% of its changed lines are in this commit, which holds another \
                         old commit too",
                        share * 100.0
                    ),
                )
            } else {
                (
                    Class::Modified,
                    format!(
                        "range-diff didn't pair them; {:.0}% of its changed lines are in this \
                         commit",
                        share * 100.0
                    ),
                )
            };
            push_inferred(out, o, best, class, why);
            continue;
        }
        let together = bag(shares.iter().map(|(n, _)| lines_of(&out.new[*n].commit)));
        let union = found_in(lines, &together) as f32 / lines.len() as f32;
        if shares.len() >= 2 && union >= FOLDED {
            for (n, share) in &shares {
                let why = format!(
                    "{:.0}% of its changed lines are in this commit, {:.0}% across {} commits",
                    share * 100.0,
                    union * 100.0,
                    shares.len()
                );
                push_inferred(out, o, *n, Class::Split, why);
            }
        }
    }
    // The new commits still unpaired, against every old one.
    for n in 0..out.new.len() {
        if linked_new(out, n) {
            continue;
        }
        let lines = lines_of(&out.new[n].commit);
        if lines.is_empty() {
            continue;
        }
        let shares: Vec<(usize, f32)> = (0..out.old.len())
            .map(|o| {
                let other = bag([lines_of(&out.old[o].commit)]);
                (o, found_in(lines, &other) as f32 / lines.len() as f32)
            })
            .filter(|(_, s)| *s >= PART)
            .collect();
        let Some(&(best, share)) = shares.iter().max_by(|a, b| a.1.total_cmp(&b.1)) else {
            continue;
        };
        if share >= FOLDED {
            let class = if linked_old(out, best) {
                Class::Split
            } else {
                Class::Modified
            };
            let why = format!(
                "{:.0}% of this commit's changed lines come from the old one",
                share * 100.0
            );
            push_inferred(out, best, n, class, why);
            continue;
        }
        let together = bag(shares.iter().map(|(o, _)| lines_of(&out.old[*o].commit)));
        let union = found_in(lines, &together) as f32 / lines.len() as f32;
        if shares.len() >= 2 && union >= FOLDED {
            for (o, share) in &shares {
                let why = format!(
                    "{:.0}% of this commit's changed lines come from the old one, {:.0}% from \
                     {} old commits",
                    share * 100.0,
                    union * 100.0,
                    shares.len()
                );
                push_inferred(out, *o, n, Class::Squashed, why);
            }
        }
    }
}

fn push_inferred(out: &mut Ribbons, old: usize, new: usize, class: Class, why: String) {
    out.ribbons.push(Ribbon {
        old,
        new,
        class,
        evidence: Evidence::Inferred,
        moved: false,
        pair: None,
        why,
    });
}

/// A new commit several old ones lead into is a squash, an old one leading to several new ones
/// a split, whatever range-diff called its pair; each commit then takes the strongest class of
/// its ribbons.
fn settle_classes(out: &mut Ribbons) {
    let into = |out: &Ribbons, n: usize| out.ribbons.iter().filter(|r| r.new == n).count();
    let from = |out: &Ribbons, o: usize| out.ribbons.iter().filter(|r| r.old == o).count();
    let classes: Vec<Class> = out
        .ribbons
        .iter()
        .map(|r| {
            if into(out, r.new) >= 2 {
                Class::Squashed
            } else if from(out, r.old) >= 2 {
                Class::Split
            } else {
                r.class
            }
        })
        .collect();
    for (r, class) in out.ribbons.iter_mut().zip(classes) {
        r.class = class;
    }
    for r in &out.ribbons {
        for end in [&mut out.old[r.old], &mut out.new[r.new]] {
            if matches!(end.class, Class::Dropped | Class::Added)
                || r.class.weight() > end.class.weight()
            {
                end.class = r.class;
            }
            end.moved |= r.moved;
        }
    }
}

/// The patches of the commits of `series` (the first [`MAX_PATCHES`] of them), by hash; and
/// whether there were more.
pub fn patches(repo: &Path, series: &Series) -> Result<(HashMap<String, Patch>, bool), String> {
    for rev in [&series.base, &series.tip] {
        not_an_option(rev)?;
    }
    let range = format!("{}..{}", series.base, series.tip);
    let limit = format!("--max-count={}", MAX_PATCHES + 1);
    let out = git::run(
        Some(repo),
        &[
            "-c",
            "core.quotePath=false",
            "log",
            "--no-show-signature",
            "--no-merges",
            "--no-color",
            "--no-ext-diff",
            "--no-textconv",
            "-p",
            "--format=commit %H",
            &limit,
            &range,
            "--",
        ],
    )?;
    let mut found = parse_patches(&out);
    let more = found.len() > MAX_PATCHES;
    if more {
        // The oldest is left out: `log` lists newest first, so it is the last one parsed.
        let last = out
            .lines()
            .rev()
            .find_map(|l| l.strip_prefix("commit "))
            .map(str::to_owned);
        if let Some(last) = last {
            found.remove(&last);
        }
    }
    // patch-id reads what `log -p` printed; a commit changing nothing has none.
    if let Ok(ids) = git::run_input(Some(repo), &["patch-id", "--stable"], &out) {
        for line in ids.lines() {
            if let Some((id, commit)) = line.split_once(' ')
                && let Some(p) = found.get_mut(commit.trim())
            {
                p.id = Some(id.to_owned());
            }
        }
    }
    Ok((found, more))
}

/// `log -p --format='commit %H'`: the changed lines of each commit, by hash.
pub(crate) fn parse_patches(out: &str) -> HashMap<String, Patch> {
    let mut found: HashMap<String, Patch> = HashMap::new();
    let mut current: Option<String> = None;
    let mut file = String::new();
    let mut in_hunk = false;
    for line in out.lines() {
        if let Some(hash) = line.strip_prefix("commit ") {
            current = Some(hash.trim().to_owned());
            found.entry(hash.trim().to_owned()).or_default();
            in_hunk = false;
            continue;
        }
        let Some(hash) = &current else {
            continue;
        };
        if line.starts_with("diff --git ") {
            in_hunk = false;
            file.clear();
            continue;
        }
        if !in_hunk {
            if let Some(path) = line.strip_prefix("+++ b/") {
                file = path.trim_end_matches('\t').to_owned();
            } else if let Some(path) = line.strip_prefix("--- a/")
                && file.is_empty()
            {
                file = path.trim_end_matches('\t').to_owned();
            }
        }
        if line.starts_with("@@") {
            in_hunk = true;
            continue;
        }
        if !in_hunk {
            continue;
        }
        let (sign, text) = match line.split_at_checked(1) {
            Some(("+", text)) => ('+', text),
            Some(("-", text)) => ('-', text),
            _ => continue,
        };
        let text = text.trim();
        // Blank lines and lone braces match everywhere: they say nothing of where a change went.
        if !text.chars().any(char::is_alphanumeric) {
            continue;
        }
        if let Some(p) = found.get_mut(hash)
            && p.lines.len() < MAX_LINES
        {
            p.lines.push(format!("{sign}{file}\u{1f}{text}"));
        }
    }
    found
}

/// The two versions `old` and `new` compared and classed; `names` the tips as the provenance
/// names them (`before`, `refs/heads/x`…), `scope` what is compared.
pub fn read(
    repo: &Path,
    old: &Series,
    new: &Series,
    names: (&str, &str),
    scope: &str,
) -> Result<Ribbons, String> {
    let diff = git::range_diff(repo, old, new)?;
    let (mut read, more_old) = patches(repo, old)?;
    let (new_patches, more_new) = patches(repo, new)?;
    read.extend(new_patches);
    let mut ribbons = classify(&diff, &read);
    ribbons.series = Some((old.clone(), new.clone()));
    let mut provenance = Provenance::new("git range-diff, patch-id", scope)
        .tip(names.0, &old.tip)
        .tip(names.1, &new.tip);
    if more_old || more_new {
        provenance = provenance.partial(format!(
            "more than {MAX_PATCHES} commits: those past them are paired by range-diff only, \
             not checked for the same patch-id, squashes or splits"
        ));
    }
    ribbons.provenance = provenance;
    Ok(ribbons)
}

/// How a pair of commits joined by a ribbon differ, as range-diff prints it: range-diff's own
/// body for a pair it made, else [`alone_patch`].
#[cfg(test)]
pub fn ribbon_patch(repo: &Path, ribbons: &Ribbons, at: usize) -> Result<Vec<String>, String> {
    let r = ribbons.ribbons.get(at).ok_or("no such ribbon")?;
    if let Some(pair) = r.pair.and_then(|p| ribbons.diff.pairs.get(p)) {
        return Ok(pair.body.clone());
    }
    let (old, new) = (
        &ribbons.old[r.old].commit.hash,
        &ribbons.new[r.new].commit.hash,
    );
    alone_patch(repo, old, new)
}

/// How commit `new` differs from `old`, for a ribbon range-diff didn't make: the two compared
/// alone, or when range-diff doesn't pair even those, the old patch (`-`) and then the new one
/// (`+`).
pub fn alone_patch(repo: &Path, old: &str, new: &str) -> Result<Vec<String>, String> {
    let alone = |hash: &str| Series::new(&format!("{hash}^"), hash);
    if let Ok(diff) = git::range_diff(repo, &alone(old), &alone(new))
        && let Some(pair) = diff
            .pairs
            .iter()
            .find(|p| p.old.is_some() && p.new.is_some())
    {
        return Ok(pair.body.clone());
    }
    let mut body = git::lone_patch(repo, old, '-')?;
    body.extend(git::lone_patch(repo, new, '+')?);
    Ok(body)
}

/// What can be compared: a rewrite gitcraft kept, a branch and its upstream, a pull request
/// as last seen and now.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Source {
    Rewrite(Rewrite),
    /// A local branch (short name) and its upstream (`origin/x`), diverged.
    Upstream {
        branch: String,
        upstream: String,
    },
    /// A pull request whose head moved since the version of it seen: its number, title, head
    /// now, and its base branch.
    Pr {
        number: u64,
        title: String,
        head: String,
        base: String,
    },
}

impl Source {
    pub fn label(&self) -> String {
        match self {
            Source::Rewrite(r) => format!(
                "{} before and after its rewrite ({})",
                r.branch,
                git::relative_time(r.time, git::now())
            ),
            Source::Upstream { branch, upstream } => format!("{upstream} and {branch}"),
            Source::Pr { number, title, .. } => format!("#{number} since last seen: {title}"),
        }
    }

    /// Two branches neither of which is the older: a commit only on one side is "only on the
    /// left / right", not dropped or added.
    pub fn sides(&self) -> bool {
        matches!(self, Source::Upstream { .. })
    }
}

/// A local branch as [`sources`] takes it: name, upstream, (ahead, behind).
pub type Tracking = (String, String, (u32, u32));

/// A pull request as [`sources`] takes it: number, title, head, base branch, and the head of
/// the version last seen (if one was).
pub type PrSeen = (u64, String, String, String, Option<String>);

/// Everything there is to compare: the rewrites kept (newest first), the branches diverged from
/// their upstream (the checked-out one first), the pull requests changed since last seen.
/// Empty when there is nothing.
pub fn sources(
    rewrites: &[Rewrite],
    branches: &[Tracking],
    current: Option<&str>,
    prs: &[PrSeen],
) -> Vec<Source> {
    let mut list: Vec<Source> = rewrites.iter().cloned().map(Source::Rewrite).collect();
    let mut diverged: Vec<&Tracking> = branches
        .iter()
        .filter(|(_, up, (ahead, behind))| !up.is_empty() && *ahead > 0 && *behind > 0)
        .collect();
    diverged.sort_by_key(|(name, _, _)| Some(name.as_str()) != current);
    list.extend(diverged.into_iter().map(|(name, up, _)| Source::Upstream {
        branch: name.clone(),
        upstream: up.clone(),
    }));
    list.extend(
        prs.iter()
            .filter(|(_, _, head, _, seen)| seen.as_ref().is_some_and(|s| s != head))
            .map(|(number, title, head, base, _)| Source::Pr {
                number: *number,
                title: title.clone(),
                head: head.clone(),
                base: base.clone(),
            }),
    );
    list
}

/// The version of pull request `number` seen (see [`git::reviewed`]) and the one now (`head`,
/// from where it forks off `base_refs`, the first that is here): nothing is fetched, so a head
/// that isn't here is an error.
pub fn pr_series(
    repo: &Path,
    repo_key: &str,
    number: u64,
    head: &str,
    base_refs: &[String],
) -> Result<(Series, Series), String> {
    let old = git::reviewed(repo, repo_key, number)?
        .ok_or_else(|| format!("no version of #{number} was seen in gitcraft"))?;
    not_an_option(head)?;
    if git::run(
        Some(repo),
        &["cat-file", "-e", &format!("{head}^{{commit}}")],
    )
    .is_err()
    {
        return Err(format!(
            "the head of #{number} ({}) isn't in this repository yet: open its page once, \
             which fetches it",
            crate::repo::short(head)
        ));
    }
    let base = base_refs
        .iter()
        .chain(std::iter::once(&old.base))
        .find_map(|b| git::merge_base(repo, b, head).ok())
        .ok_or_else(|| format!("can't tell where the commits of #{number} start"))?;
    Ok((old, Series::new(&base, head)))
}

/// `rev` as git takes it among other arguments: never as an option.
pub(crate) fn not_an_option(rev: &str) -> Result<&str, String> {
    if rev.starts_with('-') || rev.is_empty() {
        Err(format!("{rev:?} isn't a revision"))
    } else {
        Ok(rev)
    }
}

#[cfg(test)]
pub(crate) mod tests;
