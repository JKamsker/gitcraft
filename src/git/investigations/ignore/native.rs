//! Native ignore-file snapshots and guarded append operations.

use super::*;

#[cfg(any(unix, windows))]
pub(super) fn stamp(file: &std::fs::File, bytes: Vec<u8>) -> Result<Snapshot, String> {
    let meta = file.metadata().map_err(|e| e.to_string())?;
    let (identity, links) = recovery::native_file_identity(file)?;
    if !meta.is_file() || links != 1 {
        return Err(crate::i18n::text(
            "backend-ignore-edits-refuse-links-hardlinks-and-special-files-c1ea13",
        )
        .into());
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if meta.file_attributes() & 0x400 != 0 {
            return Err(crate::i18n::text("backend-ignore-file-is-a-reparse-point-0d69da").into());
        }
    }
    let time = meta.modified().map_err(|e| e.to_string())?;
    let modified = match time.duration_since(std::time::UNIX_EPOCH) {
        Ok(d) => d.as_nanos().to_string(),
        Err(e) => format!("-{}", e.duration().as_nanos()),
    };
    let result = Snapshot {
        bytes: Some(B64.encode(bytes)),
        identity: Some(identity),
        modified: Some(modified),
    };
    result.decoded()?;
    Ok(result)
}

#[cfg(any(unix, windows))]
pub(super) fn read_open(file: &mut std::fs::File) -> Result<Snapshot, String> {
    let before = stamp(file, vec![])?;
    let mut bytes = Vec::new();
    Read::by_ref(file)
        .take(LIMIT as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    let after = stamp(file, bytes)?;
    if before.identity != after.identity || before.modified != after.modified {
        return Err(crate::i18n::text("backend-ignore-file-changed-while-reading-b1e7ae").into());
    }
    Ok(after)
}

#[cfg(unix)]
pub(super) fn local_snapshot(
    identity: &Identity,
) -> Result<(Snapshot, Option<Arc<std::fs::File>>), String> {
    use rustix::fs::{Mode, OFlags, openat};
    let pin = identity
        .root_pin
        .as_ref()
        .ok_or(crate::i18n::text("backend-missing-root-pin-9ed8c4"))?;
    let (dir, name) = recovery::pinned_parent(
        Path::new(&pin.physical),
        Path::new(&identity.common),
        NAME,
        pin,
    )?;
    let fd = match openat(
        &dir,
        name.as_str(),
        OFlags::RDONLY | OFlags::CLOEXEC | OFlags::NOFOLLOW | OFlags::NONBLOCK,
        Mode::empty(),
    ) {
        Ok(fd) => fd,
        Err(rustix::io::Errno::NOENT) => return Ok((Snapshot::missing(), None)),
        Err(e) => return Err(e.to_string()),
    };
    let mut file = std::fs::File::from(fd);
    let state = read_open(&mut file)?;
    Ok((state, Some(Arc::new(file))))
}

#[cfg(windows)]
pub(super) fn local_snapshot(
    identity: &Identity,
) -> Result<(Snapshot, Option<Arc<std::fs::File>>), String> {
    use std::os::windows::fs::OpenOptionsExt;
    let pin = identity
        .root_pin
        .as_ref()
        .ok_or(crate::i18n::text("backend-missing-root-pin-9ed8c4"))?;
    let (_parents, path) = recovery::pinned_parent(
        Path::new(&pin.physical),
        Path::new(&identity.common),
        NAME,
        pin,
    )?;
    let mut file = match std::fs::OpenOptions::new()
        .read(true)
        .share_mode(3)
        .custom_flags(0x0020_0000)
        .open(path)
    {
        Ok(f) => f,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Ok((Snapshot::missing(), None));
        }
        Err(e) => return Err(e.to_string()),
    };
    let state = read_open(&mut file)?;
    Ok((state, Some(Arc::new(file))))
}

