//! Offline export, read-only header inspection and explicitly confirmed contained import.
use std::collections::BTreeSet;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use super::{command::bounded, run};
use crate::remote;

pub const BUNDLE_LIMIT: usize = 128 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BundleRef {
    pub name: String,
    pub oid: String,
}
#[derive(Clone, Debug)]
pub struct BundlePreview {
    pub refs: Vec<BundleRef>,
    pub prerequisites: Vec<BundleRef>,
    /// Only header/prerequisites are verified. Pack data is never unpacked by inspection.
    pub verification_error: Option<String>,
    pub(crate) bytes: std::sync::Arc<Vec<u8>>,
    pub digest: String,
    pub(crate) repository: String,
}

pub fn bundle_refs(repo: &Path) -> Result<Vec<BundleRef>, String> {
    let out = bounded::run(
        repo,
        &[
            "for-each-ref",
            "--format=%(objectname) %(refname) %(symref)",
            "refs/heads",
            "refs/tags",
            "refs/remotes",
        ],
        vec![],
        HEADER_LIMIT,
    )?;
    let out = std::str::from_utf8(&out)
        .map_err(|_| crate::i18n::text("backend-repository-refs-are-not-utf-8-606e2f"))?;
    let mut refs = Vec::new();
    for line in out.lines() {
        let words: Vec<_> = line.split_ascii_whitespace().collect();
        if let [oid, name] = words.as_slice() {
            refs.push(BundleRef {
                name: (*name).into(),
                oid: (*oid).into(),
            });
        }
    }
    if let Ok(oid) = run(
        Some(repo),
        &["rev-parse", "--verify", "--end-of-options", "HEAD"],
    ) {
        refs.insert(
            0,
            BundleRef {
                name: "HEAD".into(),
                oid: oid.trim().into(),
            },
        );
    }
    if refs.len() > REF_LIMIT {
        return Err(crate::i18n::text(
            "backend-over-1000-refs-select-a-bundle-with-git-in-the-terminal-029f90",
        )
        .into());
    }
    Ok(refs)
}

/// Native Git creates the pack. Both its inputs and resulting advertised refs are pinned.
pub fn export_bundle(
    repo: &Path,
    refs: &[BundleRef],
    base: Option<&str>,
) -> Result<Vec<u8>, String> {
    if refs.is_empty() || refs.len() > REF_LIMIT {
        return Err(crate::i18n::text("backend-choose-between-1-and-1000-refs-e37d2c").into());
    }
    let available: std::collections::HashMap<_, _> = bundle_refs(repo)?
        .into_iter()
        .map(|r| (r.name, r.oid))
        .collect();
    let mut seen = BTreeSet::new();
    for r in refs {
        valid_ref(&r.name)?;
        if !valid_oid(&r.oid) || !seen.insert(&r.name) {
            return Err(crate::i18n::text("backend-invalid-or-duplicate-export-ref-8df16c").into());
        }
        if available.get(&r.name) != Some(&r.oid) {
            return Err(crate::i18n::message(
                "backend-moved-since-the-export-dialog-opened-reopen-it-274c7b",
                &[("name", (r.name).to_string().into())],
            ));
        }
    }
    let no_lazy = no_lazy_fetch(repo)?;
    let base = base
        .filter(|s| !s.trim().is_empty())
        .map(|s| {
            let rev = format!("{}^{{commit}}", s.trim());
            let args: Vec<_> = no_lazy
                .iter()
                .copied()
                .chain(["rev-parse", "--verify", "--end-of-options", &rev])
                .collect();
            bounded::run(repo, &args, vec![], HEADER_LIMIT).and_then(|out| {
                String::from_utf8(out)
                    .map(|s| s.trim().to_owned())
                    .map_err(|_| {
                        crate::i18n::text("backend-base-object-id-is-not-utf-8-ea46d6").to_owned()
                    })
            })
        })
        .transpose()?;
    let mut args: Vec<String> = no_lazy
        .iter()
        .map(|s| (*s).to_owned())
        .chain([
            "bundle".to_owned(),
            "create".into(),
            "--version=3".into(),
            "-".into(),
        ])
        .collect();
    args.extend(refs.iter().map(|r| r.name.clone()));
    if let Some(base) = base {
        args.push(format!("^{base}"));
    }
    let args: Vec<_> = args.iter().map(String::as_str).collect();
    let bytes = bounded::run(repo, &args, vec![], BUNDLE_LIMIT)?;
    let (actual, _) = header(&bytes)?;
    let expected: BTreeSet<_> = refs.iter().map(|r| (&r.name, &r.oid)).collect();
    let actual: BTreeSet<_> = actual.iter().map(|r| (&r.name, &r.oid)).collect();
    if expected != actual {
        return Err(crate::i18n::text(
            "backend-refs-moved-while-exporting-or-the-range-excludes-a-selected-ref-n-a6c9b9",
        )
        .into());
    }
    Ok(bytes)
}

