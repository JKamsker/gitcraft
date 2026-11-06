//! Reading a file of a folder (a repository's working tree) so that it is that folder's own
//! file, whatever happens on the disk meanwhile: no link on the way is followed, and what is
//! checked (inside the folder, a plain file, small enough) is the file that was opened, not a
//! path looked at before. A link put in place of a folder between a check and a read would
//! otherwise make the read go anywhere.

use std::io::Read;
use std::path::Path;

/// The file `rel` (`/` between names) of the folder `root`, when it is a plain file there, at
/// most `max` bytes, and reached without going through a link; nothing otherwise.
pub fn read_inside(root: &Path, rel: &str, max: u64) -> Option<Vec<u8>> {
    let names: Vec<&str> = rel.split('/').filter(|n| !n.is_empty()).collect();
    if names.is_empty() || names.iter().any(|n| matches!(*n, "." | "..")) {
        return None;
    }
    let file = open_inside(root, &names, max)?;
    let mut bytes = Vec::new();
    // Never more than the limit, even when the file grows meanwhile.
    file.take(max + 1).read_to_end(&mut bytes).ok()?;
    (bytes.len() as u64 <= max).then_some(bytes)
}

/// Each folder opened inside the one before it, never through a link, and the file at the end
/// likewise; then the opened file itself is checked.
#[cfg(unix)]
fn open_inside(root: &Path, names: &[&str], max: u64) -> Option<std::fs::File> {
    use rustix::fs::{FileType, Mode, OFlags, fstat, open, openat};
    let flags = OFlags::RDONLY | OFlags::CLOEXEC | OFlags::NOFOLLOW;
    let (last, folders) = names.split_last()?;
    let mut dir = open(
        root,
        OFlags::RDONLY | OFlags::CLOEXEC | OFlags::DIRECTORY,
        Mode::empty(),
    )
    .ok()?;
    for name in folders {
        dir = openat(&dir, *name, flags | OFlags::DIRECTORY, Mode::empty()).ok()?;
    }
    // Not waiting on a pipe put there: what isn't a plain file is refused below anyway.
    let fd = openat(&dir, *last, flags | OFlags::NONBLOCK, Mode::empty()).ok()?;
    let stat = fstat(&fd).ok()?;
    let plain = FileType::from_raw_mode(stat.st_mode) == FileType::RegularFile;
    (plain && u64::try_from(stat.st_size).is_ok_and(|size| size <= max))
        .then(|| std::fs::File::from(fd))
}

/// Each folder opened without following a link and held open, so that it can be neither
/// renamed nor deleted (no `FILE_SHARE_DELETE`) until the file is read: the path to the file
/// stays the one checked. The file is opened without following a link either.
#[cfg(windows)]
fn open_inside(root: &Path, names: &[&str], max: u64) -> Option<std::fs::File> {
    use std::os::windows::fs::{MetadataExt, OpenOptionsExt};
    /// Sharing with readers and writers, never with renaming or deleting.
    const SHARE_READ_WRITE: u32 = 0x1 | 0x2;
    /// Opens a folder (`FILE_FLAG_BACKUP_SEMANTICS`).
    const FOLDER: u32 = 0x0200_0000;
    /// The link itself rather than what it points to (`FILE_FLAG_OPEN_REPARSE_POINT`).
    const NO_FOLLOW: u32 = 0x0020_0000;
    /// `FILE_ATTRIBUTE_REPARSE_POINT`: a link, a junction, a mount point…
    const REPARSE_POINT: u32 = 0x400;
    let open = |path: &Path, flags: u32| {
        std::fs::OpenOptions::new()
            .read(true)
            .share_mode(SHARE_READ_WRITE)
            .custom_flags(flags)
            .open(path)
            .ok()
    };
    let plain = |file: &std::fs::File| {
        file.metadata()
            .ok()
            .filter(|m| !m.file_type().is_symlink() && m.file_attributes() & REPARSE_POINT == 0)
    };
    let (last, folders) = names.split_last()?;
    // The repository's own folder may be reached through a link; what is in it may not.
    let mut held = vec![open(root, FOLDER)?];
    let mut path = root.to_path_buf();
    for name in folders {
        path.push(name);
        let folder = open(&path, FOLDER | NO_FOLLOW)?;
        if !plain(&folder)?.is_dir() {
            return None;
        }
        held.push(folder);
    }
    path.push(last);
    let file = open(&path, NO_FOLLOW)?;
    let meta = plain(&file)?;
    drop(held);
    (meta.is_file() && meta.len() <= max).then_some(file)
}

