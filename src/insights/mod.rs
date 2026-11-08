//! What the insights views are made of (issue #175), apart from their drawing: each reads Git or
//! the forge in the background and keeps where its data came from and how complete it is, so
//! the view can say so rather than seem surer than it is.
//!
//! Every analysis here is read-only: none changes the repository, the forge or the CI.

// Shared parts land before the views using them; dropped once every view is in (#175).
#![allow(dead_code)]

pub mod coupling;
pub mod history;

/// Where what a view shows came from, and how far it can be trusted: shown above every view.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Provenance {
    /// What was read: `git log`, `GitHub Actions`, `merge-tree`…
    pub source: String,
    /// What it covers: a range, a branch, the last commits loaded.
    pub scope: String,
    /// The commits it was read at (name and full id), so it can be told stale once they move.
    pub tips: Vec<(String, String)>,
    /// When it was read (seconds since the epoch).
    pub collected: i64,
    /// Why it is incomplete (a history limit reached, a forge that answered only in part), when
    /// it is.
    pub partial: Option<String>,
    /// Why it no longer matches the repository (a tip moved since), when it doesn't.
    pub stale: Option<String>,
}

impl Provenance {
    pub fn new(source: impl Into<String>, scope: impl Into<String>) -> Self {
        Self {
            source: source.into(),
            scope: scope.into(),
            collected: crate::git::now(),
            ..Self::default()
        }
    }

    pub fn tip(mut self, name: impl Into<String>, sha: impl Into<String>) -> Self {
        self.tips.push((name.into(), sha.into()));
        self
    }

    pub fn partial(mut self, why: impl Into<String>) -> Self {
        self.partial = Some(why.into());
        self
    }

    /// Marks it stale when any of its tips is now elsewhere: `current` says where a tip's name
    /// points now (`None`: it is gone).
    pub fn check_tips(&mut self, current: impl Fn(&str) -> Option<String>) {
        let moved: Vec<String> = self
            .tips
            .iter()
            .filter(|(name, sha)| current(name).as_deref() != Some(sha.as_str()))
            .map(|(name, _)| name.clone())
            .collect();
        self.stale = (!moved.is_empty()).then(|| format!("{} moved since", moved.join(", ")));
    }
}

/// How something shown is known: read from Git or the forge, worked out from what was read, or
/// not known at all. Views draw each differently, never by colour alone.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Evidence {
    /// Git or the forge says so.
    Observed,
    /// Worked out (a name match, a timing, a co-change): a hint, not a fact.
    Inferred,
    /// Nothing says either way.
    Unknown,
}

impl Evidence {
    pub fn label(self) -> &'static str {
        match self {
            Evidence::Observed => "observed",
            Evidence::Inferred => "inferred",
            Evidence::Unknown => "unknown",
        }
    }
}

/// `path`'s folder at most `depth` levels down (`src/repo/x.rs` at 1: `src`), or the root's
/// files as `""`. Depth 0 groups everything together.
pub fn folder_at(path: &str, depth: usize) -> &str {
    if depth == 0 {
        return "";
    }
    let mut end = 0;
    for (n, (i, _)) in path.match_indices('/').enumerate() {
        end = i;
        if n + 1 == depth {
            break;
        }
    }
    &path[..end]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folders_are_cut_at_their_depth() {
        assert_eq!(folder_at("src/repo/x.rs", 1), "src");
        assert_eq!(folder_at("src/repo/x.rs", 2), "src/repo");
        assert_eq!(folder_at("src/repo/x.rs", 5), "src/repo");
        assert_eq!(folder_at("README.md", 2), "");
        assert_eq!(folder_at("src/x.rs", 0), "");
    }

    #[test]
    fn provenance_is_stale_once_a_tip_moves() {
        let mut p = Provenance::new("git log", "HEAD").tip("main", "abc");
        p.check_tips(|_| Some("abc".to_owned()));
        assert_eq!(p.stale, None);
        p.check_tips(|_| Some("def".to_owned()));
        assert_eq!(p.stale.as_deref(), Some("main moved since"));
        p.check_tips(|_| None);
        assert!(p.stale.is_some());
    }
}
