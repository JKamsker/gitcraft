//! Preserve access metadata through safe platform APIs; refuse failures before moving MERGED.
use super::{directory::Directory, files::Snapshot};
use std::{ffi::OsStr, fs::File};

pub(super) fn candidate(
    dir: &Directory,
    parent: &Directory,
    name: &OsStr,
    original: Option<&Snapshot>,
    bytes: &[u8],
) -> Result<File, String> {
    #[cfg(windows)]
    if let Some(original) = original {
        // Deny write/delete while native CopyFile copies the stable source's ACL,
        // attributes and streams. No unreviewed FFI or lossy permission reconstruction.
        let pinned = Snapshot::from_file(
            &parent.path.join(name),
            parent
                .stable_source(name)
                .map_err(|e| format!("Close other writers before saving: {e}"))?,
            &|| true,
        )?;
        if pinned.handle != original.handle
            || pinned.stamp != original.stamp
            || pinned.bytes != original.bytes
        {
            return Err("MERGED changed before metadata preparation".into());
        }
        std::fs::copy(parent.path.join(name), dir.path.join("replacement")).map_err(|e| {
            format!("Could not preserve the output's native security metadata: {e}")
        })?;
        return dir
            .rewrite_owned(OsStr::new("replacement"), bytes, original)
            .map_err(|e| e.to_string());
    }
    #[cfg(target_vendor = "apple")]
    if let Some(original) = original {
        let source = original
            .handle
            .as_ref()
            .ok_or("No source identity")?
            .as_file();
        rustix::fs::fclonefileat(
            source,
            dir.file(),
            "replacement",
            rustix::fs::CloneFlags::NOOWNERCOPY,
        )
        .map_err(|e| {
            format!("This filesystem cannot safely clone the output's ACL/metadata: {e}")
        })?;
        let file = dir
            .rewrite_owned(OsStr::new("replacement"), bytes, original)
            .map_err(|e| e.to_string())?;
        unix_mode(&file, source)?;
        return Ok(file);
    }
    #[cfg(not(windows))]
    let _ = (parent, name);
    let file = dir
        .create(OsStr::new("replacement"), bytes)
        .map_err(|e| e.to_string())?;
    if let Some(original) = original {
        let source = original
            .handle
            .as_ref()
            .ok_or("No source identity")?
            .as_file();
        #[cfg(unix)]
        unix_mode(&file, source)?;
        #[cfg(any(target_os = "linux", target_os = "android"))]
        copy_xattrs(source, &file)?;
        #[cfg(not(any(target_os="linux",target_os="android",windows,target_vendor="apple")))]
        return Err("Safe preservation of this platform's access metadata is unavailable; the original output was not changed".into());
        #[cfg(windows)]
        let _ = source;
    }
    Ok(file)
}
#[cfg(unix)]
fn unix_mode(to: &File, from: &File) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    let mode = from
        .metadata()
        .map_err(|e| e.to_string())?
        .permissions()
        .mode()
        & 0o777;
    to.set_permissions(std::fs::Permissions::from_mode(mode))
        .map_err(|e| e.to_string())
}
#[cfg(any(target_os = "linux", target_os = "android"))]
fn copy_xattrs(from: &File, to: &File) -> Result<(), String> {
    use rustix::fs::{
        XattrFlags, fgetxattr, fgetxattr_query, flistxattr, flistxattr_query, fremovexattr,
        fsetxattr,
    };
    const LIMIT: usize = 1024 * 1024;
    let size = match flistxattr_query(from) {
        Ok(size) => size,
        Err(rustix::io::Errno::OPNOTSUPP) => {
            return Err("This filesystem cannot expose the output's access metadata safely; the original was not changed".into());
        }
        Err(e) => return Err(e.to_string()),
    };
    if size > LIMIT {
        return Err("Output access metadata exceeds 1 MiB; the original was not changed".into());
    }
    let mut names = vec![0; size];
    let read = flistxattr(from, &mut names[..]).map_err(|e| e.to_string())?;
    names.truncate(read);
    // A directory's newer default ACL must not grant access absent from the source.
    if !names
        .split(|b| *b == 0)
        .any(|n| n == b"system.posix_acl_access")
    {
        match fgetxattr_query(to, "system.posix_acl_access") {
            Ok(_) => fremovexattr(to, "system.posix_acl_access").map_err(|e| e.to_string())?,
            Err(rustix::io::Errno::NODATA | rustix::io::Errno::OPNOTSUPP) => {}
            Err(e) => return Err(e.to_string()),
        }
    }
    let mut total = size;
    for name in names.split(|b| *b == 0).filter(|s| !s.is_empty()) {
        // Edited contents must not regain privileged capabilities or stale integrity signatures.
        if matches!(
            name,
            b"security.capability" | b"security.ima" | b"security.evm"
        ) {
            continue;
        }
        let size = fgetxattr_query(from, name).map_err(|e| e.to_string())?;
        total = total.checked_add(size).ok_or("Output metadata overflow")?;
        if total > LIMIT {
            return Err(
                "Output access metadata exceeds 1 MiB; the original was not changed".into(),
            );
        }
        let mut value = vec![0; size];
        let read = fgetxattr(from, name, &mut value[..]).map_err(|e| e.to_string())?;
        value.truncate(read);
        let same = match fgetxattr_query(to, name) {
            Ok(size) if size <= LIMIT => {
                let mut old = vec![0; size];
                fgetxattr(to, name, &mut old[..]).is_ok_and(|n| old[..n] == value)
            }
            _ => false,
        };
        if !same {
            fsetxattr(to, name, &value, XattrFlags::empty())
                .map_err(|e| format!("Could not preserve output access metadata: {e}"))?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #[cfg(any(target_os = "linux", target_os = "android"))]
    #[test]
    fn edited_results_keep_bounded_user_metadata_and_executable_mode_without_privileged_bits()
    -> Result<(), String> {
        use std::os::unix::fs::PermissionsExt;
        let temp = crate::git::file_comparison::Scratch::new()?;
        let path = temp.0.join("result");
        std::fs::write(&path, b"old").map_err(|e| e.to_string())?;
        let source = std::fs::File::open(&path).map_err(|e| e.to_string())?;
        rustix::fs::fsetxattr(
            &source,
            "user.gitcraft-fixture",
            b"metadata\0bytes",
            rustix::fs::XattrFlags::empty(),
        )
        .map_err(|e| e.to_string())?;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o4755))
            .map_err(|e| e.to_string())?;
        let output = crate::standalone::save::Output::capture(&path, &[])?;
        let saved = output.save(b"new", &[])?;
        let file = std::fs::File::open(&path).map_err(|e| e.to_string())?;
        let mut value = [0u8; 32];
        let count = rustix::fs::fgetxattr(&file, "user.gitcraft-fixture", &mut value[..])
            .map_err(|e| e.to_string())?;
        assert_eq!(&value[..count], b"metadata\0bytes");
        assert_eq!(
            file.metadata()
                .map_err(|e| e.to_string())?
                .permissions()
                .mode()
                & 0o7777,
            0o755
        );
        assert_eq!(
            std::fs::read(saved.previous.ok_or("No retained original")?)
                .map_err(|e| e.to_string())?,
            b"old"
        );
        Ok(())
    }
}
