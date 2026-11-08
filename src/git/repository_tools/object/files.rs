//! Bounded filesystem metadata on the repository's machine. Paths are data, never shell code.
use crate::remote;
use std::path::{Path, PathBuf};
use std::time::Duration;

fn remote_read(
    path: &Path,
    body: impl FnOnce(&str) -> String,
    cap: usize,
    active: &dyn Fn() -> bool,
) -> Result<Option<Vec<u8>>, String> {
    let Some(t) = remote::parse(path) else {
        return Ok(None);
    };
    if !active() {
        return Err("Object inspection cancelled".into());
    }
    let request = remote::Request {
        script: body(&remote::sh_path(&t.path)),
        timeout: Some(Duration::from_secs(20)),
        long: true,
        ..Default::default()
    };
    let out = remote::exec(&t.host, &request)?;
    if out.code != 0 {
        return Err(format!(
            "Cannot inspect object-store path {}: {}",
            remote::display(path),
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    if !active() {
        return Err("Object inspection cancelled".into());
    }
    if out.stdout.len() > cap {
        return Err("Object-store metadata exceeds the inspection limit".into());
    }
    Ok(Some(out.stdout))
}
pub(super) fn size(path: &Path, active: &dyn Fn() -> bool) -> Result<Option<u64>, String> {
    if let Some(out) = remote_read(
        path,
        |p| {
            format!(
                "if [ -f {p} ]; then wc -c < {p}; elif [ -e {p} ]; then echo 'not a regular file' >&2; exit 1; else printf 'missing'; fi"
            )
        },
        128,
        active,
    )? {
        let s = String::from_utf8(out).map_err(|_| "Invalid file size")?;
        return if s == "missing" {
            Ok(None)
        } else {
            s.trim()
                .parse()
                .map(Some)
                .map_err(|_| "Invalid file size".into())
        };
    }
    if !active() {
        return Err("Object inspection cancelled".into());
    }
    match std::fs::metadata(path) {
        Ok(m) if m.is_file() => Ok(Some(m.len())),
        Ok(_) => Err(format!("{} is not a regular object file", path.display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.to_string()),
    }
}
pub(super) fn packs(
    store: &Path,
    max: usize,
    active: &dyn Fn() -> bool,
) -> Result<Vec<PathBuf>, String> {
    let directory = remote::join(store, "pack");
    let mut names = if let Some(out) = remote_read(
        &directory,
        |p| {
            format!(
                "[ ! -e {p} ] && exit 0\n[ -d {p} ] || exit 1\nn=0\nfor f in {p}/pack-*.idx; do [ -f \"$f\" ] || continue; n=$((n+1)); [ \"$n\" -le {max} ] || {{ echo 'More than {max} packs; use Git in the terminal' >&2; exit 1; }}; printf '%s\\0' \"${{f##*/}}\"; done"
            )
        },
        max * 128,
        active,
    )? {
        out.split(|b| *b == 0)
            .filter(|s| !s.is_empty())
            .map(|s| {
                String::from_utf8(s.to_vec()).map_err(|_| "Pack filename is not UTF-8".to_owned())
            })
            .collect::<Result<Vec<_>, _>>()?
    } else {
        let mut names = Vec::new();
        match std::fs::read_dir(&directory) {
            Ok(entries) => {
                for entry in entries {
                    let entry = entry.map_err(|e| e.to_string())?;
                    let name = entry.file_name().to_string_lossy().into_owned();
                    if name.starts_with("pack-") && name.ends_with(".idx") {
                        if names.len() >= max {
                            return Err(format!("More than {max} packs; use Git in the terminal."));
                        }
                        names.push(name);
                    }
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
            Err(e) => return Err(e.to_string()),
        }
        names
    };
    names.sort();
    names
        .into_iter()
        .map(|name| {
            let hash = name
                .strip_prefix("pack-")
                .and_then(|s| s.strip_suffix(".idx"))
                .ok_or("Invalid pack filename")?;
            if !super::valid_oid(hash) {
                return Err("Unexpected pack index filename; inspect with Git.".into());
            }
            Ok(remote::join(&directory, &name))
        })
        .collect()
}

/// Bounded optional file contents, on the repository machine.
pub(crate) fn read(
    path: &Path,
    cap: usize,
    active: &dyn Fn() -> bool,
) -> Result<Option<Vec<u8>>, String> {
    if let Some(out) = remote_read(
        path,
        |p| {
            format!(
                "if [ -f {p} ]; then printf '1'; head -c {} < {p}; elif [ -e {p} ]; then echo 'not a regular file' >&2; exit 1; else printf '0'; fi",
                cap + 1
            )
        },
        cap + 2,
        active,
    )? {
        return match out.first() {
            Some(b'0') => Ok(None),
            Some(b'1') if out.len() - 1 <= cap => Ok(Some(out[1..].to_vec())),
            _ => Err("Object-store metadata exceeds the inspection limit".into()),
        };
    }
    if !active() {
        return Err("Object inspection cancelled".into());
    }
    // O_NONBLOCK prevents a substituted FIFO from blocking open before fstat.
    // O_NOFOLLOW rejects symlink substitution; only a pinned regular file is read.
    #[cfg(unix)]
    let opened: Result<std::fs::File, std::io::Error> = rustix::fs::open(
        path,
        rustix::fs::OFlags::RDONLY
            | rustix::fs::OFlags::NONBLOCK
            | rustix::fs::OFlags::NOFOLLOW
            | rustix::fs::OFlags::CLOEXEC,
        rustix::fs::Mode::empty(),
    )
    .map(std::fs::File::from)
    .map_err(Into::into);
    #[cfg(not(unix))]
    let opened = match std::fs::symlink_metadata(path) {
        Ok(m) if m.is_file() => std::fs::File::open(path),
        Ok(_) => return Err("Object-store metadata is not a regular file".into()),
        Err(e) => Err(e),
    };
    let file = match opened {
        Ok(f) => f,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => {
            return Err(format!(
                "Cannot read regular metadata {}: {e}",
                remote::display(path)
            ));
        }
    };
    let metadata = file.metadata().map_err(|e| e.to_string())?;
    if !metadata.is_file() {
        return Err("Object-store metadata is not a regular file".into());
    }
    if metadata.len() > cap as u64 {
        return Err("Object-store metadata exceeds the inspection limit".into());
    }
    let bytes = bounded_bytes(file, cap, active)?;
    Ok(Some(bytes))
}

fn bounded_bytes(
    mut reader: impl std::io::Read,
    cap: usize,
    active: &dyn Fn() -> bool,
) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    let mut chunk = [0u8; 8192];
    loop {
        if !active() {
            return Err("Object inspection cancelled".into());
        }
        let remaining = (cap + 1).saturating_sub(bytes.len()).min(chunk.len());
        if remaining == 0 {
            return Err("Object-store metadata exceeds the inspection limit".into());
        }
        let n = reader
            .read(&mut chunk[..remaining])
            .map_err(|e| e.to_string())?;
        if n == 0 {
            return Ok(bytes);
        }
        bytes.extend_from_slice(&chunk[..n]);
        if bytes.len() > cap {
            return Err("Object-store metadata exceeds the inspection limit".into());
        }
    }
}
#[cfg(test)]
mod bounded_metadata_pure {
    use super::*;
    #[test]
    fn bounds_and_cancellation_apply_between_chunks() {
        assert_eq!(bounded_bytes(&b"abcd"[..], 4, &|| true).unwrap(), b"abcd");
        assert!(bounded_bytes(&b"abcde"[..], 4, &|| true).is_err());
        let checks = std::cell::Cell::new(0);
        assert!(
            bounded_bytes(&[0u8; 16384][..], 16384, &|| {
                checks.set(checks.get() + 1);
                checks.get() < 2
            })
            .is_err()
        );
        assert_eq!(checks.get(), 2);
    }
}
