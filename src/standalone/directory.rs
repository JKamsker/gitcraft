//! Owned directory handles: save operations never resolve an output through a swapped parent.
use std::ffi::OsStr;
use std::fs::File;
#[cfg(not(unix))]
use std::fs::OpenOptions;
use std::io::{self, Write};
use std::path::{Component, Path, PathBuf};

#[derive(Debug)]
pub(super) struct Directory {
    pub path: PathBuf,
    handle: same_file::Handle,
    // Windows opens every canonical ancestor without FILE_SHARE_DELETE, pinning the path.
    #[cfg(windows)]
    _ancestors: Vec<File>,
}
impl Directory {
    #[cfg(target_os = "macos")]
    pub fn file(&self) -> &File {
        self.handle.as_file()
    }
    #[cfg(windows)]
    pub fn stable_source(&self, name: &OsStr) -> io::Result<File> {
        use std::os::windows::fs::{MetadataExt, OpenOptionsExt};
        check_name(name)?;
        let file = OpenOptions::new()
            .read(true)
            .share_mode(0x1)
            .custom_flags(0x0020_0000)
            .open(self.path.join(name))?;
        let meta = file.metadata()?;
        if !meta.is_file() || meta.file_attributes() & 0x400 != 0 {
            return Err(io::Error::other("The output changed type"));
        }
        Ok(file)
    }
    #[cfg(any(windows, target_os = "macos"))]
    pub fn rewrite_owned(
        &self,
        name: &OsStr,
        bytes: &[u8],
        original: &super::files::Snapshot,
    ) -> io::Result<File> {
        check_name(name)?;
        #[cfg(unix)]
        let mut file = {
            use rustix::fs::{Mode, OFlags, openat};
            File::from(openat(
                self.handle.as_file(),
                name,
                OFlags::WRONLY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
                Mode::empty(),
            )?)
        };
        #[cfg(windows)]
        let mut file = {
            use std::os::windows::fs::OpenOptionsExt;
            OpenOptions::new()
                .read(true)
                .write(true)
                .custom_flags(0x0020_0000)
                .open(self.path.join(name))?
        };
        let handle = same_file::Handle::from_file(file.try_clone()?)?;
        if !file.metadata()?.is_file()
            || original
                .handle
                .as_ref()
                .is_some_and(|h| h.as_ref() == &handle)
        {
            return Err(io::Error::other("Unsafe replacement file identity"));
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            if file.metadata()?.file_attributes() & 0x400 != 0 {
                return Err(io::Error::other("Replacement is a reparse point"));
            }
        }
        file.set_len(0)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        Ok(file)
    }
    pub fn open(path: &Path) -> io::Result<Self> {
        #[cfg(unix)]
        let file = {
            use rustix::fs::{Mode, OFlags, open};
            File::from(open(
                path,
                OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
                Mode::empty(),
            )?)
        };
        #[cfg(windows)]
        let (file, ancestors) = {
            use std::os::windows::fs::{MetadataExt, OpenOptionsExt};
            let mut at = PathBuf::new();
            let mut held = Vec::new();
            for component in path.components() {
                at.push(component.as_os_str());
                if matches!(component, Component::Prefix(_)) {
                    continue;
                }
                let file = OpenOptions::new()
                    .read(true)
                    .share_mode(0x1 | 0x2)
                    .custom_flags(0x0200_0000 | 0x0020_0000)
                    .open(&at)?;
                let meta = file.metadata()?;
                if !meta.is_dir() || meta.file_attributes() & 0x400 != 0 {
                    return Err(io::Error::other(
                        "The output parent contains a link or is not a directory",
                    ));
                }
                held.push(file);
            }
            let file = held
                .pop()
                .ok_or_else(|| io::Error::other("No output parent directory"))?;
            (file, held)
        };
        #[cfg(not(any(unix, windows)))]
        let file = File::open(path)?;
        Ok(Self {
            path: path.into(),
            handle: same_file::Handle::from_file(file)?,
            #[cfg(windows)]
            _ancestors: ancestors,
        })
    }
    pub fn unchanged(&self) -> bool {
        std::fs::symlink_metadata(&self.path)
            .is_ok_and(|m| m.is_dir() && !m.file_type().is_symlink())
            && Self::open(&self.path).is_ok_and(|now| now.handle == self.handle)
    }
    pub fn child(&self, name: &OsStr) -> io::Result<Self> {
        check_name(name)?;
        #[cfg(unix)]
        {
            use rustix::fs::{Mode, OFlags, mkdirat, openat};
            mkdirat(
                self.handle.as_file(),
                name,
                Mode::RUSR | Mode::WUSR | Mode::XUSR,
            )?;
            let fd = openat(
                self.handle.as_file(),
                name,
                OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
                Mode::empty(),
            )?;
            Ok(Self {
                path: self.path.join(name),
                handle: same_file::Handle::from_file(File::from(fd))?,
            })
        }
        #[cfg(not(unix))]
        {
            let path = self.path.join(name);
            std::fs::create_dir(&path)?;
            Self::open(&path)
        }
    }
    pub fn read(&self, name: &OsStr) -> io::Result<File> {
        check_name(name)?;
        #[cfg(unix)]
        {
            use rustix::fs::{Mode, OFlags, openat};
            Ok(File::from(openat(
                self.handle.as_file(),
                name,
                OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC,
                Mode::empty(),
            )?))
        }
        #[cfg(not(unix))]
        {
            let mut options = OpenOptions::new();
            options.read(true);
            #[cfg(windows)]
            {
                use std::os::windows::fs::OpenOptionsExt;
                options
                    .share_mode(0x1 | 0x2 | 0x4)
                    .custom_flags(0x0020_0000);
            }
            let file = options.open(self.path.join(name))?;
            #[cfg(windows)]
            {
                use std::os::windows::fs::MetadataExt;
                if file.metadata()?.file_attributes() & 0x400 != 0 {
                    return Err(io::Error::other("The merge output is a link/reparse point"));
                }
            }
            Ok(file)
        }
    }
    pub fn snapshot(&self, name: &OsStr, path: &Path) -> Result<super::files::Snapshot, String> {
        #[cfg(windows)]
        let _pin = self.stable_source(name).map_err(|e| e.to_string())?;
        // The temporary Windows pin excludes path replacement during descriptor
        // reads, but the retained identity handle must permit our later rename.
        let file = self.read(name);
        super::files::Snapshot::output(path, file.map_err(|e| e.to_string())?)
    }
    pub fn create(&self, name: &OsStr, bytes: &[u8]) -> io::Result<File> {
        check_name(name)?;
        #[cfg(unix)]
        let mut file = {
            use rustix::fs::{Mode, OFlags, openat};
            File::from(openat(
                self.handle.as_file(),
                name,
                OFlags::WRONLY | OFlags::CREATE | OFlags::EXCL | OFlags::NOFOLLOW | OFlags::CLOEXEC,
                Mode::RUSR | Mode::WUSR,
            )?)
        };
        #[cfg(not(unix))]
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(self.path.join(name))?;
        file.write_all(bytes)?;
        file.sync_all()?;
        Ok(file)
    }
    pub fn move_file(&self, name: &OsStr, to: &Self, dest: &OsStr) -> io::Result<()> {
        check_name(name)?;
        check_name(dest)?;
        // Destination is an absent constant basename inside our exclusively created sibling.
        #[cfg(unix)]
        {
            rustix::fs::renameat(self.handle.as_file(), name, to.handle.as_file(), dest)?;
            Ok(())
        }
        #[cfg(not(unix))]
        {
            std::fs::rename(self.path.join(name), to.path.join(dest))
        }
    }
    pub fn link(&self, name: &OsStr, to: &Self, dest: &OsStr) -> io::Result<()> {
        check_name(name)?;
        check_name(dest)?;
        #[cfg(unix)]
        {
            rustix::fs::linkat(
                self.handle.as_file(),
                name,
                to.handle.as_file(),
                dest,
                rustix::fs::AtFlags::empty(),
            )?;
            Ok(())
        }
        #[cfg(not(unix))]
        {
            std::fs::hard_link(self.path.join(name), to.path.join(dest))
        }
    }
    pub fn remove_file(&self, name: &OsStr) -> io::Result<()> {
        check_name(name)?;
        #[cfg(unix)]
        {
            rustix::fs::unlinkat(self.handle.as_file(), name, rustix::fs::AtFlags::empty())?;
            Ok(())
        }
        #[cfg(not(unix))]
        {
            std::fs::remove_file(self.path.join(name))
        }
    }
    pub fn remove_child(&self, name: &OsStr) -> io::Result<()> {
        check_name(name)?;
        #[cfg(unix)]
        {
            rustix::fs::unlinkat(self.handle.as_file(), name, rustix::fs::AtFlags::REMOVEDIR)?;
            Ok(())
        }
        #[cfg(not(unix))]
        {
            std::fs::remove_dir(self.path.join(name))
        }
    }
}
fn check_name(name: &OsStr) -> io::Result<()> {
    let mut components = Path::new(name).components();
    if matches!(components.next(), Some(Component::Normal(_))) && components.next().is_none() {
        Ok(())
    } else {
        Err(io::Error::other("Expected one native file basename"))
    }
}
