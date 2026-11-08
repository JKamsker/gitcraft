//! Delete only a terminal receipt opened relative to a retained private directory.
//! Empty directories stay: a final pathname removal would reopen a substitution race.
use std::fs::File;
use std::io::{self, Read};
use std::path::Path;

trait Retirement {
    fn receipt(&mut self) -> io::Result<Vec<u8>>;
    fn remove(&mut self) -> io::Result<()>;
}
fn retire(owner: &mut impl Retirement, verify: impl FnOnce(&[u8]) -> bool) -> io::Result<()> {
    if !verify(&owner.receipt()?) {
        return Err(io::Error::other("unconfirmed terminal receipt"));
    }
    owner.remove()
}
pub fn retire_private_receipt(path: &Path, verify: impl FnOnce(&[u8]) -> bool) -> io::Result<()> {
    retire(&mut Pinned::open(path)?, verify)
}
struct Pinned {
    directory: File,
    terminal: File,
}
fn bounded(file: &mut File) -> io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    file.take(4097).read_to_end(&mut bytes)?;
    if bytes.len() > 4096 {
        return Err(io::Error::other("oversized terminal receipt"));
    }
    Ok(bytes)
}
#[cfg(unix)]
impl Pinned {
    fn open(path: &Path) -> io::Result<Self> {
        use std::ffi::CString;
        use std::os::fd::{AsRawFd, FromRawFd};
        use std::os::unix::{ffi::OsStrExt, fs::MetadataExt};
        let path = CString::new(path.as_os_str().as_bytes())?;
        // SAFETY: terminated path; returned descriptors are immediately uniquely owned.
        let directory = unsafe {
            let fd = libc::open(
                path.as_ptr(),
                libc::O_RDONLY | libc::O_CLOEXEC | libc::O_DIRECTORY | libc::O_NOFOLLOW,
            );
            if fd < 0 {
                return Err(io::Error::last_os_error());
            }
            File::from_raw_fd(fd)
        };
        let meta = directory.metadata()?;
        // SAFETY: geteuid has no inputs or side effects.
        let uid = unsafe { libc::geteuid() };
        if !meta.is_dir() || meta.uid() != uid || meta.mode() & 0o777 != 0o700 {
            return Err(io::Error::other("endpoint directory is not private"));
        }
        // SAFETY: fixed single component relative to the held directory descriptor.
        let terminal = unsafe {
            let fd = libc::openat(
                directory.as_raw_fd(),
                c"terminal".as_ptr(),
                libc::O_RDONLY | libc::O_CLOEXEC | libc::O_NOFOLLOW | libc::O_NONBLOCK,
            );
            if fd < 0 {
                return Err(io::Error::last_os_error());
            }
            File::from_raw_fd(fd)
        };
        let meta = terminal.metadata()?;
        if !meta.is_file() || meta.uid() != uid || meta.mode() & 0o077 != 0 || meta.len() > 4096 {
            return Err(io::Error::other(
                "receipt is not a bounded private regular file",
            ));
        }
        Ok(Self {
            directory,
            terminal,
        })
    }
}
#[cfg(unix)]
impl Retirement for Pinned {
    fn receipt(&mut self) -> io::Result<Vec<u8>> {
        bounded(&mut self.terminal)
    }
    fn remove(&mut self) -> io::Result<()> {
        use std::os::fd::AsRawFd;
        // SAFETY: fixed name; no parent pathname is resolved or symlink followed.
        if unsafe { libc::unlinkat(self.directory.as_raw_fd(), c"terminal".as_ptr(), 0) } != 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }
}
#[cfg(windows)]
impl Pinned {
    fn open(path: &Path) -> io::Result<Self> {
        use std::os::windows::fs::{MetadataExt, OpenOptionsExt};
        use windows::Win32::Storage::FileSystem::{DELETE, FILE_GENERIC_READ};
        const NO_FOLLOW: u32 = 0x00200000;
        const FOLDER: u32 = 0x02000000;
        const REPARSE: u32 = 0x400;
        let directory = std::fs::OpenOptions::new()
            .read(true)
            .share_mode(3)
            .custom_flags(NO_FOLLOW | FOLDER)
            .open(path)?;
        let meta = directory.metadata()?;
        if !meta.is_dir() || meta.file_attributes() & REPARSE != 0 {
            return Err(io::Error::other("endpoint directory is not plain"));
        }
        private_acl(&directory)?;
        // Deny writers and replacement until the exact opened receipt is marked deleted.
        let terminal = std::fs::OpenOptions::new()
            .access_mode(FILE_GENERIC_READ.0 | DELETE.0)
            .share_mode(1)
            .custom_flags(NO_FOLLOW)
            .open(path.join("terminal"))?;
        let meta = terminal.metadata()?;
        if !meta.is_file()
            || meta.file_attributes() & REPARSE != 0
            || meta.len() > 4096
            || final_path(&terminal)?.parent() != Some(final_path(&directory)?.as_path())
        {
            return Err(io::Error::other(
                "receipt is not inside the held endpoint directory",
            ));
        }
        Ok(Self {
            directory,
            terminal,
        })
    }
}
#[cfg(windows)]
impl Retirement for Pinned {
    fn receipt(&mut self) -> io::Result<Vec<u8>> {
        bounded(&mut self.terminal)
    }
    fn remove(&mut self) -> io::Result<()> {
        use std::os::windows::io::AsRawHandle;
        use windows::Win32::Foundation::HANDLE;
        use windows::Win32::Storage::FileSystem::{
            FILE_DISPOSITION_INFO, FileDispositionInfo, SetFileInformationByHandle,
        };
        let _held_directory = &self.directory;
        let info = FILE_DISPOSITION_INFO { DeleteFile: true };
        // SAFETY: live retained file HANDLE, correct information type and size. Directory
        // remains held against rename; deletion targets this file HANDLE, never its path.
        unsafe {
            SetFileInformationByHandle(
                HANDLE(self.terminal.as_raw_handle()),
                FileDispositionInfo,
                (&info as *const FILE_DISPOSITION_INFO).cast(),
                std::mem::size_of_val(&info) as u32,
            )
        }
        .map_err(|e| io::Error::other(e.to_string()))
    }
}
#[cfg(windows)]
fn final_path(file: &File) -> io::Result<std::path::PathBuf> {
    use std::os::windows::{ffi::OsStringExt, io::AsRawHandle};
    use windows::Win32::{
        Foundation::HANDLE,
        Storage::FileSystem::{FILE_NAME_NORMALIZED, GetFinalPathNameByHandleW},
    };
    let mut buf = vec![0; 32768];
    // SAFETY: retained handle and initialized writable UTF-16 output buffer.
    let n = unsafe {
        GetFinalPathNameByHandleW(HANDLE(file.as_raw_handle()), &mut buf, FILE_NAME_NORMALIZED)
    } as usize;
    if n == 0 || n >= buf.len() {
        return Err(io::Error::last_os_error());
    }
    buf.truncate(n);
    Ok(std::ffi::OsString::from_wide(&buf).into())
}
#[cfg(windows)]
fn private_acl(file: &File) -> io::Result<()> {
    use std::os::windows::io::AsRawHandle;
    use windows::Win32::{
        Foundation::{CloseHandle, HANDLE, HLOCAL, LocalFree},
        Security::{
            Authorization::{GetSecurityInfo, SE_FILE_OBJECT},
            *,
        },
        System::Threading::{GetCurrentProcess, OpenProcessToken},
    };
    let fail = |e: windows::core::Error| io::Error::other(e.to_string());
    // SAFETY: all pointers refer to owned Win32 outputs/buffers and stay live through
    // validation. Security descriptor and token are released on every result path.
    unsafe {
        let mut token = HANDLE::default();
        OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token).map_err(fail)?;
        let result = (|| {
            let mut needed = 0;
            let _ = GetTokenInformation(token, TokenUser, None, 0, &mut needed);
            let mut words = vec![0usize; (needed as usize).div_ceil(std::mem::size_of::<usize>())];
            GetTokenInformation(
                token,
                TokenUser,
                Some(words.as_mut_ptr().cast()),
                needed,
                &mut needed,
            )
            .map_err(fail)?;
            let user = &*words.as_ptr().cast::<TOKEN_USER>();
            let mut descriptor = PSECURITY_DESCRIPTOR::default();
            let mut owner = PSID::default();
            let mut acl = std::ptr::null_mut();
            GetSecurityInfo(
                HANDLE(file.as_raw_handle()),
                SE_FILE_OBJECT,
                OWNER_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION,
                Some(&mut owner),
                None,
                Some(&mut acl),
                None,
                Some(&mut descriptor),
            )
            .ok()
            .map_err(fail)?;
            let checked = (|| {
                EqualSid(owner, user.User.Sid).map_err(fail)?;
                let mut control = 0;
                let mut revision = 0;
                GetSecurityDescriptorControl(descriptor, &mut control, &mut revision)
                    .map_err(fail)?;
                if control & SE_DACL_PROTECTED.0 == 0 || acl.is_null() || (*acl).AceCount != 1 {
                    return Err(io::Error::other("endpoint DACL is not private"));
                }
                let mut ace = std::ptr::null_mut();
                GetAce(acl, 0, &mut ace).map_err(fail)?;
                let ace = &*ace.cast::<ACCESS_ALLOWED_ACE>();
                if ace.Header.AceType != 0 || ace.Header.AceSize < 16 {
                    return Err(io::Error::other("endpoint ACE is not a user allow entry"));
                }
                let sid = (&ace.SidStart as *const u32).cast::<u8>();
                let sid_len = 8usize + usize::from(*sid.add(1)) * 4;
                if usize::from(ace.Header.AceSize) < 8 + sid_len {
                    return Err(io::Error::other("endpoint ACE has an incomplete SID"));
                }
                EqualSid(PSID(sid.cast_mut().cast()), user.User.Sid).map_err(fail)
            })();
            let _ = LocalFree(Some(HLOCAL(descriptor.0)));
            checked
        })();
        let _ = CloseHandle(token);
        result
    }
}
#[cfg(test)]
mod recording_tests {
    use super::*;
    #[test]
    fn replacement_never_changes_retirement_identity_and_missing_proof_erases_nothing() {
        struct Fake {
            held: &'static str,
            path: &'static str,
            removed: Vec<&'static str>,
        }
        impl Retirement for Fake {
            fn receipt(&mut self) -> io::Result<Vec<u8>> {
                self.path = "foreign";
                Ok(b"receipt".to_vec())
            }
            fn remove(&mut self) -> io::Result<()> {
                self.path = "foreign-again";
                self.removed.push(self.held);
                Ok(())
            }
        }
        let mut fake = Fake {
            held: "original",
            path: "original",
            removed: vec![],
        };
        assert!(retire(&mut fake, |_| false).is_err());
        assert!(fake.removed.is_empty());
        retire(&mut fake, |b| b == b"receipt").unwrap();
        assert_eq!(fake.removed, ["original"]);
    }
}
