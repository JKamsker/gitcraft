//! Pre-commit verification reuses the lane's disposable worktrees and process owner.
//! A copied index is the only index on which write-tree runs (including split indexes).
#[cfg(test)]
use super::run_command;
use super::{RunStop, Scratch};
mod bounded;
mod preflight;
use crate::git::{self, Ended};
use bounded::read as read_bounded;
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

/// Freshness evidence: captured index bytes/flags, command, process environment,
/// effective Git configuration and profile. Values are hashed, never displayed.
pub(crate) fn staged_fingerprint(repo: &Path, command: &str) -> Result<String, String> {
    local(repo)?;
    fingerprint_for(repo, command, index_bytes(repo)?.as_deref())
}

fn fingerprint_for(repo: &Path, command: &str, index: Option<&[u8]>) -> Result<String, String> {
    let mut hash = Sha256::new();
    // Raw flags matter too: intent-to-add and an actually staged empty file have
    // identical ls-files --stage rows but different write-tree results.
    hash.update(b"staged/raw-blobs-v1\0");
    hash.update(index.unwrap_or_default());
    hash.update(command.as_bytes());
    hash.update(crate::git::command::snapshot_policy::without_hooks(|| {
        git::run_bytes(Some(repo), &["config", "--null", "--list"])
    })?);
    let attributes = git::run(
        Some(repo),
        &[
            "rev-parse",
            "--path-format=absolute",
            "--git-path",
            "info/attributes",
        ],
    )?;
    hash.update(read_bounded(Path::new(attributes.trim()))?.unwrap_or_default());
    let mut env: Vec<_> = std::env::vars_os().collect();
    env.sort();
    for (name, value) in env {
        // Include lengths via Debug's quoted encoding, preventing ambiguous concatenation.
        hash.update(format!("{name:?}={value:?}\0").as_bytes());
    }
    Ok(hash.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

fn index_bytes(repo: &Path) -> Result<Option<Vec<u8>>, String> {
    let index = git::run(
        Some(repo),
        &["rev-parse", "--path-format=absolute", "--git-path", "index"],
    )?;
    read_bounded(Path::new(index.trim()))
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
    crate::git::command::snapshot_policy::without_hooks(|| snapshot_inner(repo, expected))
}

fn snapshot_inner(repo: &Path, expected: Option<&[u8]>) -> Result<(Scratch, String), String> {
    local(repo)?;
    preflight::dense(repo)?;
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
    let copied = read_bounded(Path::new(source.trim()))?;
    if expected.is_some() && copied.as_deref() != expected {
        return Err(crate::i18n::text("staged-stale").into());
    }
    if let Some(bytes) = copied {
        bounded::write(Path::new(target), &bytes)?;
    } else {
        git::run(Some(&scratch.path), &["read-tree", "--empty"])?;
    }
    let tree = index_tree(repo, target, &scratch.path)?;
    let entries = preflight::manifest(repo, &tree, &scratch.path)?;
    // A raw read-tree creates a standalone scratch index without touching worktree
    // files. Materialization never calls checkout, diff-files or attribute conversions.
    std::fs::remove_file(target).map_err(|e| e.to_string())?;
    git::run(Some(&scratch.path), &["read-tree", &tree])?;
    let revision = synthetic_revision(repo, &tree)?;
    git::command::run_inert_objects(
        &scratch.path,
        &["update-ref", "--no-deref", "HEAD", &revision],
        None,
        4096,
        &[0],
    )?;
    preflight::materialize(repo, &scratch.path, &entries)?;
    preflight::faithful(&scratch.path, &entries)?;
    Ok((scratch, tree))
}

/// Git may rehash racy entries while writing an index. Keep its actual cwd empty,
/// not merely GIT_WORK_TREE: write-tree does not set up/chdir into a worktree.
/// The source Git dir still resolves split-index shared files; only the copy is written.
fn index_tree(repo: &Path, index: &str, root: &Path) -> Result<String, String> {
    let git_dir = git::run(Some(repo), &["rev-parse", "--absolute-git-dir"])?;
    let empty = tempfile::tempdir_in(root).map_err(|e| e.to_string())?;
    let worktree = git::bundle_git_path(empty.path())?;
    let env = [
        ("GIT_INDEX_FILE", index),
        ("GIT_DIR", git_dir.trim()),
        ("GIT_WORK_TREE", worktree.as_str()),
    ];
    let unresolved = git::run_env(Some(empty.path()), &["ls-files", "--unmerged"], &env)?;
    if !unresolved.is_empty() {
        return Err(crate::i18n::text("staged-unmerged").into());
    }
    let tree = git::run_env(Some(empty.path()), &["write-tree"], &env)?;
    empty.close().map_err(|e| e.to_string())?;
    Ok(tree.trim().into())
}

pub(crate) fn test_staged(
    repo: &Path,
    command: &str,
    stop: &RunStop,
) -> Result<StagedResult, String> {
    let lifetime = stop.command_stopper();
    lifetime
        .with_scope(|| {
            crate::git::command::snapshot_policy::without_hooks(|| {
                crate::processes::deadline::within(
                    Instant::now() + Duration::from_secs(20 * 60),
                    || verify(repo, command, stop),
                )
            })
        })
        .map_err(|error| scrub(&error))
}

fn verify(repo: &Path, command: &str, stop: &RunStop) -> Result<StagedResult, String> {
    let started = Instant::now();
    if stop.stopped() {
        return Err(crate::i18n::text("staged-cancelled").into());
    }
    local(repo)?;
    let expected = index_bytes(repo)?;
    let fingerprint = fingerprint_for(repo, command, expected.as_deref())?;
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

#[cfg(test)]
mod review_tests;
