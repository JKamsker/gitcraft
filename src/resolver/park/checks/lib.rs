//! Isolated, process-free harness. All Git calls are fake; archive/restore code is production.
#![allow(dead_code, unused_imports)]
mod fake_git;
mod resolver;
use fake_git as git;
mod remote {
    use std::path::Path;
    pub fn is_remote(_: &Path) -> bool {
        false
    }
    pub fn same_dir(a: &Path, b: &Path) -> bool {
        a == b
    }
}
mod mcp {
    pub mod args {
        pub fn repo_path(p: &str) -> Result<&str, String> {
            if p.is_empty()
                || std::path::Path::new(p)
                    .components()
                    .any(|c| !matches!(c, std::path::Component::Normal(_)))
            {
                Err("Unsafe path".into())
            } else {
                Ok(p)
            }
        }
    }
}
mod safe_read {
    pub fn read_inside(base: &std::path::Path, path: &str, _: u64) -> Option<Vec<u8>> {
        crate::mcp::args::repo_path(path).ok()?;
        std::fs::read(base.join(path)).ok()
    }
}
mod agent {
    use std::path::{Path, PathBuf};
    pub fn repo_file(repo: &Path, name: &str) -> Result<PathBuf, String> {
        Ok(repo.join(".git/gitcraft").join(name))
    }
    pub mod snapshot {
        use std::path::Path;
        pub fn temporary_index(repo: &Path, _: &str) -> Result<String, String> {
            Ok(repo.join(".git/temp-index").to_string_lossy().into_owned())
        }
        pub fn remove(_: &Path, path: &str) {
            let _ = std::fs::remove_file(path);
        }
    }
}
