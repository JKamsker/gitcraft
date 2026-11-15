//! Pre-commit verification reuses the lane's disposable worktrees and process owner.
//! A copied index is the only index on which write-tree runs (including split indexes).
#[cfg(test)]
use super::run_command;
use super::{RunStop, Scratch};
use crate::git::{self, Ended};
use sha2::{Digest, Sha256};
use std::{
    path::Path,
    time::{Duration, Instant},
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct StagedKey {
    pub tree: String,
    pub fingerprint: String,
}

pub(crate) struct StagedResult {
    pub key: StagedKey,
    pub command: String,
    pub ended: Ended,
    pub lines: Vec<String>,
    pub elapsed: Duration,
}

fn local(repo: &Path) -> Result<(), String> {
    if crate::remote::is_remote(repo) {
        return Err(crate::i18n::text("staged-remote").into());
    }
    Ok(())
}

/// Cheap freshness evidence: canonical index entries, command, process environment,
/// effective Git configuration and profile. Values are hashed, never displayed.
pub(crate) fn staged_fingerprint(repo: &Path, command: &str) -> Result<String, String> {
    local(repo)?;
    let entries = git::run_bytes(Some(repo), &["ls-files", "--stage", "-z"])?;
    let mut hash = Sha256::new();
    hash.update(entries);
    hash.update(command.as_bytes());
    hash.update(git::run_bytes(Some(repo), &["config", "--null", "--list"])?);
    let mut env: Vec<_> = std::env::vars_os().collect();
    env.sort();
    for (name, value) in env {
        // Include lengths via Debug's quoted encoding, preventing ambiguous concatenation.
        hash.update(format!("{name:?}={value:?}\0").as_bytes());
    }
    Ok(hash.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

fn initial_revision(repo: &Path) -> Result<String, String> {
    if let Ok(head) = git::run(Some(repo), &["rev-parse", "--verify", "HEAD"]) {
        return Ok(head.trim().into());
    }
    let empty = git::run_captured(repo, &["mktree"], Some(b""))?;
    if empty.code != Some(0) {
        return Err(empty.stderr);
    }
    synthetic_revision(repo, empty.stdout.trim())
}

fn synthetic_revision(repo: &Path, tree: &str) -> Result<String, String> {
    git::run_env(
        Some(repo),
        &[
            "-c",
            "commit.gpgSign=false",
            "commit-tree",
            tree,
            "-m",
            "staged verification",
        ],
        &[
            ("GIT_AUTHOR_NAME", "GitCraft"),
            ("GIT_AUTHOR_EMAIL", "snapshot@localhost"),
            ("GIT_COMMITTER_NAME", "GitCraft"),
            ("GIT_COMMITTER_EMAIL", "snapshot@localhost"),
        ],
    )
    .map(|s| s.trim().into())
}

/// The scratch registry supplies RAII removal and next-launch crash recovery. No refs
/// or files in the source worktree are changed. Tree objects may be added to its ODB.
fn snapshot(repo: &Path, expected: Option<&[u8]>) -> Result<(Scratch, String), String> {
    local(repo)?;
    let scratch = Scratch::add_empty(repo, "test", &initial_revision(repo)?)?;
    let source = git::run(
        Some(repo),
        &["rev-parse", "--path-format=absolute", "--git-path", "index"],
    )?;
    let target = git::run(
        Some(&scratch.path),
        &["rev-parse", "--path-format=absolute", "--git-path", "index"],
    )?;
    let target = target.trim();
    match std::fs::copy(source.trim(), target) {
        Ok(_) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            git::run(Some(&scratch.path), &["read-tree", "--empty"])?;
        }
        Err(e) => return Err(e.to_string()),
    }
    if let Some(expected) = expected {
        let copied = std::fs::read(target).map_err(|e| e.to_string())?;
        if copied != expected {
            return Err(crate::i18n::text("staged-stale").into());
        }
    }
    let env = [("GIT_INDEX_FILE", target)];
    // Run in the source repository so split-index shared files resolve there.
    let unresolved = git::run_env(Some(repo), &["ls-files", "--unmerged"], &env)?;
    if !unresolved.is_empty() {
        return Err(crate::i18n::text("staged-unmerged").into());
    }
    let tree = git::run_env(Some(repo), &["write-tree"], &env)?
        .trim()
        .to_owned();
    let entries = git::run(Some(repo), &["ls-tree", "-r", &tree])?;
    if entries.lines().any(|line| line.starts_with("160000 ")) {
        return Err(crate::i18n::text("staged-submodules").into());
    }
    // Replace the copied split index with a standalone scratch index before checkout.
    std::fs::remove_file(target).map_err(|e| e.to_string())?;
    // HEAD must describe the snapshot too: tools that inspect HEAD or reset their
    // disposable checkout must not silently switch back to the source commit.
    scratch.checkout(&synthetic_revision(repo, &tree)?)?;
    Ok((scratch, tree))
}

pub(crate) fn test_staged(
    repo: &Path,
    command: &str,
    stop: &RunStop,
) -> Result<StagedResult, String> {
    let lifetime = stop.command_stopper();
    lifetime
        .with_scope(|| verify(repo, command, stop))
        .map_err(|error| scrub(&error))
}

fn verify(repo: &Path, command: &str, stop: &RunStop) -> Result<StagedResult, String> {
    let started = Instant::now();
    if stop.stopped() {
        return Err(crate::i18n::text("staged-cancelled").into());
    }
    local(repo)?;
    let index = git::run(
        Some(repo),
        &["rev-parse", "--path-format=absolute", "--git-path", "index"],
    )?;
    let expected = match std::fs::read(index.trim()) {
        Ok(bytes) => Some(bytes),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => return Err(e.to_string()),
    };
    let fingerprint = staged_fingerprint(repo, command)?;
    let (scratch, tree) = snapshot(repo, expected.as_deref())?;
    // A concurrent staging operation must never label a mixed/new snapshot as this run.
    if staged_fingerprint(repo, command)? != fingerprint {
        return Err(crate::i18n::text("staged-stale").into());
    }
    let ran = super::runner::run_command_timed(
        &scratch.path,
        command,
        &stop.command_stopper(),
        Some(Duration::from_secs(20 * 60)),
    );
    let cleanup = scratch.remove_retrying(3, Duration::from_millis(50));
    cleanup?;
    let (ended, lines) = ran?;
    Ok(StagedResult {
        key: StagedKey { tree, fingerprint },
        command: scrub(command),
        ended,
        lines: lines.into_iter().map(|s| scrub(&s)).collect(),
        elapsed: started.elapsed(),
    })
}

#[cfg(test)]
mod tests;

// Redact known credential environment values as well as the common Git URL/header forms.
fn scrub(text: &str) -> String {
    let mut text = git::redact(text, None);
    for (name, value) in std::env::vars_os() {
        let name = name.to_string_lossy().to_ascii_uppercase();
        if ["TOKEN", "PASSWORD", "SECRET", "CREDENTIAL", "API_KEY"]
            .iter()
            .any(|key| name.contains(key))
        {
            let value = value.to_string_lossy();
            if value.len() >= 3 {
                text = text.replace(value.as_ref(), "***");
            }
        }
    }
    text
}
