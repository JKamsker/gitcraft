//! Offline exchange: immutable bundle previews and create-only atomic ref imports.
use std::collections::BTreeSet;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;
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
    /// Header and prerequisites checked by `git bundle verify`; the pack is checked on import.
    pub verification_error: Option<String>,
    /// Existing refs plus checked-out branch names, including unborn worktrees.
    pub existing_refs: BTreeSet<String>,
    bytes: Arc<Vec<u8>>,
}
#[derive(Clone, Debug)]
pub struct BundleImport {
    pub source: String,
    pub destination: String,
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
    let out = std::str::from_utf8(&out).map_err(|_| "Repository refs are not UTF-8")?;
    let mut refs = Vec::new();
    for line in out.lines() {
        let words: Vec<_> = line.split_whitespace().collect();
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
        return Err("Over 1000 refs; select a bundle with Git in the terminal".into());
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
        return Err("Choose between 1 and 1000 refs".into());
    }
    let mut seen = BTreeSet::new();
    for r in refs {
        valid_ref(repo, &r.name)?;
        if !valid_oid(&r.oid) || !seen.insert(&r.name) {
            return Err("Invalid or duplicate export ref".into());
        }
        let current = run(
            Some(repo),
            &["rev-parse", "--verify", "--end-of-options", &r.name],
        )?;
        if current.trim() != r.oid {
            return Err(format!(
                "{} moved since the export dialog opened; reopen it",
                r.name
            ));
        }
    }
    let base = base
        .filter(|s| !s.trim().is_empty())
        .map(|s| super::resolve(repo, s.trim()))
        .transpose()?;
    let mut args = vec![
        "bundle".to_owned(),
        "create".into(),
        "--version=3".into(),
        "-".into(),
    ];
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
        return Err("Refs moved while exporting, or the range excludes a selected ref; no bundle saved. Reopen the export dialog.".into());
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
        .ok_or("Bundle filename is not UTF-8")?;
    crate::safe_read::read_inside(parent, name, BUNDLE_LIMIT as u64)
        .ok_or_else(|| "Could not read a regular bundle file within 128 MiB (links and devices are not followed); use Git in the terminal for larger bundles".into())
}

/// Never overwrite an existing file, even if it appeared after the save dialog.
pub fn save_bundle_file(file: &Path, bytes: &[u8]) -> Result<(), String> {
    if bytes.len() > BUNDLE_LIMIT {
        return Err("Bundle exceeds 128 MiB".into());
    }
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut f = options.open(file).map_err(|e| {
        format!(
            "Could not create {} (existing files are never overwritten): {e}",
            file.display()
        )
    })?;
    if let Err(e) = f.write_all(bytes).and_then(|_| f.sync_all()) {
        return Err(format!(
            "Could not finish bundle output: {e}. The newly created output may be incomplete."
        ));
    }
    Ok(())
}

pub fn inspect_bundle(repo: &Path, bytes: Vec<u8>) -> Result<BundlePreview, String> {
    let (refs, prerequisites) = header(&bytes)?;
    for r in &refs {
        valid_ref(repo, &r.name)?;
    }
    let temp = BundleFile::new(repo, &bytes)?;
    let verification_error = verify(repo, &temp.path, &refs, &bytes).err();
    let existing = bounded::run(
        repo,
        &["for-each-ref", "--format=%(refname)"],
        vec![],
        HEADER_LIMIT,
    )?;
    let mut existing_refs: BTreeSet<String> = std::str::from_utf8(&existing)
        .map_err(|_| "Repository refs are not UTF-8")?
        .lines()
        .map(str::to_owned)
        .collect();
    existing_refs.extend(checked_out_refs(repo)?);
    Ok(BundlePreview {
        refs,
        prerequisites,
        verification_error,
        existing_refs,
        bytes: Arc::new(bytes),
    })
}

