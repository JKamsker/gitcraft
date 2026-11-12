//! Durable owned outputs and evidence for files this restore did not write.
//! Evidence is bound to content, attributes, write time and filesystem identity.
//! Unknown/changed evidence never authorizes omitting an existing-file flush.
use super::*;
use sha2::{Digest, Sha256};

#[derive(Serialize, Deserialize, PartialEq, Eq)]
struct Stamp {
    version: u32,
    digest: Vec<u8>,
    identity: Vec<u64>,
    modified: std::time::SystemTime,
    attributes: u32,
}

fn stamp(path: &Path) -> Result<Stamp, String> {
    let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let meta = file.metadata().map_err(|e| e.to_string())?;
    if !meta.is_file()
        || std::fs::symlink_metadata(path)
            .map_err(|e| e.to_string())?
            .is_symlink()
    {
        return Err("Durability evidence requires a regular file".into());
    }
    #[cfg(unix)]
    let (identity, attributes) = {
        use std::os::unix::fs::MetadataExt;
        (vec![meta.dev(), meta.ino()], meta.mode())
    };
    #[cfg(windows)]
    let (identity, attributes) = windows_identity(&file, &meta)?;
    #[cfg(not(any(unix, windows)))]
    return Err("File identity evidence is unsupported on this platform".into());
    let mut hasher = Sha256::new();
    use std::io::Read;
    let mut reader = &file;
    let mut buffer = [0u8; 16384];
    loop {
        let n = reader.read(&mut buffer).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        hasher.update(&buffer[..n]);
    }
    let result = Stamp {
        version: 1,
        digest: hasher.finalize().to_vec(),
        identity,
        modified: meta.modified().map_err(|e| e.to_string())?,
        attributes,
    };
    // Reject concurrent writes observed while hashing, rather than blessing a mixed image.
    let after = file.metadata().map_err(|e| e.to_string())?;
    if meta.modified().ok() != after.modified().ok() || meta.len() != after.len() {
        return Err("File changed while recording durability evidence".into());
    }
    Ok(result)
}

// Keep the native API boundary separate from hashing and receipt policy.
#[cfg(windows)]
#[allow(unsafe_code)] // GetFileInformationByHandleEx requires a live handle and typed output.
fn windows_identity(
    file: &std::fs::File,
    meta: &std::fs::Metadata,
) -> Result<(Vec<u64>, u32), String> {
    use std::os::windows::{fs::MetadataExt, io::AsRawHandle};
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_ID_INFO, FileIdInfo, GetFileInformationByHandleEx,
    };
    let mut info = FILE_ID_INFO::default();
    // SAFETY: file retains ownership of its live handle for this synchronous call.
    // info is a valid writable FILE_ID_INFO, with exactly the size passed below;
    // FileIdInfo selects that output type. No pointer is retained by the API.
    if unsafe {
        GetFileInformationByHandleEx(
            file.as_raw_handle(),
            FileIdInfo,
            (&mut info as *mut FILE_ID_INFO).cast(),
            std::mem::size_of::<FILE_ID_INFO>() as u32,
        )
    } == 0
    {
        return Err(std::io::Error::last_os_error().to_string());
    }
    // Use the full 128-bit ID (ReFS does not guarantee unique 64-bit IDs).
    if info.FileId.Identifier == [0; 16] {
        return Err("Filesystem did not provide a stable file identity".into());
    }
    let mut identity = vec![info.VolumeSerialNumber, meta.creation_time()];
    for chunk in info.FileId.Identifier.as_chunks::<8>().0 {
        identity.push(u64::from_le_bytes(*chunk));
    }
    Ok((identity, meta.file_attributes()))
}

fn receipt(dir: &Path, key: &str) -> PathBuf {
    let digest = Sha256::digest(key.replace('\\', "/").as_bytes());
    let name: String = digest.iter().map(|b| format!("{b:02x}")).collect();
    dir.join("durable-v1").join(format!("{name}.json"))
}

// Owned outputs must be flushed before recording their pre-publication evidence.
pub(super) fn remember(dir: &Path, key: &str, path: &Path) -> Result<(), String> {
    let proof = stamp(path)?;
    let destination = receipt(dir, key);
    std::fs::create_dir_all(destination.parent().ok_or("Missing evidence parent")?)
        .map_err(|e| e.to_string())?;
    let bytes = serde_json::to_vec(&proof).map_err(|e| e.to_string())?;
    journal::atomic_write(&destination, &bytes)?;
    journal::sync_ancestors(destination.parent().ok_or("Missing evidence parent")?)?;
    effects::boundary("output-evidence-durable")
}

