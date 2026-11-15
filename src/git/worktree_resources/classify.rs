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
            if cargo && path.starts_with("target") {
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
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    matches!(name, "CACHEDIR.TAG" | ".rustc_info.json" | ".cargo-lock")
        || matches!(
            path.extension().and_then(|e| e.to_str()),
            Some("rlib" | "rmeta" | "o" | "d" | "so" | "dll" | "a" | "lib" | "exe")
        )
}
