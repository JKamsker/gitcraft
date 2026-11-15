//! Descriptor-relative traversal. Linux NO_XDEV also rejects same-device bind mounts.
use rustix::fd::OwnedFd;
use rustix::fs::{self, AtFlags, Dir, FileType, Mode, OFlags};
use std::ffi::OsString;
use std::path::{Component, Path, PathBuf};

pub(in super::super) struct Purge {
    root: PathBuf,
    name: OsString,
    parent: OwnedFd,
    entry: OwnedFd,
}

const FLAGS: OFlags = OFlags::RDONLY
    .union(OFlags::CLOEXEC)
    .union(OFlags::DIRECTORY)
    .union(OFlags::NOFOLLOW);

fn open_root(path: &Path) -> Result<OwnedFd, String> {
    if !path.is_absolute() {
        return Err("Recovery root must be absolute".into());
    }
    let mut fd = fs::open("/", FLAGS, Mode::empty()).map_err(|e| e.to_string())?;
    for part in path.components() {
        match part {
            Component::RootDir => {}
            Component::Normal(name) => {
                fd = fs::openat(&fd, name, FLAGS, Mode::empty()).map_err(|e| e.to_string())?
            }
            _ => return Err("Invalid recovery ancestry".into()),
        }
    }
    Ok(fd)
}

fn child(parent: &OwnedFd, name: impl rustix::path::Arg) -> Result<OwnedFd, String> {
    #[cfg(target_os = "linux")]
    let fd = fs::openat2(
        parent,
        name,
        FLAGS,
        Mode::empty(),
        fs::ResolveFlags::BENEATH | fs::ResolveFlags::NO_SYMLINKS | fs::ResolveFlags::NO_XDEV,
    );
    #[cfg(not(target_os = "linux"))]
    let fd = fs::openat(parent, name, FLAGS, Mode::empty());
    let fd = fd.map_err(|e| format!("Purge directory boundary: {e}"))?;
    let parent_stat = fs::fstat(parent).map_err(|e| e.to_string())?;
    let stat = fs::fstat(&fd).map_err(|e| e.to_string())?;
    super::boundary(parent_stat.st_dev, stat.st_dev, 0)?;
    Ok(fd)
}

impl Purge {
    pub fn open(root: &Path, directory: &Path) -> Result<Self, String> {
        if directory.parent() != Some(root) {
            return Err("Invalid recovery item".into());
        }
        let name = directory
            .file_name()
            .ok_or("Missing recovery name")?
            .to_owned();
        let parent = open_root(root)?;
        let entry = child(&parent, name.as_os_str())?;
        Ok(Self {
            root: root.to_owned(),
            name,
            parent,
            entry,
        })
    }

    fn verify(&self) -> Result<(), String> {
        let current = open_root(&self.root)?;
        let held = fs::fstat(&self.parent).map_err(|e| e.to_string())?;
        let root = fs::fstat(&current).map_err(|e| e.to_string())?;
        let entry = fs::statat(&self.parent, &self.name, AtFlags::SYMLINK_NOFOLLOW)
            .map_err(|e| e.to_string())?;
        let opened = fs::fstat(&self.entry).map_err(|e| e.to_string())?;
        if (root.st_dev, root.st_ino) != (held.st_dev, held.st_ino)
            || (entry.st_dev, entry.st_ino) != (opened.st_dev, opened.st_ino)
        {
            return Err("Recovery identity or ancestry changed; purge refused".into());
        }
        Ok(())
    }

    pub fn remove(self, before: impl FnOnce() -> Result<(), String>) -> Result<(), String> {
        self.remove_after_check(before)
    }

    fn remove_after_check(self, after: impl FnOnce() -> Result<(), String>) -> Result<(), String> {
        self.verify()?;
        #[cfg(target_os = "linux")]
        super::mounts::check(&self.root.join(&self.name))?;
        walk(&self.entry, false, true, 0)?;
        self.verify()?;
        after()?;
        // Every recursive open repeats NO_XDEV/NOFOLLOW on the held parent, even
        // after preflight. Renames cannot redirect traversal to another ancestor.
        walk(&self.entry, true, true, 0)?;
        self.verify()?;
        fs::unlinkat(&self.entry, "record.json", AtFlags::empty()).map_err(|e| e.to_string())?;
        fs::unlinkat(&self.parent, &self.name, AtFlags::REMOVEDIR).map_err(|e| e.to_string())
    }
}

fn walk(fd: &OwnedFd, delete: bool, top: bool, depth: usize) -> Result<(), String> {
    if depth > 256 {
        return Err("Purge nesting limit exceeded".into());
    }
    let entries = Dir::read_from(fd).map_err(|e| e.to_string())?;
    for entry in entries {
        let entry = entry.map_err(|e| e.to_string())?;
        let name = entry.file_name();
        if matches!(name.to_bytes(), b"." | b"..") || (top && name.to_bytes() == b"record.json") {
            continue;
        }
        let stat = fs::statat(fd, name, AtFlags::SYMLINK_NOFOLLOW).map_err(|e| e.to_string())?;
        if FileType::from_raw_mode(stat.st_mode) == FileType::Directory {
            let opened = child(fd, name)?;
            #[cfg(test)]
            if delete {
                super::super::tests::traversal_hook();
            }
            walk(&opened, delete, false, depth + 1)?;
            if delete {
                fs::unlinkat(fd, name, AtFlags::REMOVEDIR).map_err(|e| e.to_string())?;
            }
        } else if delete {
            // unlinkat never follows a link and refuses an intervening directory/mount.
            fs::unlinkat(fd, name, AtFlags::empty()).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn ancestor_replacement_after_last_check_cannot_redirect_the_walk()
    -> Result<(), Box<dyn std::error::Error>> {
        let temp = tempfile::tempdir()?;
        let root = temp.path().canonicalize()?.join("recovery");
        let entry = root.join("tree-one");
        std::fs::create_dir_all(entry.join("tree"))?;
        std::fs::write(entry.join("record.json"), "record")?;
        std::fs::write(entry.join("tree/confirmed"), "purge this")?;
        let held = super::Purge::open(&root, &entry)?;
        let parked = root.with_extension("parked");
        let result = held.remove_after_check(|| {
            assert!(std::fs::rename(&root, &parked).is_ok());
            assert!(std::fs::create_dir_all(entry.join("tree")).is_ok());
            assert!(std::fs::write(entry.join("tree/precious"), "keep this").is_ok());
            Ok(())
        });
        assert!(result.is_err());
        assert_eq!(
            std::fs::read_to_string(entry.join("tree/precious"))?,
            "keep this"
        );
        assert!(!parked.join("tree-one/tree/confirmed").exists());
        assert!(parked.join("tree-one/record.json").exists());
        Ok(())
    }
}