pub fn inspect_bundle(repo: &Path, bytes: Vec<u8>) -> Result<BundlePreview, String> {
    let (refs, prerequisites) = header(&bytes)?;
    for r in &refs {
        valid_ref(&r.name)?;
    }
    let temp = BundleFile::new(repo, &bytes)?;
    let verification_error = verify(repo, &temp.path, &refs, &bytes).err();
    let repository = crate::parallel_bisect::bundle::identity(repo)?;
    let digest = bundle_digest(&bytes);
    Ok(BundlePreview {
        refs,
        prerequisites,
        verification_error,
        bytes: std::sync::Arc::new(bytes),
        digest,
        repository,
    })
}

/// Validation is repeated against the held file snapshot before any mutation. Import
/// always verifies again; a prior verification result is only information for the user.
pub(crate) fn check_bundle_with_presentation(
    repo: &Path,
    preview: &BundlePreview,
    selected: &BTreeSet<String>,
    prefix: &str,
    import: bool,
    presentation: &mut Option<crate::parallel_bisect::presentation::Presentation>,
) -> Result<Vec<BundleRef>, String> {
    *presentation = None;
    let (refs, prerequisites) = checked_snapshot(preview)?;
    if no_lazy_fetch(repo)?.is_empty() {
        return Err(crate::i18n::text(
            "backend-contained-bundle-import-requires-git-2-45-or-newer-to-prohibit-al-32f792",
        )
        .into());
    }
    let mapped = import_mapping(&refs, selected, prefix)?;
    let pack = preview
        .bytes
        .windows(2)
        .position(|b| b == b"\n\n")
        .ok_or(crate::i18n::text(
            "backend-no-bundle-header-terminator-c6345c",
        ))?
        + 2;
    crate::parallel_bisect::bundle::run_with_presentation(
        repo,
        &preview.repository,
        &preview.bytes[pack..],
        &refs,
        &prerequisites,
        &mapped,
        import,
        presentation,
    )?;
    Ok(mapped)
}

fn checked_snapshot(preview: &BundlePreview) -> Result<(Vec<BundleRef>, Vec<BundleRef>), String> {
    let (refs, prerequisites) = header(&preview.bytes)?;
    if refs != preview.refs
        || prerequisites != preview.prerequisites
        || bundle_digest(&preview.bytes) != preview.digest
    {
        return Err(crate::i18n::text(
            "backend-the-held-bundle-snapshot-changed-reopen-the-file-7d800a",
        )
        .into());
    }
    Ok((refs, prerequisites))
}

pub(crate) fn import_mapping(
    refs: &[BundleRef],
    selected: &BTreeSet<String>,
    prefix: &str,
) -> Result<Vec<BundleRef>, String> {
    if prefix.is_empty()
        || prefix.len() > 64
        || !prefix
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"_-".contains(&b))
    {
        return Err(crate::i18n::text(
            "backend-use-1-64-lowercase-letters-digits-underscores-or-hyphens-for-the-57b168",
        )
        .into());
    }
    if selected.is_empty() || selected.len() > REF_LIMIT {
        return Err(
            crate::i18n::text("backend-select-between-1-and-1000-incoming-refs-c70a4a").into(),
        );
    }
    if selected.iter().any(|n| !refs.iter().any(|r| &r.name == n)) {
        return Err(crate::i18n::text("backend-an-incoming-ref-selection-is-stale-47d344").into());
    }
    let mut mapped = Vec::new();
    let mut folded = BTreeSet::new();
    for r in refs.iter().filter(|r| selected.contains(&r.name)) {
        valid_ref(&r.name)?;
        if !valid_oid(&r.oid) {
            return Err(crate::i18n::text("backend-invalid-incoming-object-id-aa6124").into());
        }
        let tail = r.name.strip_prefix("refs/").unwrap_or(&r.name);
        let name = format!("refs/bundle-import/{prefix}/{tail}");
        valid_ref(&name)?;
        if !folded.insert(name.to_lowercase()) {
            return Err(crate::i18n::text(
                "backend-incoming-refs-collide-when-case-is-ignored-d51e9e",
            )
            .into());
        }
        mapped.push(BundleRef {
            name,
            oid: r.oid.clone(),
        });
    }
    for name in &folded {
        if folded
            .iter()
            .any(|other| other.starts_with(&format!("{name}/")))
        {
            return Err(crate::i18n::text(
                "backend-incoming-refs-have-a-directory-file-name-collision-4335a6",
            )
            .into());
        }
    }
    Ok(mapped)
}

