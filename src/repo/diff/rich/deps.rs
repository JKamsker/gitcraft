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
}

impl Pkg {
    fn new(name: &str, version: &str, license: Option<String>, line: u32) -> Self {
        Pkg {
            name: name.to_owned(),
            version: version.to_owned(),
            license,
            line,
            registry: Some((name.to_owned(), version.to_owned())),
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
            let (name, version) = (text(&i.node, "name")?, text(&i.node, "version")?);
            // Cargo: a git source pins a commit (shown, so a bump of it is seen); no source
            // at all is a workspace member or path dependency. uv: an editable or virtual
            // source is the project itself.
            let source = text(&i.node, "source");
            let git = source
                .filter(|s| s.starts_with("git+"))
                .and_then(|s| s.rsplit_once('#'))
                .map(|(_, rev)| rev.get(..7).unwrap_or(rev));
            let shown = match git {
                Some(rev) => format!("{version} (git {rev})"),
                None => version.to_owned(),
            };
            let pkg = Pkg::new(name, &shown, None, i.line);
            let uv_local = matches!(
                get(&i.node, "source").map(|f| &f.node),
                Some(Node::Map(m)) if m.iter().any(|f| matches!(f.key.as_str(), "editable" | "virtual" | "directory" | "path"))
            );
            let cargo_local = source.is_none() && get(&i.node, "checksum").is_none() && get(&i.node, "source").is_none();
            let pkg = if git.is_some() || uv_local || (cargo_local && !is_poetry(root)) {
                pkg.local()
            } else {
                pkg
            };
            Some(Pkg {
                registry: pkg.registry.map(|(n, _)| (n, version.to_owned())),
                ..pkg
            })
        })
        .collect()
}

/// Whether a `[[package]]` lockfile is Poetry's, whose packages name no source.
fn is_poetry(root: &Node) -> bool {
    get(root, "metadata").is_some_and(|m| get(&m.node, "content-hash").is_some())
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
                let pkg = Pkg::new(
                    name,
                    text(&f.node, "version")?,
                    license(&f.node, "license"),
                    f.line,
                );
                // A workspace (not under `node_modules`) or a link is the project's own.
                let local =
                    !f.key.contains("node_modules/") || text(&f.node, "link") == Some("true");
                Some(if local { pkg.local() } else { pkg })
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
            let mut pkg = Pkg::new(&f.key, version, None, f.line);
            // An alias, `npm:other@1.2.0`: the registry has `other`.
            if let Some((real, v)) = version.strip_prefix("npm:").and_then(|a| {
                a.rfind('@')
                    .filter(|&at| at > 0)
                    .map(|at| (&a[..at], &a[at + 1..]))
            }) {
                pkg.registry = Some((real.to_owned(), v.to_owned()));
            }
            if version.starts_with("file:") || version.starts_with("link:") {
                pkg = pkg.local();
            }
            out.push(pkg);
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
            // pnpm 5 and older: `name/1.0.0`, peers after `_` (`name/1.0.0_react@18.2.0`).
            let old_style = key
                .rsplit_once('/')
                .filter(|(_, v)| v.starts_with(|c: char| c.is_ascii_digit()))
                .or_else(|| {
                    let (k, _) = key.split_once('_')?;
                    k.rsplit_once('/')
                        .filter(|(_, v)| v.starts_with(|c: char| c.is_ascii_digit()))
                });
            // Newer: the last `@` past the first character, `@scope/name@1.0.0`.
            let (name, version) = match old_style {
                Some((name, version)) => (name, version.split('_').next().unwrap_or(version)),
                None => {
                    let at = key.rfind('@').filter(|&at| at > 0)?;
                    (&key[..at], &key[at + 1..])
                }
            };
            if name.is_empty() || version.is_empty() {
                return None;
            }
            Some(Pkg::new(name, version, None, f.line))
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
            let (name, version) = (text(&i.node, "name")?, text(&i.node, "version")?);
            // A branch (`dev-main`) is pinned by its commit: shown, so a bump of it is seen.
            let reference = get(&i.node, "dist")
                .or_else(|| get(&i.node, "source"))
                .and_then(|d| text(&d.node, "reference"))
                .filter(|_| version.starts_with("dev-"));
            let shown = match reference {
                Some(r) => format!("{version} ({})", r.get(..7).unwrap_or(r)),
                None => version.to_owned(),
            };
            let mut pkg = Pkg::new(name, &shown, license(&i.node, "license"), i.line);
            pkg.registry = Some((name.to_owned(), version.to_owned()));
            Some(pkg)
        }));
    }
    out
}

/// `go.sum`: `module version hash` lines. It keeps the `/go.mod` lines of versions no longer
/// used, so each module is read as its newest version there.
fn go_packages(text: &str) -> Vec<Pkg> {
    let mut newest: Vec<Pkg> = Vec::new();
    let mut at: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for (n, line) in text.lines().enumerate() {
        let mut parts = line.split_whitespace();
        let (Some(name), Some(version)) = (parts.next(), parts.next()) else {
            continue;
        };
        let version = version.trim_end_matches("/go.mod");
        let pkg = Pkg::new(name, version, None, n as u32 + 1);
        match at.get(name) {
            Some(&k) if version_cmp(version, &newest[k].version).is_gt() => newest[k] = pkg,
            Some(_) => {}
            None => {
                at.insert(name.to_owned(), newest.len());
                newest.push(pkg);
            }
        }
    }
    newest
}

/// Compares two versions part by part, numbers as numbers (`1.10` after `1.9`); a
/// pre-release comes before its release (`1.0.0-rc.1` before `1.0.0`, `2.0rc1` before `2.0`).
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
        registry: (
            old.and_then(|p| p.registry.clone()),
            new.and_then(|p| p.registry.clone()),
        ),
    }
}
