//! Handle-relative access to a private directory. Used by native receipt retirement and
//! workspace coordination so namespace replacement cannot redirect a later file open.
use std::fs::File;
use std::io;
use std::path::Path;

pub struct PrivateDirectory {
    file: File,
}
#[derive(Clone, Copy)]
pub enum PrivateFileMode {
    Read,
    Update,
    CreateNew,
    CreateOrOpen,
    ReadDelete,
}
impl PrivateDirectory {
    pub fn open(path: &Path) -> io::Result<Self> {
        #[cfg(unix)]
        {
            use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
            let file = std::fs::OpenOptions::new()
                .read(true)
                .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
                .open(path)?;
            let meta = file.metadata()?;
            // SAFETY: process credential query, no arguments or mutable state.
            if !meta.is_dir()
                || meta.uid() != unsafe { libc::geteuid() }
                || meta.mode() & 0o777 != 0o700
            {
                return Err(io::Error::other("directory is not private to this user"));
            }
            Ok(Self { file })
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::{MetadataExt, OpenOptionsExt};
            let file = std::fs::OpenOptions::new()
                .read(true)
                .share_mode(3)
                .custom_flags(0x02000000 | 0x00200000)
                .open(path)?;
            let meta = file.metadata()?;
            if !meta.is_dir() || meta.file_attributes() & 0x400 != 0 {
                return Err(io::Error::other("directory is not plain"));
            }
            crate::receipt_retirement::private_acl(&file)?;
            Ok(Self { file })
        }
    }
    pub fn identity(&self) -> io::Result<String> {
        file_identity(&self.file)
    }
    pub fn open_file(&self, name: &str, mode: PrivateFileMode) -> io::Result<File> {
        if !single_name(name) {
            return Err(io::Error::other("invalid private file name"));
        }
        let file = self.open_relative(name, mode)?;
        let meta = file.metadata()?;
        if !meta.is_file() {
            return Err(io::Error::other("private entry is not a regular file"));
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            // SAFETY: process credential query only.
            if meta.uid() != unsafe { libc::geteuid() }
                || meta.mode() & 0o077 != 0
                || meta.nlink() != 1
            {
                return Err(io::Error::other("entry is not a private single-link file"));
            }
        }
        #[cfg(windows)]
        {
            use std::os::windows::{fs::MetadataExt, io::AsRawHandle};
            use windows::Win32::{
                Foundation::HANDLE,
                Storage::FileSystem::{BY_HANDLE_FILE_INFORMATION, GetFileInformationByHandle},
            };
            let mut info = BY_HANDLE_FILE_INFORMATION::default();
            // SAFETY: live retained file HANDLE and initialized output structure.
            unsafe { GetFileInformationByHandle(HANDLE(file.as_raw_handle()), &mut info) }
                .map_err(|e| io::Error::other(e.to_string()))?;
            if meta.file_attributes() & 0x400 != 0 || info.nNumberOfLinks != 1 {
                return Err(io::Error::other(
                    "entry is a reparse point or has multiple links",
                ));
            }
        }
        Ok(file)
    }
    #[cfg(unix)]
    fn open_relative(&self, name: &str, mode: PrivateFileMode) -> io::Result<File> {
        use std::ffi::CString;
        use std::os::fd::{AsRawFd, FromRawFd};
        let access = match mode {
            PrivateFileMode::Read | PrivateFileMode::ReadDelete => libc::O_RDONLY,
            _ => libc::O_RDWR,
        };
        let create = match mode {
            PrivateFileMode::CreateNew => libc::O_CREAT | libc::O_EXCL,
            PrivateFileMode::CreateOrOpen => libc::O_CREAT,
            _ => 0,
        };
        let name = CString::new(name)?;
        // SAFETY: single fixed component relative to retained directory, no symlink or
        // FIFO following. Returned descriptor is immediately uniquely owned.
        let fd = unsafe {
            libc::openat(
                self.file.as_raw_fd(),
                name.as_ptr(),
                access | create | libc::O_CLOEXEC | libc::O_NOFOLLOW | libc::O_NONBLOCK,
                0o600,
            )
        };
        if fd < 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(unsafe { File::from_raw_fd(fd) })
    }
    #[cfg(windows)]
    fn open_relative(&self, name: &str, mode: PrivateFileMode) -> io::Result<File> {
        use std::os::windows::io::{AsRawHandle, FromRawHandle};
        use windows::{
            Wdk::{Foundation::OBJECT_ATTRIBUTES, Storage::FileSystem::*},
            Win32::{
                Foundation::{HANDLE, UNICODE_STRING},
                Storage::FileSystem::{
                    DELETE, FILE_FLAGS_AND_ATTRIBUTES, FILE_GENERIC_READ, FILE_GENERIC_WRITE,
                    FILE_SHARE_READ, FILE_SHARE_WRITE,
                },
                System::IO::IO_STATUS_BLOCK,
            },
            core::PWSTR,
        };
        let mut name: Vec<u16> = name.encode_utf16().collect();
        let text = UNICODE_STRING {
            Length: (name.len() * 2) as u16,
            MaximumLength: (name.len() * 2) as u16,
            Buffer: PWSTR(name.as_mut_ptr()),
        };
        let attributes = OBJECT_ATTRIBUTES {
            Length: std::mem::size_of::<OBJECT_ATTRIBUTES>() as u32,
            RootDirectory: HANDLE(self.file.as_raw_handle()),
            ObjectName: &text,
            ..Default::default()
        };
        let (access, share) = match mode {
            PrivateFileMode::Read => (FILE_GENERIC_READ, FILE_SHARE_READ | FILE_SHARE_WRITE),
            PrivateFileMode::ReadDelete => (FILE_GENERIC_READ | DELETE, FILE_SHARE_READ),
            _ => (
                FILE_GENERIC_READ | FILE_GENERIC_WRITE,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
            ),
        };
        let disposition = match mode {
            PrivateFileMode::CreateNew => FILE_CREATE,
            PrivateFileMode::CreateOrOpen => FILE_OPEN_IF,
            _ => FILE_OPEN,
        };
        let mut handle = HANDLE::default();
        let mut status = IO_STATUS_BLOCK::default();
        // SAFETY: single relative name, retained RootDirectory HANDLE and live output
        // structures. Attributes exclude inheritance. Synchronous regular-file handle;
        // reparse processing is disabled and opened reparse objects are rejected above.
        let result = unsafe {
            NtCreateFile(
                &mut handle,
                access,
                &attributes,
                &mut status,
                None,
                FILE_FLAGS_AND_ATTRIBUTES(0),
                share,
                disposition,
                FILE_NON_DIRECTORY_FILE | FILE_OPEN_REPARSE_POINT | FILE_SYNCHRONOUS_IO_NONALERT,
                None,
                0,
            )
        };
        if result.0 < 0 {
            // SAFETY: numeric status translation only. Preserve NotFound/AlreadyExists
            // so coordination can distinguish first initialization from replacement.
            let code = unsafe { windows::Win32::Foundation::RtlNtStatusToDosError(result) };
            return Err(io::Error::from_raw_os_error(code as i32));
        }
        Ok(unsafe { File::from_raw_handle(handle.0) })
    }
    /// Remove this namespace's fixed terminal receipt; Windows deletes its retained HANDLE.
    pub(crate) fn retire_terminal(&self, terminal: &File) -> io::Result<()> {
        #[cfg(unix)]
        {
            use std::os::fd::AsRawFd;
            // SAFETY: fixed component relative to owned directory; no parent lookup.
            if unsafe { libc::unlinkat(self.file.as_raw_fd(), c"terminal".as_ptr(), 0) } != 0 {
                return Err(io::Error::last_os_error());
            }
            let _ = terminal;
            Ok(())
        }
        #[cfg(windows)]
        {
            use std::os::windows::io::AsRawHandle;
            use windows::Win32::{
                Foundation::HANDLE,
                Storage::FileSystem::{
                    FILE_DISPOSITION_INFO, FileDispositionInfo, SetFileInformationByHandle,
                },
            };
            let info = FILE_DISPOSITION_INFO { DeleteFile: true };
            // SAFETY: exact opened receipt HANDLE, correct input type and size.
            unsafe {
                SetFileInformationByHandle(
                    HANDLE(terminal.as_raw_handle()),
                    FileDispositionInfo,
                    (&info as *const FILE_DISPOSITION_INFO).cast(),
                    std::mem::size_of_val(&info) as u32,
                )
            }
            .map_err(|e| io::Error::other(e.to_string()))
        }
    }
}
fn single_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 192
        && name != "."
        && name != ".."
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
}
pub fn file_identity(file: &File) -> io::Result<String> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let meta = file.metadata()?;
        Ok(format!("unix:{:016x}:{:016x}", meta.dev(), meta.ino()))
    }
    #[cfg(windows)]
    {
        use std::os::windows::io::AsRawHandle;
        use windows::Win32::{
            Foundation::HANDLE,
            Storage::FileSystem::{FILE_ID_INFO, FileIdInfo, GetFileInformationByHandleEx},
        };
        let mut info = FILE_ID_INFO::default();
        // SAFETY: retained file HANDLE and exactly sized initialized output buffer.
        unsafe {
            GetFileInformationByHandleEx(
                HANDLE(file.as_raw_handle()),
                FileIdInfo,
                (&mut info as *mut FILE_ID_INFO).cast(),
                std::mem::size_of_val(&info) as u32,
            )
        }
        .map_err(|e| io::Error::other(e.to_string()))?;
        Ok(format!(
            "windows:{:016x}:{}",
            info.VolumeSerialNumber,
            info.FileId
                .Identifier
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>()
        ))
    }
}
#[cfg(test)]
mod recording_tests {
    use super::*;
    #[test]
    fn relative_names_cannot_escape_or_select_streams_devices_or_parents() {
        for name in [
            "",
            ".",
            "..",
            "a/b",
            "a\\b",
            "C:stream",
            "/absolute",
            "nul\0byte",
            "a\nfile",
        ] {
            assert!(!single_name(name));
        }
        for name in ["primary", "generation", "0-abcd.owner", "workspace.anchor"] {
            assert!(single_name(name));
        }
    }
}