fn verify(
    repo: &Path,
    file: &str,
    refs: &[BundleRef],
    bytes: &[u8],
) -> Result<Vec<&'static str>, String> {
    let format = run(Some(repo), &["rev-parse", "--show-object-format"])?;
    let expected_len = match format.trim() {
        "sha1" => 40,
        "sha256" => 64,
        _ => {
            return Err(
                crate::i18n::text("backend-unsupported-repository-object-format-ecebca").into(),
            );
        }
    };
    if refs.iter().any(|r| r.oid.len() != expected_len) {
        return Err(crate::i18n::message(
            "backend-bundle-object-format-does-not-match-this-repository-978d87",
            &[("trim", (format.trim()).to_string().into())],
        ));
    }
    // Verification resolves prerequisite commits, which otherwise triggers a promisor
    // fetch and modifies the object store before the user chooses Import.
    let no_lazy = no_lazy_fetch(repo)?;
    // A Windows client can run Git on Unix through SSH/WSL. Only local native Git
    // gets the Windows stdin workaround; remote hosts retain pathname verification.
    let local_windows = cfg!(windows) && !remote::is_remote(repo);
    let (args, input) = verification_command(local_windows, &no_lazy, file, bytes)?;
    bounded::run(repo, &args, input, HEADER_LIMIT)?;
    Ok(no_lazy)
}

/// Git 2.45 added --no-lazy-fetch, after bundle verification gained stdin support.
/// Use that existing capability probe to retain the older ordinary-repository fallback.
fn verification_command<'a>(
    use_stdin: bool,
    no_lazy: &[&'a str],
    file: &'a str,
    bytes: &[u8],
) -> Result<(Vec<&'a str>, Vec<u8>), String> {
    // Unix Git closes stdin before checking prerequisite connectivity; its fork/pipe
    // setup can then close rev-list's fd 0. Keep pathname verification on Unix.
    // Native Windows duplicates inherited handles instead and needs the pathname bypass.
    let (file, input) = if !use_stdin || no_lazy.is_empty() {
        (file, Vec::new())
    } else {
        // Native Git for Windows opens bundle files before initializing long-path config.
        // Streaming the exact validated header avoids that pathname limit. Do not send the
        // pack: verify closes stdin after the header, leaving a large writer with EPIPE.
        header(bytes)?;
        let end = bytes
            .windows(2)
            .position(|b| b == b"\n\n")
            .ok_or(crate::i18n::text("backend-no-bundle-header-dfa585"))?
            + 2;
        ("-", bytes[..end].to_vec())
    };
    let args = no_lazy
        .iter()
        .copied()
        .chain(["-c", "core.longpaths=true", "bundle", "verify", file])
        .collect();
    Ok((args, input))
}

pub(super) fn no_lazy_fetch(repo: &Path) -> Result<Vec<&'static str>, String> {
    let probe = bounded::run(
        repo,
        &["--no-lazy-fetch", "rev-parse", "--git-dir"],
        vec![],
        HEADER_LIMIT,
    );
    no_lazy_compatibility(probe, || {
        super::run_ok_codes(
            Some(repo),
            &[
                "config",
                "--get-regexp",
                "^(extensions\\.partialclone|remote\\..*\\.promisor)$",
            ],
            &[0, 1],
        )
    })
}

fn no_lazy_compatibility(
    probe: Result<Vec<u8>, String>,
    configured: impl FnOnce() -> Result<String, String>,
) -> Result<Vec<&'static str>, String> {
    match probe {
        Ok(_) => Ok(vec!["--no-lazy-fetch"]),
        // Git 2.36 supports bundles but predates this global option. Only an ordinary
        // repository with no promisor/partial-clone configuration may safely fall back.
        Err(error)
            if error
                .lines()
                .any(|line| line.trim() == "unknown option: --no-lazy-fetch") =>
        {
            if !configured()?.trim().is_empty() {
                return Err(crate::i18n::text("backend-this-git-cannot-disable-lazy-fetching-in-a-repository-with-promis-0cc400").into());
            }
            Ok(Vec::new())
        }
        Err(error) => Err(crate::i18n::message(
            "backend-could-not-establish-read-only-bundle-verification-error-f85b4e",
            &[("detail", (error).to_string().into())],
        )),
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod recording_import_tests;

mod storage;
use storage::BundleFile;
pub(crate) use storage::bundle_digest;
pub(crate) use storage::bundle_git_path;
pub use storage::read_bundle_file;
pub use storage::save_bundle_file;
#[cfg(test)]
use storage::windows_bundle_path;

mod header;
use header::HEADER_LIMIT;
use header::REF_LIMIT;
use header::header;
use header::valid_oid;
use header::valid_ref;
