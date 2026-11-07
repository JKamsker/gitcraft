//! Repositories with files in Git LFS, for tests.

use std::path::{Path, PathBuf};

use super::run;

/// Keeps files matching `pattern` in LFS from now on (`git lfs track`, which writes
/// `.gitattributes`). Files already committed stay in git until they are changed or migrated.
pub(crate) fn lfs_track(repo: &Path, pattern: &str) -> Result<String, String> {
    if pattern.trim().is_empty() {
        return Err("Type a pattern, such as *.psd".to_owned());
    }
    run(Some(repo), &["lfs", "track", "--", pattern.trim()])
}

pub(crate) fn git(dir: &Path, args: &[&str]) -> Result<String, String> {
    run(Some(dir), args)
}

/// A repository with two LFS files pushed to a bare one, and a clone of it whose LFS objects
/// were never downloaded; `None` (and why) when git-lfs isn't installed.
pub(crate) fn pushed_and_cloned(name: &str) -> Result<Option<(PathBuf, PathBuf)>, String> {
    if run(None, &["lfs", "version"]).is_err() {
        eprintln!("skipped: git-lfs is not installed here");
        return Ok(None);
    }
    let base =
        std::env::temp_dir().join(format!("gitcraft-lfs-store-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    let (work, bare, clone) = (base.join("work"), base.join("bare"), base.join("clone"));
    for d in [&work, &bare] {
        std::fs::create_dir_all(d).map_err(|e| e.to_string())?;
    }
    git(&bare, &["init", "-q", "--bare"])?;
    git(&work, &["init", "-q"])?;
    for args in [
        &["config", "user.name", "t"][..],
        &["config", "user.email", "t@t"],
        &["config", "commit.gpgSign", "false"],
        &["lfs", "install", "--local"],
    ] {
        git(&work, args)?;
    }
    lfs_track(&work, "*.bin")?;
    std::fs::create_dir_all(work.join("art dir")).map_err(|e| e.to_string())?;
    std::fs::write(work.join("art dir/a b.bin"), vec![7u8; 5000]).map_err(|e| e.to_string())?;
    std::fs::write(work.join("ü.bin"), vec![1u8; 300]).map_err(|e| e.to_string())?;
    git(&work, &["add", "-A"])?;
    git(&work, &["commit", "-qm", "one"])?;
    std::fs::write(work.join("ü.bin"), vec![2u8; 700]).map_err(|e| e.to_string())?;
    git(&work, &["commit", "-qam", "two"])?;
    let url = bare.to_string_lossy().replace('\\', "/");
    git(&work, &["remote", "add", "origin", &url])?;
    git(&work, &["push", "-q", "origin", "HEAD:refs/heads/main"])?;
    let clone_to = clone.to_string_lossy().into_owned();
    run(
        None,
        &[
            "-c",
            "filter.lfs.smudge=git-lfs smudge --skip -- %f",
            "-c",
            "filter.lfs.process=git-lfs filter-process --skip",
            "clone",
            "-q",
            "-b",
            "main",
            &url,
            &clone_to,
        ],
    )?;
    git(&clone, &["config", "lfs.fetchrecentcommitsdays", "0"])?;
    Ok(Some((work, clone)))
}
