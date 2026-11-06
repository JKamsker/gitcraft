//! The original of each conflict: its base (from the file's diff3 markers, or made again from
//! the index), and what each side inserted and deleted relative to it.

use super::Segment;

/// What the index gave for the bases of a file's conflicts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum IndexBases {
    /// Not asked: every conflict has its base in the file.
    NotNeeded,
    Loading,
    /// The file made again from the index, its conflicts with their bases.
    Found(Vec<Segment>),
    /// Both sides added the file (add/add): there is no original.
    Added,
    Failed(String),
}

/// The original of one conflict, as the card shows it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Original<'a> {
    Text(&'a str),
    Loading,
    /// Both sides added the file.
    Added,
    /// Not found, and why.
    Unknown(&'a str),
}

/// How a line of a side differs from the base.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Change {
    Same,
    Added,
    Removed,
}

/// A side's lines against the base: each line with how it changed, without its line ending.
pub(super) type Lines = Vec<(Change, String)>;

/// Above this many line pairs, the lines between the common start and end are shown as all
/// deleted, then all inserted, rather than matched line by line.
const MAX_CELLS: usize = 1_000_000;

/// `text` without carriage returns: a working tree with CRLF line endings against blobs with
/// LF ones.
fn plain(text: &str) -> std::borrow::Cow<'_, str> {
    if text.contains('\r') {
        text.replace('\r', "").into()
    } else {
        text.into()
    }
}

/// For each conflict of `segments` (in order), its base in `found` (the file made again from
/// the index, with bases): the run of `found`'s conflicts, and the text between them, that has
/// the same two sides. git's own merge joins conflicts that only a line without letters or
/// digits (`}`) keeps apart, which the rebuilt file keeps apart; a conflict found nowhere (the
/// file edited since) has none.
pub(super) fn match_bases(segments: &[Segment], found: &[Segment]) -> Vec<Option<String>> {
    let mut next = 0;
    let mut bases = Vec::new();
    for (ours, _, theirs) in conflicts(segments) {
        let (ours, theirs) = (plain(ours), plain(theirs));
        let hit = (next..found.len())
            .filter(|&i| matches!(found[i], Segment::Conflict { .. }))
            .find_map(|start| span(found, start, &ours, &theirs));
        if let Some((end, _)) = &hit {
            next = *end;
        }
        bases.push(hit.map(|(_, base)| base));
    }
    bases
}

/// Whether the conflicts of `found` from `start` on, and the text between them, add up to
/// `ours` and `theirs`: where that run ends, and its base. Compared in place, so a run that
/// isn't it stops at its first difference.
fn span(found: &[Segment], start: usize, ours: &str, theirs: &str) -> Option<(usize, String)> {
    let (mut o, mut t) = (0, 0);
    // `piece` is next in `whole` at `at`: moves `at` past it.
    let next = |whole: &str, at: &mut usize, piece: &str| {
        let fits = whole[*at..].starts_with(piece);
        *at += piece.len();
        fits
    };
    for (i, seg) in found.iter().enumerate().skip(start) {
        let (so, st) = match seg {
            Segment::Common(text) => (text, text),
            Segment::Conflict { ours, theirs, .. } => (ours, theirs),
        };
        let (so, st) = (plain(so), plain(st));
        if !next(ours, &mut o, &so) || !next(theirs, &mut t, &st) {
            return None;
        }
        if matches!(seg, Segment::Conflict { .. }) && o == ours.len() && t == theirs.len() {
            let base = found[start..=i]
                .iter()
                .map(|seg| match seg {
                    Segment::Common(text) => plain(text).into_owned(),
                    Segment::Conflict { base, .. } => {
                        plain(base.as_deref().unwrap_or_default()).into_owned()
                    }
                })
                .collect();
            return Some((i + 1, base));
        }
    }
    None
}

/// The lines of `text` without their line endings.
fn lines(text: &str) -> Vec<&str> {
    text.split_inclusive('\n')
        .map(|l| l.trim_end_matches(['\r', '\n']))
        .collect()
}

