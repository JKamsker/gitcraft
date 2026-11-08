//! Sparse metadata only: a pinned regular file, at most 1 MiB, unchanged while read.
//! Local descriptor flags follow the accepted repository-tools bounded reader (8bd3e5).
use std::{io::Read, path::Path};

const CAP: usize = 1 << 20;
const REFUSED: &str = "Sparse patterns must be an unchanged regular file of at most 1 MiB; inspect them in the terminal";

pub(super) fn read(path: &Path) -> Result<String, String> {
    if let Some(target) = crate::remote::parse(path) {
        let output = crate::remote::exec(&target.host, &request(&target.path))?;
        return decode(output.code, output.stdout);
    }
    let file = match open(path) {
        Ok(file) => file,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(String::new()),
        Err(e) => return Err(format!("{REFUSED}: {e}")),
    };
    let before = file.metadata().map_err(|e| e.to_string())?;
    regular(before.is_file(), before.len())?;
    // Also refuse Windows reparse points before reading their contents.
    if !unchanged(&before, &before) {
        return Err(REFUSED.into());
    }
    let bytes = bounded(&file)?;
    let after = file.metadata().map_err(|e| e.to_string())?;
    let named = std::fs::symlink_metadata(path).map_err(|_| REFUSED.to_owned())?;
    if !unchanged(&before, &after) || !unchanged(&after, &named) {
        return Err(REFUSED.into());
    }
    decode(0, bytes)
}

#[cfg(unix)]
fn open(path: &Path) -> std::io::Result<std::fs::File> {
    use rustix::fs::{Mode, OFlags, open};
    open(
        path,
        OFlags::RDONLY | OFlags::CLOEXEC | OFlags::NONBLOCK | OFlags::NOFOLLOW,
        Mode::empty(),
    )
    .map(std::fs::File::from)
    .map_err(Into::into)
}
#[cfg(windows)]
fn open(path: &Path) -> std::io::Result<std::fs::File> {
    use std::os::windows::fs::OpenOptionsExt;
    // No FILE_SHARE_DELETE: pin the name; open the reparse point itself, never its target.
    std::fs::OpenOptions::new()
        .read(true)
        .share_mode(3)
        .custom_flags(0x0020_0000)
        .open(path)
}
#[cfg(not(any(unix, windows)))]
fn open(_: &Path) -> std::io::Result<std::fs::File> {
    Err(std::io::Error::other(
        "Bounded sparse metadata is unsupported on this platform",
    ))
}
fn regular(is_file: bool, len: u64) -> Result<(), String> {
    if !is_file || len > CAP as u64 {
        Err(REFUSED.into())
    } else {
        Ok(())
    }
}
fn unchanged(a: &std::fs::Metadata, b: &std::fs::Metadata) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if (a.dev(), a.ino(), a.ctime(), a.ctime_nsec())
            != (b.dev(), b.ino(), b.ctime(), b.ctime_nsec())
        {
            return false;
        }
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if a.file_attributes() & 0x400 != 0 || b.file_attributes() & 0x400 != 0 {
            return false;
        }
    }
    a.is_file()
        && b.is_file()
        && a.len() == b.len()
        && a.modified()
            .ok()
            .zip(b.modified().ok())
            .is_some_and(|(a, b)| a == b)
}
fn bounded(reader: impl Read) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    reader
        .take(CAP as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    regular(true, bytes.len() as u64)?;
    Ok(bytes)
}
fn decode(code: i32, bytes: Vec<u8>) -> Result<String, String> {
    match code {
        11 if bytes.is_empty() => Ok(String::new()),
        0 if bytes.len() <= CAP => String::from_utf8(bytes)
            .map_err(|_| "Sparse patterns are not UTF-8; inspect them in the terminal".into()),
        _ => Err(format!(
            "{REFUSED}. Remote reads require python3 and descriptor no-follow/nonblocking support"
        )),
    }
}

// Shell tests/open followed by head have a FIFO substitution race. Open nonblocking once
// and inspect that descriptor instead. Missing capability fails closed, never falls back
// to cat/head. Output is withheld until both descriptor and pathname still match.
const SCRIPT: &str = r#"import os, stat, sys
p = sys.argv[1]
cap = 1048576
try:
    fd = os.open(p, os.O_RDONLY | os.O_NONBLOCK | os.O_NOFOLLOW)
except FileNotFoundError:
    sys.exit(11)
