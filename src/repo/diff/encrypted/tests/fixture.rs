use super::*;
use crate::git::{self, Area, StatusFile};
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::sync::atomic::AtomicU64;
struct Repo(PathBuf);
impl Repo {
    fn new() -> Result<Self, String> {
        static SEQ: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "gitcraft-sops ü 'fixture'-{}-{}",
            std::process::id(),
            SEQ.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&root).map_err(|e| e.to_string())?;
        let repo = Self(root);
        for args in [
            vec!["init", "-q"],
            vec!["config", "user.name", "Fixture"],
            vec!["config", "user.email", "fixture@example.invalid"],
            vec!["config", "commit.gpgSign", "false"],
        ] {
            repo.git(&args)?;
        }
        Ok(repo)
    }
    fn git(&self, args: &[&str]) -> Result<String, String> {
        git::run(Some(&self.0), args)
    }
    fn write(&self, path: &str, text: &str) -> Result<(), String> {
        std::fs::write(self.0.join(path), text).map_err(|e| e.to_string())
    }
    fn commit(&self) -> Result<String, String> {
        self.git(&["add", "-A"])?;
        self.git(&["commit", "-qm", "fixture"])?;
        self.git(&["rev-parse", "HEAD"])
            .map(|s| s.trim().to_owned())
    }
}
impl Drop for Repo {
    fn drop(&mut self) {
        tool::TEST_PROGRAM.with(|p| *p.borrow_mut() = None);
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn work(path: &str, area: Area, code: char) -> DiffKey {
    DiffKey::Work(StatusFile {
        path: path.into(),
        old_path: None,
        area,
        code,
    })
}
fn program(repo: &Repo, body: &str) -> Result<PathBuf, String> {
    let tool = repo.0.join("sops fixture 'tool'");
    std::fs::write(&tool, format!("#!/bin/sh\n{body}\n")).map_err(|e| e.to_string())?;
    std::fs::set_permissions(&tool, std::fs::Permissions::from_mode(0o755))
        .map_err(|e| e.to_string())?;
    tool::TEST_PROGRAM.with(|p| *p.borrow_mut() = Some(tool.clone()));
    Ok(tool)
}

// Lifecycle; included to preserve qualified test names.
include!("fixture/lifecycle.rs");

// Real sops; included to preserve qualified test names.
include!("fixture/real_sops.rs");

// Local tool; included to preserve qualified test names.
include!("fixture/local_tool.rs");

// Source policy; included to preserve qualified test names.
include!("fixture/source_policy.rs");

// Private ui; included to preserve qualified test names.
include!("fixture/private_ui.rs");