/// `side` against `base`, line by line: the lines both have, those `side` inserted and those
/// it deleted, in the order they come.
pub(super) fn changes(base: &str, side: &str) -> Lines {
    let (old, new) = (lines(base), lines(side));
    let start = old.iter().zip(&new).take_while(|(a, b)| a == b).count();
    let end = old[start..]
        .iter()
        .rev()
        .zip(new[start..].iter().rev())
        .take_while(|(a, b)| a == b)
        .count();
    let (old_mid, new_mid) = (&old[start..old.len() - end], &new[start..new.len() - end]);
    let mut out: Lines = old[..start]
        .iter()
        .map(|l| (Change::Same, (*l).to_owned()))
        .collect();
    out.extend(middle(old_mid, new_mid));
    out.extend(
        old[old.len() - end..]
            .iter()
            .map(|l| (Change::Same, (*l).to_owned())),
    );
    out
}

/// The lines between the common start and end, matched by their longest common run.
fn middle(old: &[&str], new: &[&str]) -> Lines {
    let (n, m) = (old.len(), new.len());
    let removed = |l: &&str| (Change::Removed, (*l).to_owned());
    let added = |l: &&str| (Change::Added, (*l).to_owned());
    if n.saturating_mul(m) > MAX_CELLS {
        return old
            .iter()
            .map(removed)
            .chain(new.iter().map(added))
            .collect();
    }
    // `lcs[i][j]`: the longest common run of old[i..] and new[j..].
    let mut lcs = vec![vec![0u32; m + 1]; n + 1];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            lcs[i][j] = if old[i] == new[j] {
                lcs[i + 1][j + 1] + 1
            } else {
                lcs[i + 1][j].max(lcs[i][j + 1])
            };
        }
    }
    let (mut i, mut j, mut out) = (0, 0, Vec::with_capacity(n + m));
    while i < n || j < m {
        if i < n && j < m && old[i] == new[j] {
            out.push((Change::Same, old[i].to_owned()));
            i += 1;
            j += 1;
        } else if i < n && (j == m || lcs[i + 1][j] >= lcs[i][j + 1]) {
            out.push(removed(&old[i]));
            i += 1;
        } else {
            out.push(added(&new[j]));
            j += 1;
        }
    }
    out
}

/// What a side did to the base, in a few words: "deleted all 3 lines", "inserted 1 line,
/// deleted 2", "unchanged".
pub(super) fn summary(lines: &Lines) -> String {
    let count = |c| lines.iter().filter(|(k, _)| *k == c).count();
    let (added, removed, same) = (
        count(Change::Added),
        count(Change::Removed),
        count(Change::Same),
    );
    let n = |k: usize| format!("{k} line{}", if k == 1 { "" } else { "s" });
    match (added, removed) {
        (0, 0) => "unchanged".to_owned(),
        (0, r) if same == 0 => format!("deleted all {}", n(r)),
        (0, r) => format!("deleted {}", n(r)),
        (a, 0) => format!("inserted {}", n(a)),
        (a, r) if same == 0 => format!("replaced all {} with {}", n(r), n(a)),
        (a, r) => format!("inserted {}, deleted {}", n(a), n(r)),
    }
}

/// The conflicts of `segments`, in order: (ours, the base the file has, theirs).
fn conflicts(segments: &[Segment]) -> impl Iterator<Item = (&str, Option<&str>, &str)> {
    segments.iter().filter_map(|seg| match seg {
        Segment::Conflict { ours, base, theirs } => {
            Some((ours.as_str(), base.as_deref(), theirs.as_str()))
        }
        Segment::Common(_) => None,
    })
}

/// The originals of an editor's conflicts: where their bases come from, which cards show
/// theirs, and the sides' changes worked out for those.
#[derive(Debug)]
pub(super) struct State {
    pub index: IndexBases,
    /// The index was asked for [`IndexBases`] (once per [`IndexBases::Loading`]).
    pub asked: bool,
    /// Each conflict's base: the file's own, or the one found in [`IndexBases::Found`].
    bases: Vec<Option<String>>,
    /// Which conflicts' Original sections are open.
    open: Vec<bool>,
    /// The changes of (ours, theirs) against the base, for the open sections that have one.
    diffs: Vec<Option<Box<(Lines, Lines)>>>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            index: IndexBases::NotNeeded,
            asked: false,
            bases: Vec::new(),
            open: Vec::new(),
            diffs: Vec::new(),
        }
    }
}