try:
    before = os.fstat(fd)
    if not stat.S_ISREG(before.st_mode) or before.st_size > cap:
        sys.exit(12)
    data = bytearray()
    while len(data) <= cap:
        chunk = os.read(fd, min(8192, cap + 1 - len(data)))
        if not chunk:
            break
        data.extend(chunk)
    after = os.fstat(fd)
    named = os.lstat(p)
    def identity(s):
        return (s.st_dev, s.st_ino, s.st_mode, s.st_size, s.st_mtime_ns, s.st_ctime_ns)
    if len(data) > cap or identity(before) != identity(after) or identity(after) != identity(named):
        sys.exit(12)
    sys.stdout.buffer.write(data)
finally:
    os.close(fd)
"#;
fn request(path: &str) -> crate::remote::Request {
    crate::remote::Request {
        script: format!(
            "exec python3 -c {} {}",
            crate::remote::sh_quote(SCRIPT),
            crate::remote::sh_path(path)
        ),
        timeout: Some(std::time::Duration::from_secs(20)),
        long: true,
        ..Default::default()
    }
}

#[cfg(test)]
mod pure_tests {
    use super::*;
    #[test]
    fn bounded_read_and_explicit_refusals() -> Result<(), String> {
        assert_eq!(bounded(&b"patterns"[..])?, b"patterns");
        let consumed = std::cell::Cell::new(0usize);
        struct Growing<'a>(&'a std::cell::Cell<usize>);
        impl Read for Growing<'_> {
            fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
                out.fill(b'x');
                self.0.set(self.0.get() + out.len());
                Ok(out.len())
            }
        }
        assert!(bounded(Growing(&consumed)).is_err());
        assert_eq!(consumed.get(), CAP + 1);
        assert!(regular(false, 0).is_err());
        assert!(regular(true, CAP as u64 + 1).is_err());
        assert_eq!(decode(11, Vec::new())?, "");
        assert!(decode(12, Vec::new()).is_err());
        assert!(decode(0, vec![0xff]).is_err());
        Ok(())
    }
    #[test]
    fn remote_request_is_bounded_descriptor_read_with_no_fallback() {
        let req = request("/repo/it's sparse");
        assert!(req.script.starts_with("exec python3 -c "));
        for required in [
            "os.O_NONBLOCK",
            "os.O_NOFOLLOW",
            "os.fstat(fd)",
            "stat.S_ISREG",
            "cap + 1 - len(data)",
            "os.lstat(p)",
            "sys.exit(11)",
        ] {
            assert!(req.script.contains(required), "{required}");
        }
        assert_eq!(req.timeout, Some(std::time::Duration::from_secs(20)));
        assert!(!req.script.contains("head -c"));
    }
}

/// Owned filesystem regression, authored and compiled only under the global runtime hold.
#[cfg(all(test, unix))]
mod runtime_held_tests {
    use super::*;
    #[test]
    fn regular_missing_large_symlink_and_fifo_patterns() -> Result<(), String> {
        let root = std::env::temp_dir().join(format!(
            "gitcraft-sparse-patterns-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(|e| e.to_string())?
                .as_nanos()
        ));
        std::fs::create_dir(&root).map_err(|e| e.to_string())?;
        let outcome = (|| {
            let file = root.join("patterns");
            assert_eq!(read(&file)?, "");
            std::fs::write(&file, b"/src/\n").map_err(|e| e.to_string())?;
            assert_eq!(read(&file)?, "/src/\n");
            std::fs::write(&file, vec![b'x'; CAP + 1]).map_err(|e| e.to_string())?;
            assert!(read(&file).is_err());
            assert!(read(&root).is_err());
            let link = root.join("link");
            std::os::unix::fs::symlink(&file, &link).map_err(|e| e.to_string())?;
            assert!(read(&link).is_err());
            // rustix does not expose mkfifoat on Apple platforms.
            #[cfg(target_os = "linux")]
            {
                let fifo = root.join("fifo");
                rustix::fs::mkfifoat(
                    rustix::fs::CWD,
                    &fifo,
                    rustix::fs::Mode::RUSR | rustix::fs::Mode::WUSR,
                )
                .map_err(|e| e.to_string())?;
                assert!(read(&fifo).is_err());
            }
            Ok(())
        })();
        // Only this exclusive owned directory, never a process selector.
        let cleanup = std::fs::remove_dir_all(&root).map_err(|e| e.to_string());
        outcome.and(cleanup)
    }
}