pub(super) fn proven(dir: &Path, key: &str, path: &Path) -> Result<bool, String> {
    let bytes = match std::fs::read(receipt(dir, key)) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(e) => return Err(e.to_string()),
    };
    let proof: Stamp = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
    Ok(proof.version == 1 && proof == stamp(path)?)
}

/// Only the newly copied, transaction-owned output may have READONLY cleared.
/// CopyFileEx (via fs::copy) retains attributes and alternate streams. Restore the
/// exact permissions before flushing through the acquired writable handle. A denied
/// ACL/open/attribute update/flush propagates; live sources are never chmod'd.
#[allow(clippy::permissions_set_readonly_false)] // Only an owned output; exact permissions restored.
pub(super) fn copy(source: &Path, output: &Path, dir: &Path, key: &str) -> Result<(), String> {
    if output.try_exists().map_err(|e| e.to_string())? {
        return Err(format!(
            "Owned recovery output already exists: {}",
            output.display()
        ));
    }
    std::fs::copy(source, output).map_err(|e| e.to_string())?;
    if windows_policy() {
        let permissions = std::fs::metadata(output)
            .map_err(|e| e.to_string())?
            .permissions();
        if permissions.readonly() {
            let mut writable = permissions.clone();
            writable.set_readonly(false);
            std::fs::set_permissions(output, writable).map_err(|e| e.to_string())?;
        }
        let opened = std::fs::OpenOptions::new().write(true).open(output);
        // Restore even on open failure. Attribute restoration failure is itself fatal.
        std::fs::set_permissions(output, permissions).map_err(|e| e.to_string())?;
        opened
            .and_then(|f| f.sync_all())
            .map_err(|e| e.to_string())?;
    } else {
        std::fs::File::open(output)
            .and_then(|f| f.sync_all())
            .map_err(|e| e.to_string())?;
    }
    effects::boundary("owned-output-synced")?;
    remember(dir, key, output)
}

pub(super) fn sync_live_file(dir: &Path, key: &str, path: &Path) -> Result<(), String> {
    if proven(dir, key, path)? {
        return Ok(());
    }
    journal::sync_file(path)
}

/// Capture before checkout/read-tree. Equality of bytes alone cannot prove a file
/// untouched: Git may have replaced it or rewritten equal bytes through filters.
/// This receipt only excuses the same readonly object with the same write time,
/// attributes and bytes. Restored owned outputs use their own pre-publication proof.
pub(super) fn capture_untouched(
    repo: &Path,
    gd: &Path,
    dir: &Path,
    parked: &Parked,
) -> Result<(), String> {
    let tracked = run(repo, &["ls-files", "--cached", "-z"])?;
    let paths = tracked
        .split('\0')
        .filter(|p| !p.is_empty())
        .map(|p| (false, p))
        .chain(std::iter::once((true, "index")))
        .chain(parked.state_files.iter().map(|p| (true, p.as_str())));
    for (git, path) in paths {
        let full = if git { gd } else { repo }.join(crate::mcp::args::repo_path(path)?);
        if std::fs::symlink_metadata(&full).is_ok_and(|m| m.is_file() && m.permissions().readonly())
        {
            remember(dir, &format!("live/{git}/{path}"), &full)?;
        }
    }
    effects::boundary("untouched-evidence-durable")
}

#[cfg(recovery_recording)]
thread_local! {
    pub(super) static WINDOWS_POLICY: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

pub(super) fn windows_policy() -> bool {
    #[cfg(recovery_recording)]
    if WINDOWS_POLICY.with(|value| value.get()) {
        return true;
    }
    cfg!(windows)
}

pub(super) fn attributes(path: &Path) -> Result<u32, String> {
    let meta = std::fs::metadata(path).map_err(|e| e.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        Ok(meta.mode())
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        Ok(meta.file_attributes())
    }
}

pub(super) fn restored_attributes(
    repo: &Path,
    gd: &Path,
    dir: &Path,
    parked: &Parked,
) -> Result<bool, String> {
    for (git, path) in parked
        .raw
        .iter()
        .map(|p| (false, p.as_str()))
        .chain(std::iter::once((true, "index")))
        .chain(parked.state_files.iter().map(|p| (true, p.as_str())))
    {
        let source = if git {
            dir.join(path)
        } else {
            dir.join("raw").join(path)
        };
        let destination = if git { gd } else { repo }.join(path);
        if attributes(&source)? != attributes(&destination)? {
            return Ok(false);
        }
    }
    Ok(true)
}