impl State {
    /// For the conflicts of `segments`, just read: the index is to be asked for the bases the
    /// file doesn't have.
    pub fn new(segments: &[Segment]) -> Self {
        let mut state = Self::default();
        state.reset(segments);
        state
    }

    /// The conflicts are now `segments` (some solved elsewhere): every section closed. When
    /// some have no base of their own and the index wasn't asked, it is to be.
    pub fn reset(&mut self, segments: &[Segment]) {
        let n = conflicts(segments).count();
        let all_own = conflicts(segments).all(|(_, base, _)| base.is_some());
        if !all_own && self.index == IndexBases::NotNeeded {
            self.index = IndexBases::Loading;
            self.asked = false;
        }
        self.open = vec![false; n];
        self.diffs = vec![None; n];
        let found = match &self.index {
            IndexBases::Found(found) if !all_own => match_bases(segments, found),
            _ => vec![None; n],
        };
        self.bases = conflicts(segments)
            .zip(found)
            .map(|((_, own, _), found)| own.map(str::to_owned).or(found))
            .collect();
    }

    /// What the index gave for the bases.
    pub fn set_index(&mut self, index: IndexBases, segments: &[Segment]) {
        self.index = index;
        let open = std::mem::take(&mut self.open);
        self.reset(segments);
        self.open = open;
        self.open.resize(self.diffs.len(), false);
    }

    /// The original of conflict `k`.
    pub fn of(&self, k: usize) -> Original<'_> {
        if let Some(base) = self.bases.get(k).and_then(Option::as_deref) {
            return Original::Text(base);
        }
        match &self.index {
            IndexBases::NotNeeded | IndexBases::Loading => Original::Loading,
            IndexBases::Added => Original::Added,
            IndexBases::Failed(why) => Original::Unknown(why),
            IndexBases::Found(_) => Original::Unknown(
                "this conflict doesn't match the one rebuilt from the index (the file may have \
                 been edited since, or merged with options or a merge driver that can't be \
                 repeated here)",
            ),
        }
    }

    pub fn is_open(&self, k: usize) -> bool {
        self.open.get(k).copied().unwrap_or(false)
    }

    pub fn toggle(&mut self, k: usize) {
        if let Some(open) = self.open.get_mut(k) {
            *open = !*open;
        }
    }

    /// Works out the sides' changes for the open sections whose base is known (once each).
    pub fn prepare(&mut self, segments: &[Segment]) {
        for (k, (ours, _, theirs)) in conflicts(segments).enumerate() {
            if !self.is_open(k) || self.diffs.get(k).is_none_or(Option::is_some) {
                continue;
            }
            if let Some(base) = self.bases.get(k).and_then(Option::as_deref) {
                let worked = Box::new((changes(base, ours), changes(base, theirs)));
                self.diffs[k] = Some(worked);
            }
        }
    }

    /// The changes of (ours, theirs) against conflict `k`'s base, once [`Self::prepare`]d.
    pub fn diffs(&self, k: usize) -> Option<&(Lines, Lines)> {
        self.diffs.get(k)?.as_deref()
    }
}

