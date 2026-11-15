//! Exact Git membership wins over candidate directory names.
#[cfg(any(target_os = "linux", test))]
use super::Category;
use std::collections::BTreeSet;
#[cfg(any(target_os = "linux", test))]
use std::path::Path;
use std::path::{Component, PathBuf};

#[derive(Default)]
pub(super) struct Membership {
    pub tracked: BTreeSet<PathBuf>,
    pub untracked: BTreeSet<PathBuf>,
    pub ignored: BTreeSet<PathBuf>,
}
impl Membership {
    #[cfg(any(target_os = "linux", test))]
    pub fn category(&self, path: &Path, cargo: bool) -> Category {
        if self.tracked.contains(path) {
            Category::Tracked
        } else if self.untracked.contains(path) {
            Category::Untracked
        } else if self.ignored.contains(path) {
            if cargo && artifact(path) {
                Category::Build
            } else {
                Category::Ignored
            }
        } else {
            Category::Unclassified
        }
    }
    pub fn insert(out: &[u8]) -> Result<BTreeSet<PathBuf>, String> {
        out.split(|b| *b == 0)
            .filter(|p| !p.is_empty())
            .map(|p| {
                let name = std::str::from_utf8(p).map_err(|_| "Non UTF-8 Git path")?;
                let path = PathBuf::from(name);
                if path
                    .components()
                    .any(|c| !matches!(c, Component::Normal(_)))
                {
                    return Err("Unsafe Git path".into());
                }
                Ok(path)
            })
            .collect()
    }
}

/// Deliberately small artifact allow-list. A tooling marker never blesses secrets/data.
#[cfg(any(target_os = "linux", test))]
pub(super) fn artifact(path: &Path) -> bool {
    let Some(parts) = path.iter().map(|p| p.to_str()).collect::<Option<Vec<_>>>() else {
        return false;
    };
    if parts.len() < 2 || parts[0] != "target" {
        return false;
    }
    if parts.len() == 2 {
        return parts[1] == "CACHEDIR.TAG";
    }
    if parts
        .iter()
        .skip(1)
        .any(|p| p.starts_with('.') && *p != ".fingerprint")
    {
        return false;
    }
    if parts[1] == "doc" {
        return parts.len() >= 3
            && matches!(
                path.extension().and_then(|e| e.to_str()),
                Some("html" | "css" | "js" | "svg" | "png" | "woff" | "woff2" | "ttf")
            );
    }
    if !matches!(parts[1], "debug" | "release") || parts.len() < 4 {
        return false;
    }
    let layout = matches!(
        parts[2],
        "deps" | "build" | "incremental" | ".fingerprint" | "examples"
    );
    let name = parts.last().copied().unwrap_or("");
    layout
        && (matches!(
            path.extension().and_then(|e| e.to_str()),
            Some(
                "rlib"
                    | "rmeta"
                    | "o"
                    | "d"
                    | "so"
                    | "dll"
                    | "a"
                    | "lib"
                    | "exe"
                    | "bin"
                    | "json"
                    | "timestamp"
            )
        ) || (parts[2] == ".fingerprint"
            && ["lib-", "dep-lib-", "test-", "dep-test-", "build-script-"]
                .iter()
                .any(|prefix| name.starts_with(prefix))))
}

/// A Cargo-generated observation marker is protected, not an artifact itself.
#[cfg(target_os = "linux")]
pub(super) fn marker(path: &Path) -> bool {
    path == Path::new("target/.rustc_info.json")
}