pub fn import_bundle(
    repo: &Path,
    preview: &BundlePreview,
    imports: &[BundleImport],
) -> Result<(), String> {
    if imports.is_empty() || imports.len() > REF_LIMIT {
        return Err("Choose at least one ref to import".into());
    }
    // Reparse immutable bytes, not a potentially replaced file or edited preview fields.
    let (refs, _) = header(&preview.bytes)?;
    let checked_out = checked_out_refs(repo)?;
    let mut destinations = BTreeSet::new();
    let mut transaction = String::from("start\n");
    for i in imports {
        if !["refs/heads/", "refs/tags/", "refs/bundles/"]
            .iter()
            .any(|p| i.destination.starts_with(p))
        {
            return Err("Import destinations must be new refs/heads/…, refs/tags/… or refs/bundles/… refs; HEAD and special namespaces cannot change".into());
        }
        valid_ref(repo, &i.destination)?;
        if checked_out.contains(&i.destination) {
            return Err(format!(
                "{} is checked out in a worktree; choose a different destination",
                i.destination
            ));
        }
        if !destinations.insert(&i.destination) {
            return Err("Duplicate destination refs".into());
        }
        let source = refs
            .iter()
            .find(|r| r.name == i.source)
            .ok_or("Selected source is not in this bundle")?;
        transaction.push_str(&format!("create {} {}\n", i.destination, source.oid));
    }
    transaction.push_str("prepare\ncommit\n");
    let temp = BundleFile::new(repo, &preview.bytes)?;
    let no_lazy = verify(repo, &temp.path, &refs, &preview.bytes)?;
    // `bundle unbundle` does not honor fetch/transfer.fsckObjects. Feed only the pack
    // to strict index-pack so malformed objects, not only bad checksums, stop the import.
    let pack_at = preview
        .bytes
        .windows(2)
        .position(|s| s == b"\n\n")
        .ok_or("No bundle pack")?
        + 2;
    let pack_args: Vec<_> = no_lazy
        .iter()
        .copied()
        .chain([
            "index-pack",
            "--stdin",
            "--fix-thin",
            "--strict",
            "--max-input-size=134217728",
        ])
        .collect();
    bounded::run(
        repo,
        &pack_args,
        preview.bytes[pack_at..].to_vec(),
        HEADER_LIMIT,
    )?;
    let checked_out = checked_out_refs(repo)?;
    if imports.iter().any(|i| checked_out.contains(&i.destination)) {
        return Err("A destination became checked out while reading the bundle; choose different destination refs".into());
    }
    let ref_args: Vec<_> = no_lazy
        .iter()
        .copied()
        .chain(["update-ref", "--no-deref", "--stdin"])
        .collect();
    bounded::run(repo, &ref_args, transaction.into_bytes(), HEADER_LIMIT)?;
    Ok(())
}

fn checked_out_refs(repo: &Path) -> Result<BTreeSet<String>, String> {
    let out = bounded::run(
        repo,
        &["worktree", "list", "--porcelain", "-z"],
        vec![],
        HEADER_LIMIT,
    )?;
    let out = std::str::from_utf8(&out).map_err(|_| "Worktree paths are not UTF-8")?;
    Ok(out
        .split('\0')
        .filter_map(|line| line.strip_prefix("branch "))
        .map(str::to_owned)
        .collect())
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
        _ => return Err("Unsupported repository object format".into()),
    };
    if refs.iter().any(|r| r.oid.len() != expected_len) {
        return Err(format!(
            "Bundle object format does not match this {} repository",
            format.trim()
        ));
    }
    // Verification resolves prerequisite commits, which otherwise triggers a promisor
    // fetch and modifies the object store before the user chooses Import.
    let no_lazy = no_lazy_fetch(repo)?;
    let (args, input) = verification_command(&no_lazy, file, bytes)?;
    bounded::run(repo, &args, input, HEADER_LIMIT)?;
    Ok(no_lazy)
}

/// Git 2.45 added --no-lazy-fetch, after bundle verification gained stdin support.
/// Use that existing capability probe to retain the older ordinary-repository fallback.
fn verification_command<'a>(
    no_lazy: &[&'a str],
    file: &'a str,
    bytes: &[u8],
) -> Result<(Vec<&'a str>, Vec<u8>), String> {
    let (file, input) = if no_lazy.is_empty() {
        (file, Vec::new())
    } else {
        // Native Git for Windows opens bundle files before initializing long-path config.
        // Streaming the exact validated header avoids that pathname limit. Do not send the
        // pack: verify closes stdin after the header, leaving a large writer with EPIPE.
        header(bytes)?;
        let end = bytes
            .windows(2)
            .position(|b| b == b"\n\n")
            .ok_or("No bundle header")?
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

fn no_lazy_fetch(repo: &Path) -> Result<Vec<&'static str>, String> {
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
                return Err("This Git cannot disable lazy fetching in a repository with promisor/partial-clone configuration. Upgrade Git before inspecting or importing this bundle; prerequisite commits will not be fetched automatically.".into());
            }
            Ok(Vec::new())
        }
        Err(error) => Err(format!(
            "Could not establish read-only bundle verification: {error}"
        )),
    }
}

