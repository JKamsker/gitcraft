//! What a package will publish: the files `cargo package --list` or `npm pack --dry-run` would
//! ship, those of the project left out, those that look shipped by accident, and how they differ
//! from the version already published.

mod archive;
pub(crate) mod display;
pub(crate) mod check;
mod run;
#[cfg(test)]
mod tests;
pub(crate) mod tools;

use std::path::Path;

pub(crate) use check::Note;

/// Whose package it is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Ecosystem {
    Cargo,
    Npm,
}

impl Ecosystem {
    pub(crate) fn tool(self) -> &'static str {
        match self {
            Ecosystem::Cargo => "cargo package --list",
            Ecosystem::Npm => "npm pack --dry-run",
        }
    }

    pub(crate) fn registry(self) -> &'static str {
        match self {
            Ecosystem::Cargo => "crates.io",
            Ecosystem::Npm => "npm",
        }
    }

    fn manifest(self) -> &'static str {
        match self {
            Ecosystem::Cargo => "Cargo.toml",
            Ecosystem::Npm => "package.json",
        }
    }
}

/// A package of the repository.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Package {
    pub ecosystem: Ecosystem,
    pub name: String,
    pub version: String,
    /// Its folder in the repository (empty: the top).
    pub dir: String,
    /// The registry it is published to when it isn't crates.io's or npm's own: then it is not
    /// compared.
    pub registry: Option<String>,
}

/// The version published, and how the package differs from it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Published {
    pub version: String,
    pub added: Vec<String>,
    pub gone: Vec<String>,
}

/// What the panel shows of a package.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Report {
    pub files: Vec<(String, Option<u64>)>,
    pub missing: Vec<Note>,
    pub accidental: Vec<Note>,
    /// What the listing can't account for (npm's build scripts it doesn't run).
    pub hint: Option<String>,
}

/// The packages the repository has: those of the manifests at its top.
pub(crate) fn packages(repo: &Path) -> Result<Vec<Package>, String> {
    let has = |file: &str| crate::remote::read_to_string(&crate::remote::join(repo, file)).is_ok();
    let (cargo, npm) = (has("Cargo.toml"), has("package.json"));
    if !cargo && !npm {
        return Err("No Cargo.toml or package.json at the top of the repository".to_owned());
    }
    tools::packages(repo, cargo, npm)
}

/// Works out the report of `pkg`, from its tool and git (the registry is asked apart:
/// [`published`]).
pub(crate) fn report(repo: &Path, pkg: &Package) -> Result<Report, String> {
    let files = tools::files(repo, pkg)?;
    let tracked = tracked(repo, &pkg.dir)?;
    let manifest_path = if pkg.dir.is_empty() {
        pkg.ecosystem.manifest().to_owned()
    } else {
        format!("{}/{}", pkg.dir, pkg.ecosystem.manifest())
    };
    let manifest = crate::remote::read_to_string(&crate::remote::join(repo, &manifest_path))
        .unwrap_or_default();
    Ok(Report {
        missing: check::missing(pkg.ecosystem, &files, &tracked, &manifest),
        accidental: check::accidental(pkg.ecosystem, &files, &tracked),
        hint: check::hint(pkg.ecosystem, &manifest),
        files,
    })
}

/// How `files` (of `pkg`) differ from the version on the registry; `Ok(None)`: never
/// published there.
pub(crate) fn published(
    pkg: &Package,
    files: &[(String, Option<u64>)],
) -> Result<Option<Published>, String> {
    if let Some(registry) = &pkg.registry {
        return Err(format!(
            "published to {registry}, not {}: not compared",
            pkg.ecosystem.registry()
        ));
    }
    let Some((version, url)) = tools::published(pkg)? else {
        return Ok(None);
    };
    let before = tools::published_files(&url)?;
    let (added, gone) = check::compare(pkg.ecosystem, files, &before);
    Ok(Some(Published {
        version,
        added,
        gone,
    }))
}

/// The files git tracks in `dir` (and below), relative to it.
fn tracked(repo: &Path, dir: &str) -> Result<Vec<String>, String> {
    let mut args = vec!["ls-files", "-z", "--full-name", "--"];
    let spec = if dir.is_empty() {
        ".".to_owned()
    } else {
        format!(":(top){dir}")
    };
    args.push(&spec);
    let out = crate::git::run(Some(repo), &args)?;
    let prefix = if dir.is_empty() {
        String::new()
    } else {
        format!("{dir}/")
    };
    Ok(out
        .split('\0')
        .filter(|p| !p.is_empty())
        .filter_map(|p| p.strip_prefix(&prefix).map(str::to_owned))
        .collect())
}
