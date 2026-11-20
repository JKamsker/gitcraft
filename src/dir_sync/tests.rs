use super::*;

struct Fixture(std::path::PathBuf);
impl Fixture {
    fn new(name: &str) -> io::Result<Self> {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "gitcraft-dir-sync-{name}-{}-{n}",
            std::process::id()
        ));
        std::fs::create_dir(&path)?;
        Ok(Self(path))
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn flushes_a_real_directory_after_a_rename_without_changing_entries() -> io::Result<()> {
    let fixture = Fixture::new("rename")?;
    let temp = fixture.0.join("receipt.pending");
    std::fs::write(&temp, b"\xff\0receipt\r\n")?;
    std::fs::rename(&temp, fixture.0.join("receipt.json"))?;
    sync(&fixture.0)?;
    // Repeated flushes are harmless, and the handle shares deletion: the directory
    // can still be renamed and removed right after.
    sync(&fixture.0)?;
    let moved = fixture.0.with_extension("moved");
    std::fs::rename(&fixture.0, &moved)?;
    let names: Vec<_> = std::fs::read_dir(&moved)?
        .map(|e| e.map(|e| e.file_name()))
        .collect::<Result<_, _>>()?;
    std::fs::rename(&moved, &fixture.0)?;
    assert_eq!(names, ["receipt.json"]);
    assert_eq!(
        std::fs::read(fixture.0.join("receipt.json"))?,
        b"\xff\0receipt\r\n"
    );
    Ok(())
}

#[test]
fn missing_directory_is_an_error_not_a_silent_success() -> io::Result<()> {
    let fixture = Fixture::new("missing")?;
    let missing = fixture.0.join("never-created");
    let error = sync(&missing)
        .err()
        .ok_or_else(|| io::Error::other("flushed nothing"))?;
    assert_eq!(error.kind(), io::ErrorKind::NotFound, "{error}");
    assert!(!cannot_add_entries(&error));
    assert!(!missing.exists(), "a flush never creates its directory");
    Ok(())
}

#[test]
fn only_windows_access_denial_means_no_entries_could_be_added() {
    let denied = io::Error::from(io::ErrorKind::PermissionDenied);
    assert_eq!(cannot_add_entries(&denied), cfg!(windows));
    for other in [io::ErrorKind::NotFound, io::ErrorKind::Other] {
        assert!(!cannot_add_entries(&io::Error::from(other)));
    }
}

/// The reason for the access request: Windows refuses to flush a read-only directory handle.
#[cfg(windows)]
#[test]
fn windows_refuses_to_flush_a_read_only_directory_handle() -> io::Result<()> {
    use std::os::windows::fs::OpenOptionsExt;
    let fixture = Fixture::new("read-only-handle")?;
    let handle = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(0x0200_0000)
        .open(&fixture.0)?;
    let error = handle
        .sync_all()
        .err()
        .ok_or_else(|| io::Error::other("a read-only directory handle flushed"))?;
    assert_eq!(error.kind(), io::ErrorKind::PermissionDenied, "{error}");
    sync(&fixture.0)
}

/// The READONLY attribute does not stop adding entries to a Windows directory, nor its flush.
#[cfg(windows)]
#[test]
#[allow(clippy::permissions_set_readonly_false)] // Restores the owned fixture for removal.
fn windows_flushes_a_directory_marked_read_only() -> io::Result<()> {
    let fixture = Fixture::new("read-only-attribute")?;
    let mut permissions = std::fs::metadata(&fixture.0)?.permissions();
    permissions.set_readonly(true);
    std::fs::set_permissions(&fixture.0, permissions.clone())?;
    let flushed = sync(&fixture.0);
    permissions.set_readonly(false);
    std::fs::set_permissions(&fixture.0, permissions)?;
    flushed
}
