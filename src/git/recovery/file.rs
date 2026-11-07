//! New-file recovery only. Existing files, links, tracked paths and ignored names stay out.
use super::{Identity, Reader};
use std::io::Write;
use std::path::Path;

#[derive(Clone, Debug)]
pub(crate) struct RootPin {
    pub physical: String,
    token: String,
    // Keep the original local directory allocated (and, on Windows, unrenamable).
    _held: Option<std::sync::Arc<std::fs::File>>,
}
impl PartialEq for RootPin {
    fn eq(&self, other: &Self) -> bool {
        self.physical == other.physical && self.token == other.token
    }
}
impl Eq for RootPin {}
pub(super) fn capture(reader: &Reader<'_>, root: &str) -> Result<RootPin, String> {
    if let Some(target) = crate::remote::parse(reader.repo) {
        let out = crate::remote::exec(
            &target.host,
            &crate::remote::Request {
                script: pin_script(root),
                timeout: Some(std::time::Duration::from_secs(30)),
                long: true,
                ..Default::default()
            },
        )?;
        if !reader.active() || out.code != 0 || !out.stderr.is_empty() || out.stdout.len() > 8192 {
            return Err("Cannot capture the remote working-root identity".into());
        }
        parse_pin(&out.stdout)
    } else {
        local_pin(Path::new(root))
    }
}
fn pin_script(root: &str) -> String {
    format!(
        "cd -P -- {} || exit 1\nprintf '%s\\0' \"$(pwd -P)\"\n(LC_ALL=C TZ=UTC0 stat -c '%d:%i:%z' . 2>/dev/null || LC_ALL=C stat -f '%d:%i:%c' .)\n",
        crate::remote::sh_quote(root)
    )
}
fn parse_pin(bytes: &[u8]) -> Result<RootPin, String> {
    let split = bytes
        .iter()
        .position(|b| *b == 0)
        .ok_or("Missing root identity separator")?;
    let physical = std::str::from_utf8(&bytes[..split]).map_err(|_| "Root path is not UTF-8")?;
    let token = bytes[split + 1..]
        .strip_suffix(b"\n")
        .ok_or("Truncated root identity")?;
    let token = std::str::from_utf8(token).map_err(|_| "Invalid filesystem identity")?;
    let parts: Vec<_> = token.splitn(3, ':').collect();
    if !physical.starts_with('/')
        || physical.len() > 4096
        || physical.chars().any(char::is_control)
        || token.len() > 256
        || token.chars().any(char::is_control)
        || parts.len() != 3
        || parts[..2]
            .iter()
            .any(|part| part.is_empty() || !part.bytes().all(|b| b.is_ascii_digit()))
        || parts[2].is_empty()
    {
        return Err("Invalid filesystem identity".into());
    }
    Ok(RootPin {
        physical: physical.into(),
        token: token.into(),
        _held: None,
    })
}
#[cfg(unix)]
fn local_pin(root: &Path) -> Result<RootPin, String> {
    use rustix::fs::{Mode, OFlags, open};
    let physical = std::fs::canonicalize(root).map_err(|e| e.to_string())?;
    let file = std::fs::File::from(
        open(
            &physical,
            OFlags::RDONLY | OFlags::CLOEXEC | OFlags::DIRECTORY | OFlags::NOFOLLOW,
            Mode::empty(),
        )
        .map_err(|e| e.to_string())?,
    );
    Ok(RootPin {
        physical: physical.to_str().ok_or("Root path is not UTF-8")?.into(),
        token: native_token(&file)?,
        _held: Some(std::sync::Arc::new(file)),
    })
}
#[cfg(unix)]
fn native_token(file: &std::fs::File) -> Result<String, String> {
    use std::os::unix::fs::MetadataExt;
    let meta = file.metadata().map_err(|e| e.to_string())?;
    if !meta.is_dir() {
        return Err("Working root is not a directory".into());
    }
    Ok(format!("{}:{}", meta.dev(), meta.ino()))
}
#[cfg(windows)]
fn local_pin(root: &Path) -> Result<RootPin, String> {
    use std::os::windows::fs::{MetadataExt, OpenOptionsExt};
    let physical = std::fs::canonicalize(root).map_err(|e| e.to_string())?;
    let file = std::fs::OpenOptions::new()
        .read(true)
        .share_mode(3)
        .custom_flags(0x0200_0000 | 0x0020_0000)
        .open(&physical)
        .map_err(|e| e.to_string())?;
    let meta = file.metadata().map_err(|e| e.to_string())?;
    if !meta.is_dir() || meta.file_attributes() & 0x400 != 0 {
        return Err("Working root is linked or not a directory".into());
    }
    Ok(RootPin {
        physical: physical.to_str().ok_or("Root path is not UTF-8")?.into(),
        token: native_token(&file)?,
        _held: Some(std::sync::Arc::new(file)),
    })
}
#[cfg(windows)]
#[allow(unsafe_code)]
fn native_token(file: &std::fs::File) -> Result<String, String> {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Storage::FileSystem::{
        BY_HANDLE_FILE_INFORMATION, GetFileInformationByHandle,
    };
    let mut info = BY_HANDLE_FILE_INFORMATION::default();
    // SAFETY: file owns a live directory handle throughout this synchronous call;
    // info is a valid, writable output structure of the exact API type and size.
    if unsafe { GetFileInformationByHandle(file.as_raw_handle(), &mut info) } == 0 {
        return Err(std::io::Error::last_os_error().to_string());
    }
    if info.nFileIndexHigh == 0 && info.nFileIndexLow == 0 {
        return Err("This filesystem does not expose a usable directory identity".into());
    }
    Ok(format!(
        "{}:{}:{}",
        info.dwVolumeSerialNumber, info.nFileIndexHigh, info.nFileIndexLow
    ))
}
#[cfg(not(any(unix, windows)))]
fn local_pin(_: &Path) -> Result<RootPin, String> {
    Err("Filesystem identity is unavailable on this platform".into())
}
fn pinned(identity: &Identity) -> Result<&RootPin, String> {
    identity.root_pin.as_ref().ok_or_else(||"Safe file recovery is unavailable: working-root filesystem identity could not be captured".into())
}

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
    let pin = pinned(identity)?;
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
            Path::new(&pin.physical),
            Path::new(&identity.common),
            path,
            pin,
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
    let pin = pinned(identity)?;
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
            Path::new(&pin.physical),
            Path::new(&identity.common),
            path,
            bytes,
            pin,
        )
    }
}
fn remote_script(identity: &Identity, path: &str, create: bool) -> Result<String, String> {
    use crate::remote::sh_quote;
    valid_path(path)?;
    let pin = pinned(identity)?;
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
    script.push_str(&format!("[ \"$(pwd -P)\" = {} ] || exit 1\n[ \"$((LC_ALL=C TZ=UTC0 stat -c '%d:%i:%z' . 2>/dev/null || LC_ALL=C stat -f '%d:%i:%c' .))\" = {} ] || exit 1\n",sh_quote(&pin.physical),sh_quote(&pin.token)));
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
        script.push_str(&format!("recovery_tmp=$(mktemp -d -- {template}) || exit 1\ntrap 'rm -rf -- \"$recovery_tmp\"' 0\ntrap 'exit 1' HUP INT TERM PIPE\ncat >\"$recovery_tmp/blob\" || exit 1\nif ln -T -- \"$recovery_tmp/blob\" {name} 2>/dev/null; then :; elif command -v perl >/dev/null 2>&1; then perl -e 'link($ARGV[0], $ARGV[1]) or die \"create-only publication: $!\\n\"' -- \"$recovery_tmp/blob\" {name} || exit 1; else printf 'Create-only recovery needs GNU ln or Perl\\n' >&2; exit 1; fi\nprintf 'created\\n'\n"));
    } else {
        script.push_str("printf 'available\\n'\n");
    }
    Ok(script)
}
#[cfg(unix)]
fn parent(
    root: &Path,
    common: &Path,
    path: &str,
    pin: &RootPin,
) -> Result<(std::fs::File, String), String> {
    use rustix::fs::{Mode, OFlags, fstat, open, openat};
    use std::os::unix::fs::MetadataExt;
    let parts: Vec<_> = path.split('/').collect();
    let (name, folders) = parts.split_last().ok_or("Empty destination")?;
    let flags = OFlags::RDONLY | OFlags::CLOEXEC | OFlags::DIRECTORY | OFlags::NOFOLLOW;
    let mut dir = std::fs::File::from(open(root, flags, Mode::empty()).map_err(|e| e.to_string())?);
    if native_token(&dir)? != pin.token {
        return Err("The working-root filesystem identity changed since preview".into());
    }
    let metadata = std::fs::metadata(common).map_err(|e| e.to_string())?;
    let root_stat = fstat(&dir).map_err(|e| e.to_string())?;
    if root_stat.st_dev as u64 == metadata.dev() && root_stat.st_ino == metadata.ino() {
        return Err("Working root is Git metadata; choose another repository".into());
    }
    for folder in folders {
        dir = std::fs::File::from(
            openat(&dir, *folder, flags, Mode::empty())
                .map_err(|e| format!("Destination parent is unavailable or linked: {e}"))?,
        );
        let stat = fstat(&dir).map_err(|e| e.to_string())?;
        if stat.st_dev as u64 == metadata.dev() && stat.st_ino == metadata.ino() {
            return Err("Recovery cannot create files inside Git metadata".into());
        }
    }
    Ok((dir, (*name).into()))
}
#[cfg(unix)]
fn local_available(root: &Path, common: &Path, path: &str, pin: &RootPin) -> Result<(), String> {
    use rustix::fs::{AtFlags, statat};
    let (dir, name) = parent(root, common, path, pin)?;
    match statat(&dir, name.as_str(), AtFlags::SYMLINK_NOFOLLOW) {
        Err(rustix::io::Errno::NOENT) => Ok(()),
        _ => Err("Destination already exists or cannot be verified".into()),
    }
}
#[cfg(unix)]
fn local_create(
    root: &Path,
    common: &Path,
    path: &str,
    bytes: &[u8],
    pin: &RootPin,
) -> Result<(), String> {
    use rustix::fs::{Mode, OFlags, openat};
    let (dir, name) = parent(root, common, path, pin)?;
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
fn physical_ancestors(root: &Path) -> Result<Vec<std::path::PathBuf>, String> {
    if !root.is_absolute() {
        return Err("Recovery root must be a physical absolute path".into());
    }
    let mut paths: Vec<_> = root
        .ancestors()
        .filter(|path| path.has_root())
        .map(Path::to_path_buf)
        .collect();
    paths.reverse();
    paths.dedup();
    if paths.last().is_none_or(|path| path != root) {
        return Err("Cannot anchor the physical recovery path".into());
    }
    Ok(paths)
}
#[cfg(windows)]
fn windows_parent(
    root: &Path,
    common: &Path,
    path: &str,
    pin: &RootPin,
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
    // Hold every physical component, not only the worktree and its descendants.
    // No-delete sharing directly prevents each ancestor's rename/replacement while
    // the final absolute-path CREATE_NEW is resolved. Reparse points fail closed.
    let mut held = Vec::new();
    for ancestor in physical_ancestors(root)? {
        let directory = open(&ancestor, true)?;
        let meta = directory.metadata().map_err(|e| e.to_string())?;
        if !meta.is_dir() || meta.file_attributes() & 0x400 != 0 {
            return Err("A physical recovery ancestor is linked or unavailable".into());
        }
        held.push(directory);
    }
    let root_file = held.last().ok_or("Missing physical recovery root")?;
    if native_token(root_file)? != pin.token {
        return Err("The working-root filesystem identity changed since preview".into());
    }
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
fn local_available(root: &Path, common: &Path, path: &str, pin: &RootPin) -> Result<(), String> {
    let (_held, dest) = windows_parent(root, common, path, pin)?;
    match std::fs::symlink_metadata(dest) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        _ => Err("Destination already exists or cannot be verified".into()),
    }
}
#[cfg(windows)]
fn local_create(
    root: &Path,
    common: &Path,
    path: &str,
    bytes: &[u8],
    pin: &RootPin,
) -> Result<(), String> {
    use std::os::windows::fs::OpenOptionsExt;
    let (_held, dest) = windows_parent(root, common, path, pin)?;
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
fn local_available(_: &Path, _: &Path, _: &str, _: &RootPin) -> Result<(), String> {
    Err("Safe new-file recovery is unsupported on this platform".into())
}
#[cfg(not(any(unix, windows)))]
fn local_create(_: &Path, _: &Path, _: &str, _: &[u8], _: &RootPin) -> Result<(), String> {
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
        let mut identity = Reader::new(&f.0, &|| true).identity()?;
        let probe = Command::new("sh")
            .args(["-c", &pin_script(identity.root.as_deref().ok_or("root")?)])
            .output()
            .map_err(|e| e.to_string())?;
        identity.root_pin = Some(parse_pin(&probe.stdout)?);
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

#[cfg(all(test, unix))]
mod root_replacement_tests {
    use super::*;
    use std::process::{Command, Stdio};
    fn nested(f: &super::super::tests::Fixture, name: &str) -> Result<std::path::PathBuf, String> {
        let root = f.0.join(name);
        std::fs::create_dir_all(&root).map_err(|e| e.to_string())?;
        crate::git::run(Some(&root), &["init", "-q"])?;
        Ok(root)
    }
    #[test]
    fn local_creation_rejects_root_and_ancestor_replacements_after_the_final_git_check()
    -> Result<(), String> {
        let f = super::super::tests::Fixture::new()?;
        let root = nested(&f, "selected")?;
        let reader = Reader::new(&root, &|| true);
        let identity = reader.identity()?;
        let moved = f.0.join("saved-selected");
        let unrelated = nested(&f, "unrelated")?;
        std::fs::rename(&root, &moved).map_err(|e| e.to_string())?;
        std::os::unix::fs::symlink(&unrelated, &root).map_err(|e| e.to_string())?;
        assert!(create(&reader, &identity, "must-not-escape", b"saved raw bytes").is_err());
        assert!(!unrelated.join("must-not-escape").exists());
        std::fs::remove_file(&root)
            .and_then(|_| std::fs::rename(&moved, &root))
            .map_err(|e| e.to_string())?;
        let root = nested(&f, "container/repo")?;
        let reader = Reader::new(&root, &|| true);
        let identity = reader.identity()?;
        let parent = f.0.join("container");
        let moved = f.0.join("saved-container");
        let outside = nested(&f, "outside-container/repo")?;
        std::fs::rename(&parent, &moved).map_err(|e| e.to_string())?;
        std::os::unix::fs::symlink(outside.parent().ok_or("parent")?, &parent)
            .map_err(|e| e.to_string())?;
        assert!(create(&reader, &identity, "must-not-escape", b"saved raw bytes").is_err());
        assert!(!outside.join("must-not-escape").exists());
        std::fs::remove_file(&parent)
            .and_then(|_| std::fs::rename(&moved, &parent))
            .map_err(|e| e.to_string())?;
        Ok(())
    }
    #[test]
    fn typed_remote_publication_rejects_root_symlink_and_same_name_directory_substitution()
    -> Result<(), String> {
        let f = super::super::tests::Fixture::new()?;
        let selected = nested(&f, "selected")?;
        let unrelated = nested(&f, "unrelated")?;
        for symlink in [true, false] {
            let mut identity = Reader::new(&selected, &|| true).identity()?;
            let out = Command::new("sh")
                .args(["-c", &pin_script(selected.to_str().ok_or("utf8")?)])
                .output()
                .map_err(|e| e.to_string())?;
            identity.root_pin = Some(parse_pin(&out.stdout)?);
            let script = remote_script(&identity, "must-not-escape", true)?;
            let saved = f.0.join("saved-selected");
            std::fs::rename(&selected, &saved).map_err(|e| e.to_string())?;
            if symlink {
                std::os::unix::fs::symlink(&unrelated, &selected).map_err(|e| e.to_string())?;
            } else {
                std::fs::create_dir(&selected)
                    .and_then(|_| std::fs::create_dir(selected.join(".git")))
                    .map_err(|e| e.to_string())?;
            }
            let mut child = Command::new("sh")
                .args(["-c", &script])
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .map_err(|e| e.to_string())?;
            // Feed the already-previewed bytes, just as the production typed request does.
            let _ = child
                .stdin
                .take()
                .ok_or("stdin")?
                .write_all(b"saved raw bytes");
            let out = child.wait_with_output().map_err(|e| e.to_string())?;
            assert!(!out.status.success());
            assert!(!unrelated.join("must-not-escape").exists());
            assert!(!selected.join("must-not-escape").exists());
            if symlink {
                std::fs::remove_file(&selected).map_err(|e| e.to_string())?;
            } else {
                std::fs::remove_dir_all(&selected).map_err(|e| e.to_string())?;
            }
            std::fs::rename(&saved, &selected).map_err(|e| e.to_string())?;
        }
        Ok(())
    }
}

#[cfg(all(test, windows))]
mod windows_ancestor_tests {
    use super::*;
    #[test]
    fn physical_paths_include_every_drive_and_unc_ancestor_without_drive_relative_prefixes()
    -> Result<(), String> {
        for root in [r"\\?\C:\parent\repo", r"\\?\UNC\server\share\parent\repo"] {
            let root = Path::new(root);
            let paths = physical_ancestors(root)?;
            assert_eq!(paths.last().map(std::path::PathBuf::as_path), Some(root));
            assert!(
                paths
                    .iter()
                    .all(|path| path.is_absolute() && path.has_root())
            );
            assert!(paths.len() >= 3);
            for pair in paths.windows(2) {
                assert_eq!(pair[1].parent(), Some(pair[0].as_path()));
            }
        }
        assert!(physical_ancestors(Path::new(r"C:relative")).is_err());
        Ok(())
    }
}