pub fn bundle_destination(source: &str) -> String {
    if source == "HEAD" {
        "refs/heads/bundle/HEAD".into()
    } else if let Some(name) = source.strip_prefix("refs/heads/") {
        format!("refs/heads/bundle/{name}")
    } else if let Some(name) = source.strip_prefix("refs/tags/") {
        format!("refs/tags/bundle/{name}")
    } else {
        format!(
            "refs/bundles/{}",
            source.strip_prefix("refs/").unwrap_or(source)
        )
    }
}

fn valid_oid(oid: &str) -> bool {
    matches!(oid.len(), 40 | 64)
        && oid.bytes().all(|b| b.is_ascii_hexdigit())
        && oid.bytes().any(|b| b != b'0')
}
fn valid_ref(repo: &Path, name: &str) -> Result<(), String> {
    if name == "HEAD" {
        return Ok(());
    }
    if name.len() > 1024 || !name.starts_with("refs/") || name.chars().any(char::is_control) {
        return Err("Bundle ref must be a full refs/… name without control characters".into());
    }
    // Ref syntax is repository-independent. Avoid a remote shell round trip per ref,
    // while keeping Practice independent from the active profile's config/error state.
    let checked = run(Some(repo), &["check-ref-format", name]);
    checked
        .map(|_| ())
        .map_err(|_| format!("Invalid bundle ref: {name}"))
}

fn header(bytes: &[u8]) -> Result<(Vec<BundleRef>, Vec<BundleRef>), String> {
    if bytes.len() > BUNDLE_LIMIT {
        return Err("Bundle exceeds 128 MiB".into());
    }
    let mut pos = 0;
    let mut next = || -> Result<&str, String> {
        let end = bytes
            .get(pos..)
            .and_then(|b| b.iter().position(|c| *c == b'\n'))
            .ok_or("Incomplete bundle header")?
            + pos;
        if end > HEADER_LIMIT {
            return Err("Bundle header exceeds 1 MiB".into());
        }
        let line =
            std::str::from_utf8(&bytes[pos..end]).map_err(|_| "Bundle header is not UTF-8")?;
        pos = end + 1;
        Ok(line)
    };
    let version = next()?;
    if !matches!(version, "# v2 git bundle" | "# v3 git bundle") {
        return Err("Not a supported Git v2/v3 bundle".into());
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
                return Err("Misplaced bundle capability".into());
            }
            hash_len = match capability {
                "object-format=sha1" => 40,
                "object-format=sha256" => 64,
                _ => return Err(format!("Unsupported bundle capability: {capability}")),
            };
            continue;
        }
        let (prerequisite, line) = match line.strip_prefix('-') {
            Some(rest) => (true, rest),
            None => (false, line),
        };
        let (oid, name) = match line.split_once(' ') {
            Some(parts) => parts,
            None if prerequisite => (line, ""),
            None => return Err("Invalid bundle header entry".into()),
        };
        if !valid_oid(oid)
            || oid.len() != hash_len
            || (!prerequisite && (name.is_empty() || name.chars().any(char::is_control)))
        {
            return Err("Invalid bundle object ID or name".into());
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
            oid: oid.into(),
        };
        if prerequisite {
            prerequisites.push(r);
        } else {
            if !names.insert(name) {
                return Err("Duplicate advertised bundle ref".into());
            }
            refs.push(r);
        }
        if refs.len() + prerequisites.len() > REF_LIMIT {
            return Err("Bundle has more than 1000 refs/prerequisites".into());
        }
    }
    if refs.is_empty() || bytes.get(pos..pos + 4) != Some(b"PACK") {
        return Err("Bundle has no advertised refs or no pack data".into());
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
        Err("Could not create a private bundle temporary directory".into())
    }
}

fn bundle_git_path(path: &Path) -> Result<String, String> {
    let native = path.to_str().ok_or("Temporary path is not UTF-8")?;
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
                return Err("The temporary UNC path needs both a server and share".into());
            }
            return Ok(format!("//{network}"));
        }
        let bytes = path.as_bytes();
        if bytes.len() >= 3 && bytes[0].is_ascii_alphabetic() && bytes[1..3] == *b":/" {
            return Ok(path.into());
        }
        return Err("Git cannot open this temporary Windows device namespace".into());
    }
    if normalized.starts_with("//./") {
        return Err("Git cannot open this temporary Windows device namespace".into());
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
