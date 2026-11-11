//! Content and attribution dependency fingerprints for file-view reads.

use super::*;

pub(in crate::repo) fn content_key(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

pub(super) fn file_inputs(
    repo: &std::path::Path,
    rev: Option<&str>,
    path: &str,
    blame: bool,
) -> FileInputs {
    FileInputs {
        blame: blame.then(|| blame_input(repo, rev, path)).flatten(),
        history: git::resolve(repo, rev.filter(|rev| !rev.is_empty()).unwrap_or("HEAD")).ok(),
        text: git::file_at(repo, rev, path)
            .ok()
            .map(|bytes| content_key(&bytes)),
    }
}

/// Blame depends on attribution settings even at a pinned revision. Observe effective
/// configuration and the content/existence of its repository-side file inputs. Explicit
/// refresh always invalidates, including external textconv programs with mutable state.
pub(in crate::repo) fn blame_input(
    repo: &std::path::Path,
    rev: Option<&str>,
    path: &str,
) -> Option<String> {
    let mut bytes = git::run(Some(repo), &["config", "--null", "--list", "--show-origin"])
        .ok()?
        .into_bytes();
    let mut add = |label: &str, value: Result<Vec<u8>, String>| {
        bytes.extend_from_slice(&(label.len() as u64).to_le_bytes());
        bytes.extend_from_slice(label.as_bytes());
        let value = match value {
            Ok(value) => [vec![0], value].concat(),
            Err(error) => [vec![1], error.into_bytes()].concat(),
        };
        bytes.extend_from_slice(&(value.len() as u64).to_le_bytes());
        bytes.extend(value);
    };
    add(
        "revision",
        git::resolve(repo, rev.filter(|r| !r.is_empty()).unwrap_or("HEAD")).map(String::into_bytes),
    );
    if rev.is_none() {
        add("worktree", git::file_at(repo, None, path));
        add("index", git::file_at(repo, Some(""), path));
    }
    // Ask Git to resolve effective attributes, including system/default-global
    // sources and index fallback. Hashing only worktree .gitattributes misses both.
    add(
        "attributes",
        git::run(Some(repo), &["check-attr", "-z", "--all", "--", path]).map(String::into_bytes),
    );
    let mut files = vec![".mailmap".to_owned()];
    for key in ["blame.ignoreRevsFile", "mailmap.file"] {
        let values = git::run_ok_codes(
            Some(repo),
            &["config", "--null", "--path", "--get-all", key],
            &[0, 1],
        )
        .ok()?;
        files.extend(
            values
                .split('\0')
                .filter(|s| !s.is_empty())
                .map(str::to_owned),
        );
    }
    for file in files {
        add(&file, attribution_file_input(repo, &file));
    }
    // A configured ref:path is mutable even when the viewed commit is fixed.
    // HEAD:.mailmap also covers Git's bare-repository default conservatively.
    let blobs = git::run_ok_codes(
        Some(repo),
        &["config", "--null", "--get-all", "mailmap.blob"],
        &[0, 1],
    )
    .ok()?;
    for blob in blobs
        .split('\0')
        .filter(|s| !s.is_empty())
        .chain(std::iter::once("HEAD:.mailmap"))
    {
        add(
            blob,
            git::run(Some(repo), &["cat-file", "blob", blob]).map(String::into_bytes),
        );
    }
    Some(content_key(&bytes))
}

/// Configuration dependencies are read by Git with ordinary symlink-following
/// file semantics. Keep absolute paths on the repository's machine: joining an
/// absolute dependency to an ssh:// path would silently discard the remote host.
pub(super) fn attribution_file_input(
    repo: &std::path::Path,
    file: &str,
) -> Result<Vec<u8>, String> {
    attribution_file_input_with(repo, file, git::run_shell)
}

pub(super) fn attribution_file_input_with(
    repo: &std::path::Path,
    file: &str,
    run: impl FnOnce(&std::path::Path, &str) -> Result<git::ShellRun, String>,
) -> Result<Vec<u8>, String> {
    let file = crate::remote::sh_quote(file);
    let script = format!(
        "if test -f {file}; then printf 'file:'; git hash-object --stdin < {file}; elif test -e {file} || test -L {file}; then printf 'unreadable-or-nonregular'; else printf 'absent'; fi"
    );
    let result = run(repo, &script)?;
    if result.code == 0 {
        Ok(result.stdout)
    } else {
        Err(String::from_utf8_lossy(&result.stderr).into_owned())
    }
}
