//! Dependency changes: a lockfile before and after the change, read as the packages it pins,
//! and compared as packages upgraded, downgraded, added and removed, with the licence when the
//! lockfile records it (npm, Composer) and where to read about each package.

use std::collections::BTreeMap;

use super::tree::{Field, Node};
use super::{Sides, json, toml, yaml};

/// Where the packages of a lockfile come from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ecosystem {
    Cargo,
    Npm,
    PyPi,
    Composer,
    Go,
}

/// A package as its registry knows it: (name, version); `None` for one of the project's own.
pub type Registry = Option<(String, String)>;

/// One package pinned, and the line it is pinned on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pkg {
    pub name: String,
    pub version: String,
    pub license: Option<String>,
    pub line: u32,
    /// The package as its registry knows it (name, version); `None` for one of the project's
    /// own (a workspace member, a path dependency), which no registry has.
    pub registry: Registry,
    /// Only a lockfile-recorded public registry permits a metadata request.
    pub public_registry: bool,
}

impl Pkg {
    fn new(name: &str, version: &str, license: Option<String>, line: u32) -> Self {
        Pkg {
            name: name.to_owned(),
            version: version.to_owned(),
            license,
            line,
            registry: Some((name.to_owned(), version.to_owned())),
            public_registry: false,
        }
    }

    /// One of the project's own: no registry page.
    fn local(mut self) -> Self {
        self.registry = None;
        self
    }
}

/// What happened to a package.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Change {
    Upgraded,
    Downgraded,
    Added,
    Removed,
    /// Its version was kept; its licence wasn't.
    Relicensed,
}