#[cfg(any(unix, windows))]
pub(super) fn append_open(
    mut file: std::fs::File,
    expected: &Snapshot,
    commit: &str,
    existing: bool,
) -> Result<Appended, String> {
    if existing {
        let current = read_open(&mut file)?;
        if &current != expected {
            return Err(crate::i18n::text(
                "backend-ignore-file-changed-since-preview-prepare-again-073e15",
            )
            .into());
        }
        if contains(&current.decoded()?.unwrap_or_default(), commit) {
            return Ok(Appended::AlreadyPresent);
        }
    }
    let bytes = payload(commit);
    let count = file.write(&bytes).map_err(|e| {
        crate::i18n::message(
            "backend-ignore-append-may-be-incomplete-existing-bytes-were-preserved-e-5fa2c5",
            &[("detail", (e).to_string().into())],
        )
    })?;
    if count != bytes.len() {
        return Err(crate::i18n::text(
            "backend-ignore-append-was-partial-inspect-the-file-before-retrying-existi-a0ee9e",
        )
        .into());
    }
    file.sync_all().map_err(|e| {
        crate::i18n::message(
            "backend-entry-was-appended-but-durability-is-uncertain-inspect-before-ret-b2f100",
            &[("detail", (e).to_string().into())],
        )
    })?;
    Ok(Appended::Changed)
}

#[cfg(unix)]
pub(super) fn local_append(
    identity: &Identity,
    expected: &Snapshot,
    commit: &str,
    active: &dyn Fn() -> bool,
) -> Result<Appended, String> {
    use rustix::fs::{Mode, OFlags, openat};
    let pin = identity
        .root_pin
        .as_ref()
        .ok_or(crate::i18n::text("backend-missing-root-pin-9ed8c4"))?;
    let (dir, name) = recovery::pinned_parent(
        Path::new(&pin.physical),
        Path::new(&identity.common),
        NAME,
        pin,
    )?;
    if !active() {
        return Err("Ignore edit cancelled before opening".into());
    }
    let exists = expected.bytes.is_some();
    let flags = OFlags::CLOEXEC
        | OFlags::NOFOLLOW
        | OFlags::NONBLOCK
        | OFlags::APPEND
        | if exists {
            OFlags::RDWR
        } else {
            OFlags::WRONLY | OFlags::CREATE | OFlags::EXCL
        };
    let fd = openat(&dir, name.as_str(), flags, Mode::from_bits_truncate(0o600))
        .map_err(|e| e.to_string())?;
    append_open(std::fs::File::from(fd), expected, commit, exists)
}

#[cfg(windows)]
pub(super) fn local_append(
    identity: &Identity,
    expected: &Snapshot,
    commit: &str,
    active: &dyn Fn() -> bool,
) -> Result<Appended, String> {
    use std::os::windows::fs::OpenOptionsExt;
    let pin = identity
        .root_pin
        .as_ref()
        .ok_or(crate::i18n::text("backend-missing-root-pin-9ed8c4"))?;
    let (_parents, path) = recovery::pinned_parent(
        Path::new(&pin.physical),
        Path::new(&identity.common),
        NAME,
        pin,
    )?;
    if !active() {
        return Err("Ignore edit cancelled before opening".into());
    }
    let exists = expected.bytes.is_some();
    let mut options = std::fs::OpenOptions::new();
    options
        .append(true)
        .read(exists)
        .create_new(!exists)
        .share_mode(3)
        .custom_flags(0x0020_0000);
    let file = options.open(path).map_err(|e| e.to_string())?;
    append_open(file, expected, commit, exists)
}

#[cfg(not(any(unix, windows)))]
pub(super) fn local_snapshot(
    _: &Identity,
) -> Result<(Snapshot, Option<Arc<std::fs::File>>), String> {
    Err(
        crate::i18n::text("backend-safe-ignore-file-edits-are-unsupported-on-this-platform-da2200")
            .into(),
    )
}

#[cfg(not(any(unix, windows)))]
pub(super) fn local_append(
    _: &Identity,
    _: &Snapshot,
    _: &str,
    _: &dyn Fn() -> bool,
) -> Result<Appended, String> {
    Err(
        crate::i18n::text("backend-safe-ignore-file-edits-are-unsupported-on-this-platform-da2200")
            .into(),
    )
}
