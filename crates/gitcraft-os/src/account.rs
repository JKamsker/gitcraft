//! Account home as OpenSSH resolves it: the current UID's passwd entry, independent of HOME.

use std::ffi::{CStr, OsString};
use std::io;
use std::os::unix::ffi::OsStringExt;
use std::path::PathBuf;

pub fn account_home() -> io::Result<PathBuf> {
    // SAFETY: getuid has no arguments and cannot fail.
    let uid = unsafe { libc::getuid() };
    let mut size = 1024;
    loop {
        let mut bytes = vec![0u8; size];
        let mut entry = std::mem::MaybeUninit::<libc::passwd>::uninit();
        let mut found = std::ptr::null_mut();
        // SAFETY: entry/found point to writable objects; bytes owns size writable bytes
        // until all fields of the returned entry have been copied below.
        let error = unsafe {
            libc::getpwuid_r(
                uid,
                entry.as_mut_ptr(),
                bytes.as_mut_ptr().cast(),
                bytes.len(),
                &mut found,
            )
        };
        if error == libc::ERANGE && size < 1 << 20 {
            size *= 2;
            continue;
        }
        if error != 0 {
            return Err(io::Error::from_raw_os_error(error));
        }
        if found.is_null() {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                "current account has no passwd entry",
            ));
        }
        // SAFETY: successful getpwuid_r with non-null found initialized entry. pw_dir is
        // a NUL-terminated string backed by bytes (or null, checked before reading).
        let entry = unsafe { entry.assume_init() };
        if entry.pw_dir.is_null() {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                "current account has no home",
            ));
        }
        // SAFETY: pw_dir is a valid NUL-terminated field while bytes remains alive.
        let home = unsafe { CStr::from_ptr(entry.pw_dir) }
            .to_bytes()
            .to_owned();
        return Ok(PathBuf::from(OsString::from_vec(home)));
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn current_account_has_an_absolute_home() -> std::io::Result<()> {
        assert!(super::account_home()?.is_absolute());
        Ok(())
    }
}
