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

/// One package pinned, and the line it is pinned on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pkg {
    pub name: String,
    pub version: String,
    pub license: Option<String>,
    pub line: u32,
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
                "PyPI's release history",
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
pub fn packages(file: &str, text: &str) -> Result<Vec<Pkg>, String> {
    match file {
        "Cargo.lock" | "poetry.lock" | "uv.lock" => Ok(toml_packages(&toml::parse(text)?)),
        "package-lock.json" | "npm-shrinkwrap.json" => Ok(npm_packages(&json::parse(text)?)),
        "pnpm-lock.yaml" => Ok(pnpm_packages(&yaml::parse(text)?)),
        "composer.lock" => Ok(composer_packages(&json::parse(text)?)),
        "go.sum" => Ok(go_packages(text)),
        _ => Err("not a lockfile this reads".to_owned()),
    }
}

fn get<'a>(node: &'a Node, key: &str) -> Option<&'a Field> {
    match node {
        Node::Map(fields) => fields.iter().find(|f| f.key == key),
        _ => None,
    }
}

fn text<'a>(node: &'a Node, key: &str) -> Option<&'a str> {
    match &get(node, key)?.node {
        Node::Scalar { text, .. } => Some(text),
        _ => None,
    }
}

/// A licence as written: a string, or a list of them (Composer), or `{type: …}` (old npm).
fn license(node: &Node, key: &str) -> Option<String> {
    match &get(node, key)?.node {
        Node::Scalar { text, .. } => Some(text.clone()),
        Node::List(items) => Some(
            items
                .iter()
                .filter_map(|i| match &i.node {
                    Node::Scalar { text, .. } => Some(text.as_str()),
                    other => text(other, "type"),
                })
                .collect::<Vec<_>>()
                .join(" OR "),
        ),
        other => text(other, "type").map(str::to_owned),
    }
}

/// `[[package]]` tables with `name` and `version` (Cargo, Poetry, uv).
fn toml_packages(root: &Node) -> Vec<Pkg> {
    let Some(Field {
        node: Node::List(items),
        ..
    }) = get(root, "package")
    else {
        return Vec::new();
    };
    items
        .iter()
        .filter_map(|i| {
            Some(Pkg {
                name: text(&i.node, "name")?.to_owned(),
                version: text(&i.node, "version")?.to_owned(),
                license: None,
                line: i.line,
            })
        })
        .collect()
}

/// npm's lockfile: `packages` keyed by their folder (v2, v3), or nested `dependencies` (v1).
fn npm_packages(root: &Node) -> Vec<Pkg> {
    if let Some(Field {
        node: Node::Map(fields),
        ..
    }) = get(root, "packages")
    {
        return fields
            .iter()
            .filter(|f| !f.key.is_empty())
            .filter_map(|f| {
                // `node_modules/a/node_modules/@b/c` is `@b/c`.
                let name = f
                    .key
                    .rsplit_once("node_modules/")
                    .map_or(f.key.as_str(), |(_, n)| n);
                let name = text(&f.node, "name").unwrap_or(name);
                Some(Pkg {
                    name: name.to_owned(),
                    version: text(&f.node, "version")?.to_owned(),
                    license: license(&f.node, "license"),
                    line: f.line,
                })
            })
            .collect();
    }
    let mut out = Vec::new();
    npm_v1(root, &mut out, 0);
    out
}

fn npm_v1(node: &Node, out: &mut Vec<Pkg>, depth: usize) {
    let Some(Field {
        node: Node::Map(fields),
        ..
    }) = get(node, "dependencies")
    else {
        return;
    };
    if depth > 64 {
        return;
    }
    for f in fields {
        if let Some(version) = text(&f.node, "version") {
            out.push(Pkg {
                name: f.key.clone(),
                version: version.to_owned(),
                license: None,
                line: f.line,
            });
        }
        npm_v1(&f.node, out, depth + 1);
    }
}

