//! Which files fold and why: from their attributes, their names and the settings' patterns.

use crate::git::FoldAttr;
use crate::settings::Settings;

/// Lockfiles of the usual package managers, by file name.
const LOCKFILES: &[&str] = &[
    "Cargo.lock",
    "package-lock.json",
    "npm-shrinkwrap.json",
    "yarn.lock",
    "pnpm-lock.yaml",
    "bun.lock",
    "bun.lockb",
    "deno.lock",
    "poetry.lock",
    "Pipfile.lock",
    "pdm.lock",
    "uv.lock",
    "go.sum",
    "Gemfile.lock",
    "composer.lock",
    "flake.lock",
    "mix.lock",
    "pubspec.lock",
    "Podfile.lock",
    "Package.resolved",
    "Cartfile.resolved",
    "packages.lock.json",
    "gradle.lockfile",
    "conan.lock",
    "paket.lock",
];

/// Endings of minified files.
const MINIFIED: &[&str] = &[".min.js", ".min.mjs", ".min.cjs", ".min.css"];

/// Why a file starts folded.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(in crate::repo) enum FoldReason {
    /// `linguist-generated` in `.gitattributes`.
    Generated,
    /// `-diff` in `.gitattributes`.
    NoDiff,
    Lockfile,
    Minified,
    /// One of the settings' patterns.
    Pattern,
}

impl FoldReason {
    /// The word that ends the folded line: _+12 −3, lockfile_.
    pub(in crate::repo) fn label(self) -> &'static str {
        match self {
            FoldReason::Generated => "generated",
            FoldReason::NoDiff => "-diff in .gitattributes",
            FoldReason::Lockfile => "lockfile",
            FoldReason::Minified => "minified",
            FoldReason::Pattern => "folded by a pattern in Settings",
        }
    }
}

/// What the settings say about folding, read once for a list or a diff.
#[derive(Clone, Debug, Default, Hash, PartialEq, Eq)]
pub(in crate::repo) struct Rules {
    pub(in crate::repo) on: bool,
    patterns: Vec<String>,
}

impl Rules {
    pub(in crate::repo) fn from_settings(settings: &Settings) -> Self {
        Self::new(settings.fold_generated, &settings.fold_patterns)
    }

    /// `patterns`: separated by commas or white space.
    pub(in crate::repo) fn new(on: bool, patterns: &str) -> Self {
        Self {
            on,
            patterns: patterns
                .split(|c: char| c == ',' || c.is_whitespace())
                .filter(|p| !p.is_empty())
                .map(str::to_owned)
                .collect(),
        }
    }

    /// Why the file at `path` folds, given what its attributes say (`None`: not known yet).
    pub(in crate::repo) fn reason(&self, path: &str, attr: Option<FoldAttr>) -> Option<FoldReason> {
        if !self.on {
            return None;
        }
        match attr {
            Some(FoldAttr::NotGenerated) => return None,
            Some(FoldAttr::Generated) => return Some(FoldReason::Generated),
            Some(FoldAttr::NoDiff) => return Some(FoldReason::NoDiff),
            Some(FoldAttr::Unspecified) | None => {}
        }
        // A path ending in `/` is a folder (a repository nested in the working tree).
        if path.ends_with('/') {
            return None;
        }
        let name = path
            .trim_end_matches('/')
            .rsplit('/')
            .next()
            .unwrap_or(path);
        if LOCKFILES.contains(&name) {
            Some(FoldReason::Lockfile)
        } else if MINIFIED
            .iter()
            .any(|end| name.ends_with(end) && name.len() > end.len())
        {
            Some(FoldReason::Minified)
        } else if self.patterns.iter().any(|p| pattern_matches(p, path, name)) {
            Some(FoldReason::Pattern)
        } else {
            None
        }
    }
}

/// A pattern without a `/` matches the file's name; one with a `/` its path from the top of
/// the repository (a leading `/` ignored), where `*` stays within a folder and `**` doesn't.
fn pattern_matches(pattern: &str, path: &str, name: &str) -> bool {
    match pattern.strip_prefix('/') {
        Some(rooted) => glob(rooted.as_bytes(), path.as_bytes()),
        None if pattern.contains('/') => glob(pattern.as_bytes(), path.as_bytes()),
        None => glob(pattern.as_bytes(), name.as_bytes()),
    }
}

/// `*`, `**` and `?` against `text`, byte by byte (a `?` matches one byte of a character
/// written in several). Each (pattern, text) position is tried once, so many stars over a
/// long name stay quick.
fn glob(pattern: &[u8], text: &[u8]) -> bool {
    let mut failed = std::collections::HashSet::new();
    glob_at(pattern, text, 0, 0, &mut failed)
}

fn glob_at(
    pattern: &[u8],
    text: &[u8],
    p: usize,
    t: usize,
    failed: &mut std::collections::HashSet<(usize, usize)>,
) -> bool {
    if failed.contains(&(p, t)) {
        return false;
    }
    let rest = &text[t..];
    let matched = match &pattern[p..] {
        [] => rest.is_empty(),
        [b'*', b'*', b'/', ..] => (t..=text.len())
            // `**/`: no folder, or any number of whole ones.
            .filter(|&i| i == 0 || text[i - 1] == b'/')
            .any(|i| glob_at(pattern, text, p + 3, i, failed)),
        [b'*', b'*', ..] => (t..=text.len()).any(|i| glob_at(pattern, text, p + 2, i, failed)),
        [b'*', ..] => {
            let within = rest.iter().position(|&b| b == b'/').unwrap_or(rest.len());
            (t..=t + within).any(|i| glob_at(pattern, text, p + 1, i, failed))
        }
        [b'?', ..] => {
            matches!(rest.first(), Some(&c) if c != b'/')
                && glob_at(pattern, text, p + 1, t + 1, failed)
        }
        [c, ..] => rest.first() == Some(c) && glob_at(pattern, text, p + 1, t + 1, failed),
    };
    if !matched {
        failed.insert((p, t));
    }
    matched
}

/// _+1,204 −880, lockfile_: the lines a folded file adds and removes, and why it's folded.
/// Without counts (a binary file, a `-diff` one): only why.
pub(in crate::repo) fn stats_line(
    added: Option<u32>,
    removed: Option<u32>,
    reason: FoldReason,
) -> String {
    match (added, removed) {
        (Some(a), Some(d)) => format!("+{} −{}, {}", thousands(a), thousands(d), reason.label()),
        _ => reason.label().to_owned(),
    }
}

/// `1204` as `1,204`.
pub(in crate::repo) fn thousands(n: u32) -> String {
    let digits = n.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

#[cfg(test)]
pub(super) fn glob_for_tests(pattern: &str, text: &str) -> bool {
    glob(pattern.as_bytes(), text.as_bytes())
}
