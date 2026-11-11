//! Bundle byte storage, private temporary files, and platform path conversion.

use super::*;

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

pub(crate) fn bundle_digest(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

pub(super) struct BundleFile {
    pub(super) path: String,
    pub(super) local_dir: Option<PathBuf>,
    pub(super) target: Option<remote::Target>,
}

impl BundleFile {
    pub(super) fn new(repo: &Path, bytes: &[u8]) -> Result<Self, String> {
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
    pub(super) fn local(root: &Path, bytes: &[u8]) -> Result<Self, String> {
        static SEQ: AtomicU64 = AtomicU64::new(0);
        for _ in 0..100 {
            let dir = root.join(format!(
                "gitcraft-bundle-{}-{}",
                std::process::id(),
                SEQ.fetch_add(1, Ordering::Relaxed)
            ));
            let builder = std::fs::DirBuilder::new();
            #[cfg(unix)]
            let builder = {
                use std::os::unix::fs::DirBuilderExt;
                let mut builder = builder;
                builder.mode(0o700);
                builder
            };
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
pub(super) fn windows_bundle_path(native: &str) -> Result<String, String> {
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