impl Change {
    pub(super) fn label(self) -> &'static str {
        crate::i18n::text(match self {
            Self::Upgraded => "repo-core-change-upgraded",
            Self::Downgraded => "repo-core-change-downgraded",
            Self::Added => "repo-core-change-added",
            Self::Removed => "repo-core-change-removed",
            Self::Relicensed => "repo-core-change-relicensed",
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DepChange {
    pub change: Change,
    pub name: String,
    pub old: Option<String>,
    pub new: Option<String>,
    /// The licence before and after, when it changed (or of a package added).
    pub license: Option<(Option<String>, Option<String>)>,
    pub old_line: Option<u32>,
    pub new_line: Option<u32>,
    /// The package on its registry before and after, for the links.
    pub registry: (Registry, Registry),
    pub metadata_registry: (Registry, Registry),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DepsDiff {
    pub ecosystem: Ecosystem,
    /// `Cargo.lock`, `package-lock.json`…
    pub file: &'static str,
    pub changes: Vec<DepChange>,
}

/// The lockfiles read, by name.
const FILES: &[(&str, Ecosystem)] = &[
    ("Cargo.lock", Ecosystem::Cargo),
    ("package-lock.json", Ecosystem::Npm),
    ("npm-shrinkwrap.json", Ecosystem::Npm),
    ("pnpm-lock.yaml", Ecosystem::Npm),
    ("poetry.lock", Ecosystem::PyPi),
    ("uv.lock", Ecosystem::PyPi),
    ("composer.lock", Ecosystem::Composer),
    ("go.sum", Ecosystem::Go),
];

/// The lockfile `path` is, if it is one this reads.
pub fn lockfile(path: &str) -> Option<(&'static str, Ecosystem)> {
    let name = path.rsplit('/').next().unwrap_or(path);
    FILES.iter().find(|(f, _)| *f == name).copied()
}

impl Ecosystem {
    /// The package's page on its registry, at `version`.
    pub fn registry(self, name: &str, version: &str) -> (String, &'static str) {
        match self {
            Ecosystem::Cargo => (
                format!("https://crates.io/crates/{name}/{version}"),
                "crates.io",
            ),
            Ecosystem::Npm => (
                format!("https://www.npmjs.com/package/{name}/v/{version}"),
                "npm",
            ),
            Ecosystem::PyPi => (
                format!("https://pypi.org/project/{name}/{version}/"),
                "PyPI",
            ),
            Ecosystem::Composer => (
                format!("https://packagist.org/packages/{name}#{version}"),
                "Packagist",
            ),
            Ecosystem::Go => (format!("https://pkg.go.dev/{name}@{version}"), "pkg.go.dev"),
        }
    }

    /// Where to read what changed between two versions: the code compared for crates and npm
    /// packages, the release history elsewhere.
    pub fn changes(self, name: &str, old: &str, new: &str) -> (String, &'static str) {
        match self {
            Ecosystem::Cargo => (format!("https://diff.rs/{name}/{old}/{new}"), "diff.rs"),
            Ecosystem::Npm => (
                format!("https://npmdiff.dev/{name}/{old}/{new}/"),
                "npmdiff.dev",
            ),
            Ecosystem::PyPi => (
                format!("https://pypi.org/project/{name}/#history"),
                crate::i18n::text("repo-core-2106"),
            ),
            Ecosystem::Composer => (
                format!("https://packagist.org/packages/{name}"),
                "Packagist",
            ),
            Ecosystem::Go => (
                format!("https://pkg.go.dev/{name}?tab=versions"),
                "pkg.go.dev",
            ),
        }
    }
}

/// The packages a lockfile named `file` pins; `Err` says why it can't be read.
pub fn version_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    use std::cmp::Ordering::*;
    // `1.2.3-rc.1+build` → release `1.2.3`, pre-release `rc.1` (the build is left out).
    let split = |v: &str| -> (Vec<String>, String) {
        let v = v.trim_start_matches('v');
        let v = v.split_once('+').map_or(v, |(v, _)| v);
        let (release, pre) = v.split_once('-').unwrap_or((v, ""));
        let mut parts: Vec<String> = release.split('.').map(str::to_owned).collect();
        let mut pre = pre.to_owned();
        // `2.0rc1`: letters in the last part start a pre-release.
        if pre.is_empty()
            && let Some(last) = parts.last_mut()
            && let Some(at) = last.find(|c: char| c.is_ascii_alphabetic())
            && at > 0
        {
            pre = last[at..].to_owned();
            last.truncate(at);
        }
        (parts, pre)
    };
    let part = |x: &str, y: &str| match (x.parse::<u64>(), y.parse::<u64>()) {
        (Ok(x), Ok(y)) => x.cmp(&y),
        (Ok(_), Err(_)) => Less,
        (Err(_), Ok(_)) => Greater,
        _ => x.cmp(y),
    };
    let ((ra, pa), (rb, pb)) = (split(a), split(b));
    for k in 0..ra.len().max(rb.len()) {
        let ord = part(
            ra.get(k).map_or("0", String::as_str),
            rb.get(k).map_or("0", String::as_str),
        );
        if ord.is_ne() {
            return ord;
        }
    }
    match (pa.is_empty(), pb.is_empty()) {
        (true, true) => Equal,
        (true, false) => Greater,
        (false, true) => Less,
        _ => {
            let (xa, xb): (Vec<&str>, Vec<&str>) =
                (pa.split('.').collect(), pb.split('.').collect());
            for (x, y) in xa.iter().zip(&xb) {
                let ord = part(x, y);
                if ord.is_ne() {
                    return ord;
                }
            }
            xa.len().cmp(&xb.len())
        }
    }
}

/// Compares the packages pinned on each side of the lockfile `path`.
pub fn diff(path: &str, sides: &Sides) -> Result<DepsDiff, String> {
    let (file, ecosystem) = lockfile(path).ok_or(crate::i18n::text("repo-core-2109"))?;
    let read = |text: &Option<String>, side: &str| -> Result<Vec<Pkg>, String> {
        text.as_deref()
            .map(|t| {
                packages(file, t).map_err(|e| {
                    crate::i18n::message(
                        "repo-core-2138",
                        &[
                            ("file", file.to_string().into()),
                            ("side", side.to_string().into()),
                            ("e", e.to_string().into()),
                        ],
                    )
                })
            })
            .transpose()
            .map(Option::unwrap_or_default)
    };
    let (old, new) = (
        read(&sides.old, crate::i18n::text("repo-core-2049"))?,
        read(&sides.new, crate::i18n::text("repo-core-2050"))?,
    );
    Ok(DepsDiff {
        ecosystem,
        file,
        changes: compare(&old, &new),
    })
}

/// Each package's versions before and after, as changes.
pub fn compare(old: &[Pkg], new: &[Pkg]) -> Vec<DepChange> {
    // A version installed in several places (npm's nested copies, pnpm's peer variants) is
    // one version of the package.
    let mut by_name: BTreeMap<&str, (Vec<&Pkg>, Vec<&Pkg>)> = BTreeMap::new();
    for p in old {
        let side = &mut by_name.entry(&p.name).or_default().0;
        if !side.iter().any(|q| q.version == p.version) {
            side.push(p);
        }
    }
    for p in new {
        let side = &mut by_name.entry(&p.name).or_default().1;
        if !side.iter().any(|q| q.version == p.version) {
            side.push(p);
        }
    }
    let mut out = Vec::new();
    for (name, (olds, news)) in by_name {
        // A version on both sides is kept (its licence may have changed).
        let gone: Vec<&Pkg> = olds
            .iter()
            .filter(|o| !news.iter().any(|n| n.version == o.version))
            .copied()
            .collect();
        let came: Vec<&Pkg> = news
            .iter()
            .filter(|n| !olds.iter().any(|o| o.version == n.version))
            .copied()
            .collect();
        for o in &olds {
            if let Some(n) = news.iter().find(|n| n.version == o.version)
                && o.license != n.license
                && o.license.is_some()
            {
                out.push(change(Change::Relicensed, name, Some(o), Some(n)));
            }
        }
        // One version for another: upgraded or downgraded. Several: each added or removed.
        if gone.len() == 1 && came.len() == 1 {
            let (o, n) = (gone[0], came[0]);
            let kind = if version_cmp(&n.version, &o.version).is_lt() {
                Change::Downgraded
            } else {
                Change::Upgraded
            };
            out.push(change(kind, name, Some(o), Some(n)));
            continue;
        }
        out.extend(
            gone.iter()
                .map(|o| change(Change::Removed, name, Some(o), None)),
        );
        out.extend(
            came.iter()
                .map(|n| change(Change::Added, name, None, Some(n))),
        );
    }
    out.sort_by_key(|c| c.change);
    out
}

fn change(kind: Change, name: &str, old: Option<&Pkg>, new: Option<&Pkg>) -> DepChange {
    let (ol, nl) = (
        old.and_then(|p| p.license.clone()),
        new.and_then(|p| p.license.clone()),
    );
    let license = match kind {
        Change::Added => nl.clone().map(|l| (None, Some(l))),
        _ if old.is_some() && new.is_some() && ol != nl && (ol.is_some() || nl.is_some()) => {
            Some((ol, nl))
        }
        _ => None,
    };
    DepChange {
        change: kind,
        name: name.to_owned(),
        old: old.map(|p| p.version.clone()),
        new: new.map(|p| p.version.clone()),
        license,
        old_line: old.map(|p| p.line),
        new_line: new.map(|p| p.line),
        metadata_registry: (
            old.filter(|p| p.public_registry)
                .and_then(|p| p.registry.clone()),
            new.filter(|p| p.public_registry)
                .and_then(|p| p.registry.clone()),
        ),
        registry: (
            old.and_then(|p| p.registry.clone()),
            new.and_then(|p| p.registry.clone()),
        ),
    }
}

mod parse;

pub use parse::packages;
