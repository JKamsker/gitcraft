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
impl RootPin {
    pub(crate) fn device_inode(&self) -> Option<(&str, &str)> {
        let mut parts = self.token.split(':');
        Some((parts.next()?, parts.next()?))
    }

    /// A caller must enter the captured repository directory before this guard.
    /// The shell's cwd then retains that object through subsequent relative operations.
    pub(crate) fn remote_guard(&self) -> String {
        use crate::remote::sh_quote;
        format!(
            "[ \"$(pwd -P)\" = {} ] || exit 1\n[ \"$(LC_ALL=C TZ=UTC0 stat -c '%d:%i:%z' . 2>/dev/null || LC_ALL=C stat -f '%d:%i:%c' .)\" = {} ] || exit 1\n",
            sh_quote(&self.physical),
            sh_quote(&self.token)
        )
    }
}

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
            return Err(crate::i18n::text(
                "backend-cannot-capture-the-remote-working-root-identity-3c942e",
            )
            .into());
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
    let split = bytes.iter().position(|b| *b == 0).ok_or(crate::i18n::text(
        "backend-missing-root-identity-separator-a7274b",
    ))?;
    let physical = std::str::from_utf8(&bytes[..split])
        .map_err(|_| crate::i18n::text("backend-root-path-is-not-utf-8-e3f90e"))?;
    let token = bytes[split + 1..]
        .strip_suffix(b"\n")
        .ok_or(crate::i18n::text("backend-truncated-root-identity-27d7ff"))?;
    let token = std::str::from_utf8(token)
        .map_err(|_| crate::i18n::text("backend-invalid-filesystem-identity-9dc568"))?;
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
        return Err(crate::i18n::text("backend-invalid-filesystem-identity-9dc568").into());
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
        physical: physical
            .to_str()
            .ok_or(crate::i18n::text("backend-root-path-is-not-utf-8-e3f90e"))?
            .into(),
        token: native_token(&file)?,
        _held: Some(std::sync::Arc::new(file)),
    })
}
#[cfg(unix)]
fn native_token(file: &std::fs::File) -> Result<String, String> {
    use std::os::unix::fs::MetadataExt;
    let meta = file.metadata().map_err(|e| e.to_string())?;
    if !meta.is_dir() {
        return Err(crate::i18n::text("backend-working-root-is-not-a-directory-48a45e").into());
    }
    Ok(format!("{}:{}", meta.dev(), meta.ino()))
}
#[cfg(unix)]
pub(crate) fn native_file_identity(file: &std::fs::File) -> Result<(String, u64), String> {
    use std::os::unix::fs::MetadataExt;
    let meta = file.metadata().map_err(|e| e.to_string())?;
    Ok((format!("{}:{}", meta.dev(), meta.ino()), meta.nlink()))
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
        return Err(
            crate::i18n::text("backend-working-root-is-linked-or-not-a-directory-5a3f3b").into(),
        );
    }
    Ok(RootPin {
        physical: physical
            .to_str()
            .ok_or(crate::i18n::text("backend-root-path-is-not-utf-8-e3f90e"))?
            .into(),
        token: native_token(&file)?,
        _held: Some(std::sync::Arc::new(file)),
    })
}
#[cfg(windows)]
fn native_token(file: &std::fs::File) -> Result<String, String> {
    native_file_identity(file).map(|(token, _)| token)
}
#[cfg(windows)]
#[allow(unsafe_code)]
pub(crate) fn native_file_identity(file: &std::fs::File) -> Result<(String, u64), String> {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Storage::FileSystem::{
        BY_HANDLE_FILE_INFORMATION, GetFileInformationByHandle,
    };
    let mut info = BY_HANDLE_FILE_INFORMATION::default();
    // SAFETY: file owns a live file/directory handle throughout this synchronous call;
    // info is a valid, writable output structure of the exact API type and size.
    if unsafe { GetFileInformationByHandle(file.as_raw_handle(), &mut info) } == 0 {
        return Err(std::io::Error::last_os_error().to_string());
    }
    if info.nFileIndexHigh == 0 && info.nFileIndexLow == 0 {
        return Err(crate::i18n::text(
            "backend-this-filesystem-does-not-expose-a-usable-directory-identity-766092",
        )
        .into());
    }
    Ok((
        format!(
            "{}:{}:{}",
            info.dwVolumeSerialNumber, info.nFileIndexHigh, info.nFileIndexLow
        ),
        u64::from(info.nNumberOfLinks),
    ))
}
#[cfg(not(any(unix, windows)))]
fn local_pin(_: &Path) -> Result<RootPin, String> {
    Err(
        crate::i18n::text("backend-filesystem-identity-is-unavailable-on-this-platform-ccdc09")
            .into(),
    )
}
fn pinned(identity: &Identity) -> Result<&RootPin, String> {
    identity.root_pin.as_ref().ok_or_else(|| {
        crate::i18n::text(
            "backend-safe-file-recovery-is-unavailable-working-root-filesystem-identit-a4ddcb",
        )
        .into()
    })
}

pub(super) fn valid_path(path: &str) -> Result<(), String> {
    if path.is_empty()
        || path.len() > 4096
        || path.starts_with('/')
        || path
            .chars()
            .any(|c| c.is_control() || matches!(c, '\\' | ':' | '<' | '>' | '"' | '|' | '?' | '*'))
    {
        return Err(crate::i18n::text(
            "backend-choose-a-portable-repository-relative-filename-c109a3",
        )
        .into());
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
            return Err(crate::i18n::text(
                "backend-destination-crosses-git-metadata-or-a-reserved-ambiguous-filename-7a1e1c",
            )
            .into());
        }
    }
    Ok(())
}
fn root(identity: &Identity) -> Result<&str, String> {
    identity.root.as_deref().ok_or_else(|| {
        crate::i18n::text("backend-file-recovery-needs-a-working-repository-4a4bd6").into()
    })
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
            Err(crate::i18n::text(
                "backend-destination-exists-crosses-a-link-or-has-an-unavailable-parent-9e1a12",
            )
            .into())
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
            Err(crate::i18n::text(
                "backend-new-file-recovery-failed-safely-the-destination-must-not-exist-re-64b66f",
            )
            .into())
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
        return Err(crate::i18n::text(
            "backend-remote-repository-metadata-must-be-absolute-967767",
        )
        .into());
    }
    let parts: Vec<_> = path.split('/').collect();
    let (name, parents) = parts
        .split_last()
        .ok_or(crate::i18n::text("backend-empty-destination-93a1ec"))?;
    let mut script = format!(
        "umask 077\ncd -P -- {} || exit 1\nrecovery_root=$(pwd -P) || exit 1\n[ \"$recovery_root\" = / ] && recovery_root=\n",
        sh_quote(root)
    );
    script.push_str(&pin.remote_guard());
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

#[cfg(test)]
mod tests;

#[cfg(all(test, unix))]
mod root_replacement_tests;

#[cfg(all(test, windows))]
mod windows_ancestor_tests;

mod local;
use local::local_available;

use local::local_create;

#[cfg(unix)]
pub(crate) use local::parent;
#[cfg(windows)]
use local::physical_ancestors;
#[cfg(windows)]
pub(crate) use local::windows_parent;
