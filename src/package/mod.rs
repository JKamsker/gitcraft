//! What a package will publish: the files `cargo package --list` or `npm pack --dry-run` would
//! ship, those of the project left out, those that look shipped by accident, and how they differ
//! from the version already published.

mod archive;
pub(crate) mod check;
pub(crate) mod display;
mod run;
#[cfg(test)]
mod tests;
pub(crate) mod tools;
pub(crate) mod witness;
pub(crate) use witness::Failure;

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
#[cfg(test)]
pub(crate) fn packages(repo: &Path) -> Result<Vec<Package>, String> {
    packages_witnessed(repo).map_err(Failure::into_raw)
}

pub(crate) fn packages_witnessed(repo: &Path) -> Result<Vec<Package>, Failure> {
    let has = |file: &str| crate::remote::read_to_string(&crate::remote::join(repo, file)).is_ok();
    let (cargo, npm) = (has("Cargo.toml"), has("package.json"));
    require_manifests(cargo, npm)?;
    tools::packages_witnessed(repo, cargo, npm)
}

/// Works out the report of `pkg`, from its tool and git (the registry is asked apart:
/// [`published_witnessed`]).
#[cfg(test)]
pub(crate) fn report(repo: &Path, pkg: &Package) -> Result<Report, String> {
    report_witnessed(repo, pkg).map_err(Failure::into_raw)
}

pub(crate) fn report_witnessed(repo: &Path, pkg: &Package) -> Result<Report, Failure> {
    let files = tools::files_witnessed(repo, pkg)?;
    let tracked = tracked(repo, &pkg.dir).map_err(Failure::Opaque)?;
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
pub(crate) fn published_witnessed(
    pkg: &Package,
    files: &[(String, Option<u64>)],
) -> Result<Option<Published>, Failure> {
    check_registry(pkg)?;
    let Some((version, url)) = tools::published_witnessed(pkg)? else {
        return Ok(None);
    };
    let before = tools::published_files_witnessed(&url)?;
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

// Pure source guards shared by the actual entry points and capture-path tests.
fn require_manifests(cargo: bool, npm: bool) -> Result<(), Failure> {
    if !cargo && !npm {
        return Err(Failure::NoManifest {
            raw: "No Cargo.toml or package.json at the top of the repository".to_owned(),
        });
    }
    Ok(())
}

fn check_registry(pkg: &Package) -> Result<(), Failure> {
    if let Some(registry) = &pkg.registry {
        return Err(Failure::OtherRegistry {
            raw: format!(
                "published to {registry}, not {}: not compared",
                pkg.ecosystem.registry()
            ),
            registry: registry.clone(),
            ecosystem: pkg.ecosystem,
        });
    }
    Ok(())
}

#[cfg(test)]
mod witness_tests {
    use super::*;

    #[test]
    fn actual_source_guards_capture_owned_witnesses_without_discovery() -> Result<(), String> {
        let missing = require_manifests(false, false)
            .err()
            .ok_or("no manifests")?;
        assert!(require_manifests(true, false).is_ok());
        assert!(require_manifests(false, true).is_ok());
        let pkg = Package {
            ecosystem: Ecosystem::Cargo,
            name: "opaque".into(),
            version: "1234.5".into(),
            dir: "ref/main".into(),
            registry: Some("https://example/1234.5; ;".into()),
        };
        let registry = check_registry(&pkg).err().ok_or("custom registry")?;
        let raw = "published to https://example/1234.5; ;, not crates.io: not compared";
        assert_eq!(registry.raw(), raw);
        for locale in ["en-US", "de", "en-US"] {
            crate::i18n::set_language(locale);
            assert_eq!(missing.display() == missing.raw(), locale == "en-US");
            assert_eq!(registry.display() == registry.raw(), locale == "en-US");
            assert!(registry.display().contains("https://example/1234.5; ;"));
            assert_eq!(registry.clone().into_raw(), raw);
        }

        Ok(())
    }
}
