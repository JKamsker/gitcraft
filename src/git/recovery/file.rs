//! New-file recovery only. Existing files, links, tracked paths and ignored names stay out.
use super::{Identity, Reader};
use std::io::Write;
use std::path::Path;

pub(super) fn valid_path(path: &str) -> Result<(), String> {
    if path.is_empty()
        || path.len() > 4096
        || path.starts_with('/')
        || path
            .chars()
            .any(|c| c.is_control() || matches!(c, '\\' | ':' | '<' | '>' | '"' | '|' | '?' | '*'))
    {
        return Err("Choose a portable repository-relative filename".into());
    }
    for part in path.split('/') {
        let folded:String=part.chars().filter(|c| !matches!(*c,'\u{200c}'..='\u{200f}'|'\u{202a}'..='\u{202e}'|'\u{2060}'..='\u{206f}'|'\u{feff}')).flat_map(char::to_lowercase).collect();
        let stem = folded.split('.').next().unwrap_or_default();
        if part.is_empty()
            || matches!(part, "." | "..")
            || part.ends_with(['.', ' '])
            || matches!(folded.as_str(), ".git" | "git~1" | ".git~1")
            || matches!(stem, "con" | "prn" | "aux" | "nul")
            || (stem.len() == 4
                && (stem.starts_with("com") || stem.starts_with("lpt"))
                && matches!(stem.as_bytes()[3], b'1'..=b'9'))
        {
            return Err("Destination crosses Git metadata or a reserved/ambiguous filename".into());
        }
    }
    Ok(())
}
fn root(identity: &Identity) -> Result<&str, String> {
    identity
        .root
        .as_deref()
        .ok_or_else(|| "File recovery needs a working repository".into())
}
pub(super) fn available(
    reader: &Reader<'_>,
    identity: &Identity,
    path: &str,
) -> Result<(), String> {
    valid_path(path)?;
    if let Some(host) = &identity.host {
        let out = crate::remote::exec(
            host,
            &crate::remote::Request {
                script: remote_script(identity, path, false)?,
                timeout: Some(std::time::Duration::from_secs(30)),
                long: true,
                ..Default::default()
            },
        )?;
        if !reader.active() {
            return Err("File preview cancelled".into());
        }
        if out.code == 0 && out.stdout == b"available\n" && out.stderr.is_empty() {
            Ok(())
        } else {
            Err("Destination exists, crosses a link, or has an unavailable parent".into())
        }
    } else {
        local_available(
            Path::new(root(identity)?),
            Path::new(&identity.common),
            path,
        )
    }
}
pub(super) fn create(
    reader: &Reader<'_>,
    identity: &Identity,
    path: &str,
    bytes: &[u8],
) -> Result<(), String> {
    valid_path(path)?;
    if bytes.len() > super::BLOB_LIMIT || !reader.active() {
        return Err("File recovery cancelled or exceeds 2 MiB".into());
    }
    if let Some(host) = &identity.host {
        // Direct, owned call in the mutation worker. It cannot release a queued next
        // operation while the current remote command is still connecting or running.
        let out = crate::remote::exec(
            host,
            &crate::remote::Request {
                script: remote_script(identity, path, true)?,
                stdin: Some(bytes.to_vec()),
                timeout: Some(std::time::Duration::from_secs(30)),
                long: true,
                ..Default::default()
            },
        )?;
        if out.code == 0 && out.stdout == b"created\n" && out.stderr.is_empty() {
            Ok(())
        } else {
            Err("New-file recovery failed safely. The destination must not exist; remote recovery requires GNU ln and Git metadata on the same filesystem. Existing files were not overwritten.".into())
        }
    } else {
        local_create(
            Path::new(root(identity)?),
            Path::new(&identity.common),
            path,
            bytes,
        )
    }
}
fn remote_script(identity: &Identity, path: &str, create: bool) -> Result<String, String> {
    use crate::remote::sh_quote;
    valid_path(path)?;
    let root = root(identity)?;
    if !root.starts_with('/') || !identity.common.starts_with('/') {
        return Err("Remote repository metadata must be absolute".into());
    }
    let parts: Vec<_> = path.split('/').collect();
    let (name, parents) = parts.split_last().ok_or("Empty destination")?;
    let mut script = format!(
        "umask 077\ncd -P -- {} || exit 1\nrecovery_root=$(pwd -P) || exit 1\n[ \"$recovery_root\" = / ] && recovery_root=\n",
        sh_quote(root)
    );
    script.push_str(&format!("recovery_metadata=$(cd -P -- {} && pwd -P) || exit 1\n[ \"$recovery_metadata\" = / ] && recovery_metadata=\n[ \"$recovery_root\" != \"$recovery_metadata\" ] || exit 1\n", sh_quote(&identity.common)));
    let mut relative = String::new();
    for parent in parents {
        relative.push('/');
        relative.push_str(parent);
        let parent = sh_quote(parent);
        script.push_str(&format!("[ -L {parent} ] && exit 1\ncd -P -- {parent} || exit 1\n[ \"$(pwd -P)\" = \"$recovery_root\"{} ] || exit 1\n[ \"$(pwd -P)\" != \"$recovery_metadata\" ] || exit 1\n",sh_quote(&relative)));
    }
    let name = sh_quote(name);
    script.push_str(&format!("[ ! -e {name} ] && [ ! -L {name} ] || exit 10\n"));
    if create {
        let template = sh_quote(&format!(
            "{}/gitcraft-content-recovery.XXXXXXXX",
            identity.common
        ));
        // A private Git-metadata scratch file avoids a partial or visible working
        // file. GNU ln -T performs create-only publication even if a directory/FIFO
        // races the final name; cross-filesystem publication refuses safely.
        script.push_str(&format!("recovery_tmp=$(mktemp -d -- {template}) || exit 1\ntrap 'rm -rf -- \"$recovery_tmp\"' 0\ntrap 'exit 1' HUP INT TERM PIPE\ncat >\"$recovery_tmp/blob\" || exit 1\nln -T -- \"$recovery_tmp/blob\" {name} || exit 1\nprintf 'created\\n'\n"));
    } else {
        script.push_str("printf 'available\\n'\n");
    }
    Ok(script)
}
#[cfg(unix)]
fn parent(root: &Path, common: &Path, path: &str) -> Result<(rustix::fd::OwnedFd, String), String> {
    use rustix::fs::{Mode, OFlags, fstat, open, openat};
    use std::os::unix::fs::MetadataExt;
    let parts: Vec<_> = path.split('/').collect();
    let (name, folders) = parts.split_last().ok_or("Empty destination")?;
    let flags = OFlags::RDONLY | OFlags::CLOEXEC | OFlags::DIRECTORY | OFlags::NOFOLLOW;
    let mut dir = open(root, flags - OFlags::NOFOLLOW, Mode::empty()).map_err(|e| e.to_string())?;
    let metadata = std::fs::metadata(common).map_err(|e| e.to_string())?;
    let root_stat = fstat(&dir).map_err(|e| e.to_string())?;
    if root_stat.st_dev == metadata.dev() && root_stat.st_ino == metadata.ino() {
        return Err("Working root is Git metadata; choose another repository".into());
    }
    for folder in folders {
        dir = openat(&dir, *folder, flags, Mode::empty())
            .map_err(|e| format!("Destination parent is unavailable or linked: {e}"))?;
        let stat = fstat(&dir).map_err(|e| e.to_string())?;
        if stat.st_dev == metadata.dev() && stat.st_ino == metadata.ino() {
            return Err("Recovery cannot create files inside Git metadata".into());
        }
    }
    Ok((dir, (*name).into()))
}
#[cfg(unix)]
fn local_available(root: &Path, common: &Path, path: &str) -> Result<(), String> {
    use rustix::fs::{AtFlags, statat};
    let (dir, name) = parent(root, common, path)?;
    match statat(&dir, name.as_str(), AtFlags::SYMLINK_NOFOLLOW) {
        Err(rustix::io::Errno::NOENT) => Ok(()),
        _ => Err("Destination already exists or cannot be verified".into()),
    }
}
#[cfg(unix)]
fn local_create(root: &Path, common: &Path, path: &str, bytes: &[u8]) -> Result<(), String> {
    use rustix::fs::{Mode, OFlags, openat};
    let (dir, name) = parent(root, common, path)?;
    let fd = openat(
        &dir,
        name.as_str(),
        OFlags::WRONLY | OFlags::CLOEXEC | OFlags::NOFOLLOW | OFlags::CREATE | OFlags::EXCL,
        Mode::from_bits_truncate(0o600),
    )
    .map_err(|e| format!("Could not exclusively create recovery file: {e}"))?;
    let mut file = std::fs::File::from(fd);
    file.write_all(bytes).and_then(|_|file.sync_all()).map_err(|e|format!("A new recovery file may be incomplete at {path}; existing files were preserved: {e}"))
}
#[cfg(windows)]
fn windows_parent(
    root: &Path,
    common: &Path,
    path: &str,
) -> Result<(Vec<std::fs::File>, std::path::PathBuf), String> {
    use std::os::windows::fs::{MetadataExt, OpenOptionsExt};
    let open = |path: &Path, no_follow: bool| {
        std::fs::OpenOptions::new()
            .read(true)
            .share_mode(3)
            .custom_flags(0x0200_0000 | if no_follow { 0x0020_0000 } else { 0 })
            .open(path)
            .map_err(|e| e.to_string())
    };
    let mut held = vec![open(root, false)?];
    let metadata_root = std::fs::canonicalize(common).map_err(|e| e.to_string())?;
    if std::fs::canonicalize(root).map_err(|e| e.to_string())? == metadata_root {
        return Err("Working root is Git metadata".into());
    }
    let mut dest = root.to_path_buf();
    let parts: Vec<_> = path.split('/').collect();
    let (name, folders) = parts.split_last().ok_or("Empty destination")?;
    for folder in folders {
        dest.push(folder);
        let dir = open(&dest, true)?;
        let meta = dir.metadata().map_err(|e| e.to_string())?;
        if !meta.is_dir() || meta.file_attributes() & 0x400 != 0 {
            return Err("Destination parent is linked or not a folder".into());
        }
        if std::fs::canonicalize(&dest).map_err(|e| e.to_string())? == metadata_root {
            return Err("Destination crosses Git metadata".into());
        }
        held.push(dir);
    }
    dest.push(name);
    Ok((held, dest))
}
#[cfg(windows)]
fn local_available(root: &Path, common: &Path, path: &str) -> Result<(), String> {
    let (_held, dest) = windows_parent(root, common, path)?;
    match std::fs::symlink_metadata(dest) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        _ => Err("Destination already exists or cannot be verified".into()),
    }
}
#[cfg(windows)]
fn local_create(root: &Path, common: &Path, path: &str, bytes: &[u8]) -> Result<(), String> {
    use std::os::windows::fs::OpenOptionsExt;
    let (_held, dest) = windows_parent(root, common, path)?;
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .share_mode(3)
        .custom_flags(0x0020_0000)
        .open(dest)
        .map_err(|e| e.to_string())?;
    file.write_all(bytes).and_then(|_|file.sync_all()).map_err(|e|format!("A new recovery file may be incomplete at {path}; existing files were preserved: {e}"))
}
#[cfg(not(any(unix, windows)))]
fn local_available(_: &Path, _: &Path, _: &str) -> Result<(), String> {
    Err("Safe new-file recovery is unsupported on this platform".into())
}
#[cfg(not(any(unix, windows)))]
fn local_create(_: &Path, _: &Path, _: &str, _: &[u8]) -> Result<(), String> {
    Err("Safe new-file recovery is unsupported on this platform".into())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn destinations_refuse_metadata_aliases_devices_and_traversal() {
        for path in [
            "../x",
            ".git/config",
            "GIT~1/config",
            ".git\u{200c}/config",
            "a/../b",
            "a\\b",
            "C:/x",
            "NUL",
            "lpt1.txt",
            "a/./b",
            "space./x",
            "x ",
        ] {
            assert!(valid_path(path).is_err(), "{path}");
        }
        assert!(valid_path("recovered ü ' file.rs").is_ok());
    }
    #[cfg(unix)]
    #[test]
    fn remote_new_file_script_is_quoted_exclusive_and_keeps_existing_and_linked_files()
    -> Result<(), String> {
        use std::process::{Command, Stdio};
        let f = super::super::tests::Fixture::new()?;
        let identity = Reader::new(&f.0, &|| true).identity()?;
        let run = |name: &str, bytes: &[u8]| -> Result<std::process::Output, String> {
            let script = remote_script(&identity, name, true)?;
            let mut child = Command::new("sh")
                .args(["-c", &script])
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .map_err(|e| e.to_string())?;
            child
                .stdin
                .take()
                .ok_or("stdin")?
                .write_all(bytes)
                .map_err(|e| e.to_string())?;
            child.wait_with_output().map_err(|e| e.to_string())
        };
        let name = "literal ' $(touch marker) ü";
        assert!(run(name, b"raw\0bytes")?.status.success());
        assert_eq!(
            std::fs::read(f.0.join(name)).map_err(|e| e.to_string())?,
            b"raw\0bytes"
        );
        assert!(!run(name, b"overwrite")?.status.success());
        std::os::unix::fs::symlink("missing", f.0.join("link")).map_err(|e| e.to_string())?;
        assert!(!run("link", b"bad")?.status.success());
        assert!(!run("link/file", b"bad")?.status.success());
        assert!(!f.0.join("marker").exists());
        assert!(!f.0.join("missing").exists());
        assert!(
            !std::fs::read_dir(f.0.join(".git"))
                .map_err(|e| e.to_string())?
                .flatten()
                .any(|entry| entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with("gitcraft-content-recovery."))
        );
        Ok(())
    }
}
