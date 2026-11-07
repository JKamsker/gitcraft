//! Real Git contract for the three independent repository visualization readers.
use super::{activity::Query, atlas, growth, postcards};
use std::{
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
};
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Repo(PathBuf);
impl Repo {
    fn new() -> Result<Self, String> {
        let path = std::env::temp_dir().join(format!(
            "gitcraft-visual-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).map_err(|e| e.to_string())?;
        let repo = Self(path);
        for args in [
            &["init", "-q"][..],
            &["config", "user.name", "Alias"],
            &["config", "user.email", "alias@test.invalid"],
            &["config", "commit.gpgSign", "false"],
            &["config", "core.autocrlf", "false"],
        ] {
            repo.git(args)?;
        }
        Ok(repo)
    }
    fn git(&self, args: &[&str]) -> Result<String, String> {
        let out = Command::new("git")
            .arg("-C")
            .arg(&self.0)
            .args(args)
            .output()
            .map_err(|e| e.to_string())?;
        if !out.status.success() {
            return Err(String::from_utf8_lossy(&out.stderr).into_owned());
        }
        Ok(String::from_utf8_lossy(&out.stdout).trim().to_owned())
    }
    fn commit(&self, msg: &str) -> Result<String, String> {
        self.git(&["add", "."])?;
        self.git(&["commit", "-qm", msg])?;
        self.git(&["rev-parse", "HEAD"])
    }
}
impl Drop for Repo {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
#[test]
fn atlas_growth_and_postcard_read_a_real_pinned_release_without_mutation() -> Result<(), String> {
    let repo = Repo::new()?;
    std::fs::create_dir(repo.0.join("src")).map_err(|e| e.to_string())?;
    std::fs::write(repo.0.join("src/a"), "base").map_err(|e| e.to_string())?;
    let base = repo.commit("base")?;
    let main = repo.git(&["branch", "--show-current"])?;
    repo.git(&["checkout", "-qb", "feature"])?;
    std::fs::write(repo.0.join("side"), "feature").map_err(|e| e.to_string())?;
    repo.commit("feature")?;
    repo.git(&["checkout", "-q", &main])?;
    repo.git(&["merge", "--no-ff", "-qm", "merge feature", "feature"])?;
    repo.git(&["mv", "src/a", "src/b"])?;
    let tip = repo.commit("rename")?;
    repo.git(&["tag", "v1"])?;
    std::fs::write(
        repo.0.join(".mailmap"),
        "Canonical <canonical@test.invalid> Alias <alias@test.invalid>\n",
    )
    .map_err(|e| e.to_string())?;
    let before = repo.git(&["status", "--porcelain=v1"])?;
    let atlas = atlas::read(&repo.0, 100, Query::default(), &|| true)?;
    assert_eq!(atlas.head, tip);
    assert_eq!(atlas.history.commits.len(), 4);
    assert!(
        atlas
            .nodes
            .iter()
            .any(|n| n.path == b"src/a" && n.bytes == 0 && n.changes == 2)
    );
    assert!(
        atlas
            .nodes
            .iter()
            .any(|n| n.path == b"src/b" && n.bytes == 4 && n.contributor.starts_with("Canonical"))
    );
    let growth = growth::read(&repo.0, &|| true)?;
    assert_eq!(growth.frames.len(), 3); // feature enters via the first-parent merge
    let final_tree = growth.files_at(growth.frames.len() - 1);
    assert!(final_tree.contains(b"src/b".as_slice()));
    assert!(final_tree.contains(b"side".as_slice()));
    assert!(!final_tree.contains(b"src/a".as_slice()));
    assert!(growth.frames.iter().all(|f| f.name == "Canonical"));
    let release = postcards::range(&repo.0, &base, &tip, &|| true)?;
    assert_eq!(release.len(), 3);
    assert!(release.iter().all(|c| c.author == "Canonical"));
    assert_eq!(release[0].hash, tip);
    assert!(postcards::range(&repo.0, "--all", &tip, &|| true).is_err());
    repo.git(&["tag", "-f", "v1", &base])?;
    assert_eq!(postcards::range(&repo.0, &base, &tip, &|| true)?.len(), 3);
    assert_eq!(repo.git(&["status", "--porcelain=v1"])?, before);
    Ok(())
}
