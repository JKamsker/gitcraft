//! Offline export, read-only header inspection and explicitly confirmed contained import.
use std::collections::BTreeSet;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use super::{command::bounded, run};
use crate::remote;

pub const BUNDLE_LIMIT: usize = 128 * 1024 * 1024;
const HEADER_LIMIT: usize = 1024 * 1024;
const REF_LIMIT: usize = 1000;

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

/// File dialogs always select this machine's disk. Read once with a hard bound.
pub fn read_bundle_file(file: &Path) -> Result<Vec<u8>, String> {
    let parent = file
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let name = file
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or(crate::i18n::text(
            "backend-bundle-filename-is-not-utf-8-eb4b78",
        ))?;
    crate::safe_read::read_inside(parent, name, BUNDLE_LIMIT as u64).ok_or_else(|| {
        crate::i18n::text(
            "backend-could-not-read-a-regular-bundle-file-within-128-mib-links-and-dev-3127c1",
        )
        .into()
    })
}

/// Never overwrite an existing file, even if it appeared after the save dialog.
pub fn save_bundle_file(file: &Path, bytes: &[u8]) -> Result<(), String> {
    if crate::tasks::cancellation().is_cancelled() {
        return Err("cancelled".into());
    }
    if bytes.len() > BUNDLE_LIMIT {
        return Err(crate::i18n::text("backend-bundle-exceeds-128-mib-c505c5").into());
    }
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut f = options.open(file).map_err(|e| {
        crate::i18n::message(
            "backend-could-not-create-existing-files-are-never-overwritten-e-cbe7e2",
            &[
                ("path", (file.display()).to_string().into()),
                ("detail", (e).to_string().into()),
            ],
        )
    })?;
    if let Err(e) = f.write_all(bytes).and_then(|_| f.sync_all()) {
        return Err(crate::i18n::message(
            "backend-could-not-finish-bundle-output-e-the-newly-created-output-may-be-da3dda",
            &[("detail", (e).to_string().into())],
        ));
    }
    Ok(())
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

pub(crate) fn bundle_digest(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
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

fn valid_oid(oid: &str) -> bool {
    matches!(oid.len(), 40 | 64)
        && oid.bytes().all(|b| b.is_ascii_hexdigit())
        && oid.bytes().any(|b| b != b'0')
}
fn valid_ref(name: &str) -> Result<(), String> {
    // Git's full-ref grammar is repository-independent. Validate locally so an
    // inspection never needs one remote round trip per advertised ref.
    if name == "HEAD" {
        return Ok(());
    }
    let invalid = name.len() > 1024
        || !name.starts_with("refs/")
        || name.ends_with(['/', '.'])
        || name.contains("..")
        || name.contains("@{")
        || name
            .split('/')
            .any(|part| part.is_empty() || part.starts_with('.') || part.ends_with(".lock"))
        || name
            .bytes()
            .any(|b| b <= b' ' || b == 127 || b"~^:?*[\\".contains(&b));
    if invalid {
        Err(crate::i18n::message(
            "backend-invalid-bundle-ref-name-897fdb",
            &[("name", (name).to_string().into())],
        ))
    } else {
        Ok(())
    }
}

fn header(bytes: &[u8]) -> Result<(Vec<BundleRef>, Vec<BundleRef>), String> {
    if bytes.len() > BUNDLE_LIMIT {
        return Err(crate::i18n::text("backend-bundle-exceeds-128-mib-c505c5").into());
    }
    let mut pos = 0;
    let mut next = || -> Result<std::borrow::Cow<'_, str>, String> {
        let end = bytes
            .get(pos..)
            .and_then(|b| b.iter().position(|c| *c == b'\n'))
            .ok_or(crate::i18n::text("backend-incomplete-bundle-header-3f723b"))?
            + pos;
        if end > HEADER_LIMIT {
            return Err(crate::i18n::text("backend-bundle-header-exceeds-1-mib-868fea").into());
        }
        let line = &bytes[pos..end];
        pos = end + 1;
        if line.starts_with(b"-") {
            // Subjects can come from legacy-encoded commits; the OID is still validated.
            Ok(String::from_utf8_lossy(line))
        } else {
            std::str::from_utf8(line)
                .map(std::borrow::Cow::Borrowed)
                .map_err(|_| {
                    crate::i18n::text("backend-bundle-ref-header-is-not-utf-8-a72a69").to_owned()
                })
        }
    };
    let version = next()?;
    if !matches!(version.as_ref(), "# v2 git bundle" | "# v3 git bundle") {
        return Err(crate::i18n::text("backend-not-a-supported-git-v2-v3-bundle-d8d741").into());
    }
    let (mut refs, mut prerequisites) = (Vec::new(), Vec::new());
    let mut names = BTreeSet::new();
    let mut hash_len = 40;
    loop {
        let line = next()?;
        if line.is_empty() {
            break;
        }
        if let Some(capability) = line.strip_prefix('@') {
            if version != "# v3 git bundle" || !refs.is_empty() || !prerequisites.is_empty() {
                return Err(crate::i18n::text("backend-misplaced-bundle-capability-92bb13").into());
            }
            hash_len = match capability {
                "object-format=sha1" => 40,
                "object-format=sha256" => 64,
                _ => {
                    return Err(crate::i18n::message(
                        "backend-unsupported-bundle-capability-capability-644a2c",
                        &[("capability", (capability).to_string().into())],
                    ));
                }
            };
            continue;
        }
        let (prerequisite, line) = match line.strip_prefix('-') {
            Some(rest) => (true, rest),
            None => (false, line.as_ref()),
        };
        let (oid, name) = match line.split_once(' ') {
            Some(parts) => parts,
            None if prerequisite => (line, ""),
            None => {
                return Err(crate::i18n::text("backend-invalid-bundle-header-entry-a00585").into());
            }
        };
        if !valid_oid(oid)
            || oid.len() != hash_len
            || (!prerequisite && (name.is_empty() || name.chars().any(char::is_control)))
        {
            return Err(
                crate::i18n::text("backend-invalid-bundle-object-id-or-name-776557").into(),
            );
        }
        let r = BundleRef {
            // Prerequisite descriptions are commit subjects, not ref names. Empty
            // subjects and tabs are valid; sanitize controls only for presentation.
            name: if prerequisite {
                name.chars()
                    .map(|c| if c.is_control() { ' ' } else { c })
                    .collect()
            } else {
                name.into()
            },
            oid: oid.to_ascii_lowercase(),
        };
        if prerequisite {
            prerequisites.push(r);
        } else {
            if !names.insert(name.to_owned()) {
                return Err(
                    crate::i18n::text("backend-duplicate-advertised-bundle-ref-c6d994").into(),
                );
            }
            refs.push(r);
        }
        if refs.len() + prerequisites.len() > REF_LIMIT {
            return Err(crate::i18n::text(
                "backend-bundle-has-more-than-1000-refs-prerequisites-73ceb6",
            )
            .into());
        }
    }
    if refs.is_empty() || bytes.get(pos..pos + 4) != Some(b"PACK") {
        return Err(crate::i18n::text(
            "backend-bundle-has-no-advertised-refs-or-no-pack-data-71b03c",
        )
        .into());
    }
    Ok((refs, prerequisites))
}

struct BundleFile {
    path: String,
    local_dir: Option<PathBuf>,
    target: Option<remote::Target>,
}
impl BundleFile {
    fn new(repo: &Path, bytes: &[u8]) -> Result<Self, String> {
        if let Some(target) = remote::parse(repo) {
            let path = remote::host_temp_file(&target, "exchange.bundle", bytes)?;
            return Ok(Self {
                path,
                local_dir: None,
                target: Some(target),
            });
        }
        let root = std::fs::canonicalize(std::env::temp_dir()).map_err(|e| e.to_string())?;
        Self::local(&root, bytes)
    }
    fn local(root: &Path, bytes: &[u8]) -> Result<Self, String> {
        static SEQ: AtomicU64 = AtomicU64::new(0);
        for _ in 0..100 {
            let dir = root.join(format!(
                "gitcraft-bundle-{}-{}",
                std::process::id(),
                SEQ.fetch_add(1, Ordering::Relaxed)
            ));
            let mut builder = std::fs::DirBuilder::new();
            #[cfg(unix)]
            {
                use std::os::unix::fs::DirBuilderExt;
                builder.mode(0o700);
            }
            match builder.create(&dir) {
                Ok(()) => {
                    let mut temp = Self {
                        path: String::new(),
                        local_dir: Some(dir.clone()),
                        target: None,
                    };
                    let path = dir.join("exchange.bundle");
                    // Keep the native canonical PathBuf for file IO and cleanup.
                    // Only the argument passed to Git uses its portable spelling.
                    temp.path = bundle_git_path(&path)?;
                    OpenOptions::new()
                        .create_new(true)
                        .write(true)
                        .open(path)
                        .and_then(|mut f| f.write_all(bytes))
                        .map_err(|e| e.to_string())?;
                    return Ok(temp);
                }
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(e) => return Err(e.to_string()),
            }
        }
        Err(crate::i18n::text(
            "backend-could-not-create-a-private-bundle-temporary-directory-950ce3",
        )
        .into())
    }
}

pub(crate) fn bundle_git_path(path: &Path) -> Result<String, String> {
    let native = path.to_str().ok_or(crate::i18n::text(
        "backend-temporary-path-is-not-utf-8-a9bf46",
    ))?;
    #[cfg(windows)]
    return windows_bundle_path(native);
    #[cfg(not(windows))]
    Ok(native.to_owned())
}

/// Convert only recognized Win32 verbatim drive/UNC prefixes. Never remove a
/// generic device namespace, resolve dot components, or recanonicalize a link.
#[cfg(any(windows, test))]
fn windows_bundle_path(native: &str) -> Result<String, String> {
    let normalized = native.replace('\\', "/");
    if let Some(path) = normalized.strip_prefix("//?/") {
        if path
            .get(..4)
            .is_some_and(|p| p.eq_ignore_ascii_case("UNC/"))
        {
            let network = &path[4..];
            let mut parts = network.split('/');
            if parts.next().is_none_or(str::is_empty) || parts.next().is_none_or(str::is_empty) {
                return Err(crate::i18n::text(
                    "backend-the-temporary-unc-path-needs-both-a-server-and-share-9b955c",
                )
                .into());
            }
            return Ok(format!("//{network}"));
        }
        let bytes = path.as_bytes();
        if bytes.len() >= 3 && bytes[0].is_ascii_alphabetic() && bytes[1..3] == *b":/" {
            return Ok(path.into());
        }
        return Err(crate::i18n::text(
            "backend-git-cannot-open-this-temporary-windows-device-namespace-8b48e7",
        )
        .into());
    }
    if normalized.starts_with("//./") {
        return Err(crate::i18n::text(
            "backend-git-cannot-open-this-temporary-windows-device-namespace-8b48e7",
        )
        .into());
    }
    Ok(normalized)
}
impl Drop for BundleFile {
    fn drop(&mut self) {
        if let Some(dir) = &self.local_dir {
            let _ = std::fs::remove_dir_all(dir);
        }
        if let Some(target) = &self.target {
            let _ = remote::exec(
                &target.host,
                &remote::Request {
                    cwd: Some(target.path.clone()),
                    script: format!("rm -f -- {}", remote::sh_quote(&self.path)),
                    ..Default::default()
                },
            );
        }
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod recording_import_tests {
    use super::*;
    fn refs(names: &[&str]) -> Vec<BundleRef> {
        names
            .iter()
            .map(|n| BundleRef {
                name: (*n).into(),
                oid: "a".repeat(40),
            })
            .collect()
    }
    #[test]
    fn selected_refs_map_to_new_names_and_reject_stale_or_colliding_selection() -> Result<(), String>
    {
        let refs = refs(&["HEAD", "refs/heads/main", "refs/tags/v1"]);
        let selected = ["HEAD", "refs/tags/v1"]
            .map(str::to_owned)
            .into_iter()
            .collect();
        let mapped = import_mapping(&refs, &selected, "handoff_1")?;
        assert_eq!(
            mapped.iter().map(|r| r.name.as_str()).collect::<Vec<_>>(),
            [
                "refs/bundle-import/handoff_1/HEAD",
                "refs/bundle-import/handoff_1/tags/v1"
            ]
        );
        assert!(mapped.iter().all(|r| r.oid == "a".repeat(40)));
        for prefix in [
            "",
            "../main",
            "NEW",
            "a/b",
            "refs/heads/main",
            "x\ncreate other",
        ] {
            assert!(import_mapping(&refs, &selected, prefix).is_err());
        }
        assert!(import_mapping(&refs, &BTreeSet::new(), "new").is_err());
        assert!(
            import_mapping(
                &refs,
                &["refs/heads/stale".into()].into_iter().collect(),
                "new"
            )
            .is_err()
        );
        for names in [
            ["refs/heads/Main", "refs/heads/main"],
            ["refs/heads/a", "refs/heads/a/b"],
        ] {
            let colliding = self::refs(&names);
            let all = colliding.iter().map(|r| r.name.clone()).collect();
            assert!(import_mapping(&colliding, &all, "new").is_err());
        }
        Ok(())
    }
    #[test]
    fn held_snapshot_digest_binds_pack_bytes_not_only_advertised_header() -> Result<(), String> {
        let header = format!(
            "# v2 git bundle\n{} refs/heads/main\n\nPACK",
            "a".repeat(40)
        );
        let original = header.into_bytes();
        let mut altered = original.clone();
        altered.push(0);
        assert_eq!(self::header(&original)?, self::header(&altered)?);
        assert_ne!(bundle_digest(&original), bundle_digest(&altered));
        let (refs, prerequisites) = self::header(&altered)?;
        let preview = BundlePreview {
            refs,
            prerequisites,
            verification_error: None,
            bytes: std::sync::Arc::new(altered),
            digest: bundle_digest(&original),
            repository: "recording-only".into(),
        };
        // This pure production guard runs before any capability probe. No process-
        // capable backend exists in this test, including on a validation regression.
        assert!(checked_snapshot(&preview).is_err_and(|error| error.contains("snapshot changed")));
        assert_eq!(
            bundle_digest(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        Ok(())
    }
}
