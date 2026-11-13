//! Reading a revision through git: large trees must not deadlock on the pipes.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

use gitcraft_arch_fitness::source;

/// Far beyond any OS pipe buffer in both directions: ~1 MB of `cat-file` requests and
/// ~16 MB of blob output. Writing all requests before reading any output deadlocks.
const FILES: usize = 25_000;
const BLOB_BYTES: usize = 640;

fn git(dir: &Path, args: &[&str], input: Option<&str>) -> Result<String, String> {
    let mut child = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .env("GIT_AUTHOR_NAME", "t")
        .env("GIT_AUTHOR_EMAIL", "t@example.invalid")
        .env("GIT_COMMITTER_NAME", "t")
        .env("GIT_COMMITTER_EMAIL", "t@example.invalid")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| e.to_string())?;
    let mut stdin = child.stdin.take().ok_or("no stdin")?;
    let input = input.unwrap_or_default().to_owned();
    // Fixture setup feeds git from a thread so that it cannot deadlock itself.
    let writer =
        std::thread::spawn(move || std::io::Write::write_all(&mut stdin, input.as_bytes()));
    let out = child.wait_with_output().map_err(|e| e.to_string())?;
    writer
        .join()
        .map_err(|_| "writer panicked")?
        .map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err(format!(
            "git {}: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_owned())
}

/// A repository whose single commit tracks `FILES` Rust files sharing one blob.
fn large_repo() -> Result<(PathBuf, String), String> {
    let dir = std::env::temp_dir().join(format!("arch-fitness-source-{}", std::process::id()));
    if dir.exists() {
        std::fs::remove_dir_all(&dir).map_err(|e| e.to_string())?;
    }
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    git(&dir, &["init", "-q"], None)?;
    let body = format!("pub fn f() {{}}\n{}\n", "//".repeat(BLOB_BYTES / 2));
    let blob = git(&dir, &["hash-object", "-w", "--stdin"], Some(&body))?;
    let index: String = (0..FILES)
        .map(|i| format!("100644 {blob}\tsrc/m{i}.rs\n"))
        .collect();
    git(&dir, &["update-index", "--index-info"], Some(&index))?;
    let tree = git(&dir, &["write-tree"], None)?;
    let commit = git(&dir, &["commit-tree", &tree, "-m", "large"], None)?;
    Ok((dir, commit))
}

#[test]
fn large_revisions_are_read_without_a_pipe_deadlock() -> Result<(), String> {
    let (dir, commit) = large_repo()?;
    let (done, result) = mpsc::channel();
    let root = dir.clone();
    std::thread::spawn(move || {
        let _ = done.send(source::revision(&root, &commit).map(|(files, _)| files));
    });
    let files = result
        .recv_timeout(Duration::from_secs(60))
        .map_err(|_| "reading the revision hung: git pipes deadlocked".to_owned())??;
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(files.len(), FILES);
    assert!(
        files
            .values()
            .all(|source| source.starts_with("pub fn f() {}"))
    );
    Ok(())
}