/// pnpm's lockfile: `packages` keyed `name@version` (v9), `/name@version` or `/name/version`.
fn pnpm_packages(root: &Node) -> Vec<Pkg> {
    let Some(Field {
        node: Node::Map(fields),
        ..
    }) = get(root, "packages")
    else {
        return Vec::new();
    };
    fields
        .iter()
        .filter_map(|f| {
            let key = f.key.trim_start_matches('/');
            // `(react@18.2.0)` after the version names the peers it was resolved with.
            let key = key.split_once('(').map_or(key, |(k, _)| k);
            // The last `@` past the first character: `@scope/name@1.0.0`.
            let (name, version) = match key.rfind('@').filter(|&at| at > 0) {
                Some(at) => (&key[..at], &key[at + 1..]),
                None => key.rsplit_once('/')?,
            };
            if name.is_empty() || version.is_empty() {
                return None;
            }
            Some(Pkg {
                name: name.to_owned(),
                version: version.to_owned(),
                license: None,
                line: f.line,
            })
        })
        .collect()
}

/// Composer's lockfile: `packages` and `packages-dev`, lists of `name` / `version`.
fn composer_packages(root: &Node) -> Vec<Pkg> {
    let mut out = Vec::new();
    for key in ["packages", "packages-dev"] {
        let Some(Field {
            node: Node::List(items),
            ..
        }) = get(root, key)
        else {
            continue;
        };
        out.extend(items.iter().filter_map(|i| {
            Some(Pkg {
                name: text(&i.node, "name")?.to_owned(),
                version: text(&i.node, "version")?.to_owned(),
                license: license(&i.node, "license"),
                line: i.line,
            })
        }));
    }
    out
}

/// `go.sum`: `module version hash` lines; the `/go.mod` line of a version is the same version.
fn go_packages(text: &str) -> Vec<Pkg> {
    let mut out: Vec<Pkg> = Vec::new();
    for (n, line) in text.lines().enumerate() {
        let mut parts = line.split_whitespace();
        let (Some(name), Some(version)) = (parts.next(), parts.next()) else {
            continue;
        };
        let version = version.trim_end_matches("/go.mod");
        if out
            .iter()
            .rev()
            .take(4)
            .any(|p| p.name == name && p.version == version)
        {
            continue;
        }
        out.push(Pkg {
            name: name.to_owned(),
            version: version.to_owned(),
            license: None,
            line: n as u32 + 1,
        });
    }
    out
}

/// Compares two versions part by part, numbers as numbers (`1.10` after `1.9`).
pub fn version_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    let parts = |v: &str| -> Vec<String> {
        v.trim_start_matches('v')
            .split(['.', '-', '+'])
            .map(str::to_owned)
            .collect()
    };
    let (pa, pb) = (parts(a), parts(b));
    for (x, y) in pa.iter().zip(&pb) {
        let ord = match (x.parse::<u64>(), y.parse::<u64>()) {
            (Ok(x), Ok(y)) => x.cmp(&y),
            _ => x.cmp(y),
        };
        if ord.is_ne() {
            return ord;
        }
    }
    pa.len().cmp(&pb.len())
}

/// Compares the packages pinned on each side of the lockfile `path`.
pub fn diff(path: &str, sides: &Sides) -> Result<DepsDiff, String> {
    let (file, ecosystem) = lockfile(path).ok_or("not a lockfile this reads")?;
    let read = |text: &Option<String>, side: &str| -> Result<Vec<Pkg>, String> {
        text.as_deref()
            .map(|t| {
                packages(file, t)
                    .map_err(|e| format!("the {file} {side} the change isn't valid ({e})"))
            })
            .transpose()
            .map(Option::unwrap_or_default)
    };
    let (old, new) = (read(&sides.old, "before")?, read(&sides.new, "after")?);
    Ok(DepsDiff {
        ecosystem,
        file,
        changes: compare(&old, &new),
    })
}

/// Each package's versions before and after, as changes.
pub fn compare(old: &[Pkg], new: &[Pkg]) -> Vec<DepChange> {
    let mut by_name: BTreeMap<&str, (Vec<&Pkg>, Vec<&Pkg>)> = BTreeMap::new();
    for p in old {
        by_name.entry(&p.name).or_default().0.push(p);
    }
    for p in new {
        by_name.entry(&p.name).or_default().1.push(p);
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
    }
}