#[cfg(not(any(unix, windows)))]
fn open_inside(_: &Path, _: &[&str], _: u64) -> Option<std::fs::File> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("gitcraft-safe-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::create_dir_all(dir.join("docs"));
        dir
    }

    #[test]
    fn a_plain_file_inside_is_read_within_its_limit() -> std::io::Result<()> {
        let dir = scratch("plain");
        std::fs::write(dir.join("docs/s.png"), "picture")?;
        let found = (
            read_inside(&dir, "docs/s.png", 100),
            read_inside(&dir, "docs/s.png", 3),
            read_inside(&dir, "docs/none.png", 100),
            read_inside(&dir, "docs/../docs/s.png", 100),
            read_inside(&dir, "docs", 100),
            read_inside(&dir, "", 100),
        );
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(
            found,
            (Some(b"picture".to_vec()), None, None, None, None, None)
        );
        Ok(())
    }

    /// A folder or a file that is a link is never gone through, whenever it became one: the
    /// opened file is what is checked.
    #[cfg(unix)]
    #[test]
    fn links_are_never_followed() -> std::io::Result<()> {
        let dir = scratch("links");
        let outside = scratch("outside");
        std::fs::write(outside.join("secret.png"), "secret")?;
        std::os::unix::fs::symlink(&outside, dir.join("docs/l"))?;
        std::os::unix::fs::symlink(outside.join("secret.png"), dir.join("docs/s.png"))?;
        let found = (
            read_inside(&dir, "docs/l/secret.png", 100),
            read_inside(&dir, "docs/s.png", 100),
        );
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_dir_all(&outside);
        assert_eq!(found, (None, None));
        Ok(())
    }

    /// A folder swapped for a link while the files are read: no read ever comes from outside,
    /// whichever moment the swap falls on.
    #[cfg(unix)]
    #[test]
    fn a_folder_swapped_for_a_link_meanwhile_is_never_read_through() -> std::io::Result<()> {
        use std::sync::Arc;
        use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
        let dir = scratch("swap");
        let outside = scratch("swap-outside");
        std::fs::create_dir_all(outside.join("docs"))?;
        std::fs::write(outside.join("docs/s.png"), "secret")?;
        std::fs::write(dir.join("docs/s.png"), "picture")?;
        let stop = Arc::new(AtomicBool::new(false));
        let swaps = Arc::new(AtomicUsize::new(0));
        let swapper = {
            let (dir, outside) = (dir.clone(), outside.clone());
            let (stop, swaps) = (stop.clone(), swaps.clone());
            std::thread::spawn(move || {
                let (real, moved) = (dir.join("docs"), dir.join("docs-real"));
                while !stop.load(Ordering::Relaxed) {
                    let _ = std::fs::rename(&real, &moved);
                    let _ = std::os::unix::fs::symlink(outside.join("docs"), &real);
                    let _ = std::fs::remove_file(&real);
                    let _ = std::fs::rename(&moved, &real);
                    swaps.fetch_add(1, Ordering::Relaxed);
                }
            })
        };
        // Reads all the while the folder is being swapped, many times over.
        let (mut leaked, mut reads) = (0, 0);
        while reads < 2_000 || swaps.load(Ordering::Relaxed) < 2_000 {
            if read_inside(&dir, "docs/s.png", 100).as_deref() == Some(b"secret".as_slice()) {
                leaked += 1;
            }
            reads += 1;
        }
        stop.store(true, Ordering::Relaxed);
        let _ = swapper.join();
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_dir_all(&outside);
        assert_eq!(leaked, 0);
        Ok(())
    }
}