/// What the index of `repo` gives for the bases of `path`'s conflicts (blocking: in the
/// background).
pub(super) fn load(repo: &std::path::Path, path: &str) -> IndexBases {
    let stages = match crate::git::conflict_stages(repo, path) {
        Ok(stages) => stages,
        Err(e) => return IndexBases::Failed(format!("could not read the index: {e}")),
    };
    if stages.base.is_none() {
        return if stages.ours.is_some() && stages.theirs.is_some() {
            IndexBases::Added
        } else {
            IndexBases::Failed("the index no longer has this file in conflict".to_owned())
        };
    }
    match crate::git::conflict_with_base(repo, path) {
        Ok(text) => IndexBases::Found(super::parse(&text).0),
        Err(e) => IndexBases::Failed(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(lines: &Lines) -> String {
        lines
            .iter()
            .map(|(c, l)| {
                let mark = match c {
                    Change::Same => ' ',
                    Change::Added => '+',
                    Change::Removed => '-',
                };
                format!("{mark}{l}")
            })
            .collect::<Vec<_>>()
            .join("|")
    }

    #[test]
    fn a_side_against_the_base() {
        let base = "check(x)\nvalidate(x)\nsave(x)\n";
        let deleted = changes(base, "check(x)\nsave(x)\n");
        assert_eq!(kinds(&deleted), " check(x)|-validate(x)| save(x)");
        assert_eq!(summary(&deleted), "deleted 1 line");
        let changed = changes(base, "check(x)\nvalidate(x, strict)\nsave(x)\n");
        assert_eq!(
            kinds(&changed),
            " check(x)|-validate(x)|+validate(x, strict)| save(x)"
        );
        assert_eq!(summary(&changed), "inserted 1 line, deleted 1 line");
        assert_eq!(summary(&changes(base, base)), "unchanged");
        assert_eq!(summary(&changes(base, "")), "deleted all 3 lines");
        assert_eq!(summary(&changes("", "ünï\n")), "inserted 1 line");
        assert_eq!(
            summary(&changes("a\n", "b\nc\n")),
            "replaced all 1 line with 2 lines"
        );
        // CRLF against LF: the same lines.
        assert_eq!(summary(&changes("a\r\nb\r\n", "a\nb\n")), "unchanged");
        assert!(changes("", "").is_empty());
    }

    #[test]
    fn huge_sides_are_not_matched_line_by_line() {
        let base: String = (0..3000).map(|i| format!("old {i}\n")).collect();
        let side: String = (0..3000).map(|i| format!("new {i}\n")).collect();
        let lines = changes(&base, &side);
        assert_eq!(lines.len(), 6000);
        assert!(lines[..3000].iter().all(|(c, _)| *c == Change::Removed));
        assert!(lines[3000..].iter().all(|(c, _)| *c == Change::Added));
    }

    #[test]
    fn conflicts_are_matched_with_the_index_in_order() {
        let parse = |text: &str| super::super::parse(text).0;
        let segments = parse(
            "<<<<<<< a\nx\r\n=======\ny\r\n>>>>>>> b\nmid\n<<<<<<< a\nedited\n=======\nq\n>>>>>>> b\n\
             <<<<<<< a\np\n=======\nq\n>>>>>>> b\n",
        );
        let found = parse(
            "<<<<<<< o\nx\n||||||| b\nbase 1\n=======\ny\n>>>>>>> t\nmid\n\
             <<<<<<< o\no\n||||||| b\nbase 2\n=======\nq\n>>>>>>> t\n\
             <<<<<<< o\np\n||||||| b\nbase 3\n=======\nq\n>>>>>>> t\n",
        );
        assert_eq!(
            match_bases(&segments, &found),
            vec![
                Some("base 1\n".to_owned()),
                None,
                Some("base 3\n".to_owned())
            ]
        );
        assert_eq!(match_bases(&segments, &[]), vec![None, None, None]);
    }

    /// git's merge keeps `A } B` / `X } Y` as one conflict; the rebuilt file has two, with the
    /// `}` between them.
    #[test]
    fn one_conflict_matches_a_run_of_rebuilt_ones() {
        let parse = |text: &str| super::super::parse(text).0;
        let segments = parse("<<<<<<< a\nA\n}\nB\n=======\nX\n}\nY\n>>>>>>> b\n");
        let found = parse(
            "<<<<<<< o\nA\n||||||| b\na\n=======\nX\n>>>>>>> t\n}\n\
             <<<<<<< o\nB\n||||||| b\nb\n=======\nY\n>>>>>>> t\n",
        );
        assert_eq!(
            match_bases(&segments, &found),
            vec![Some("a\n}\nb\n".to_owned())]
        );
    }
}
