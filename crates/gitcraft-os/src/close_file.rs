//! A checked final close for durable publication, consuming exactly one owned file.
use std::{fs::File, io};
pub fn close_file(file: File) -> io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::fd::IntoRawFd;
        let fd = file.into_raw_fd();
        // SAFETY: consume this owned descriptor exactly once. Never retry even EINTR;
        // the kernel may already have retired it, so a retry could close a reused fd.
        if unsafe { libc::close(fd) } < 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(())
        }
    }
    #[cfg(windows)]
    {
        use std::os::windows::io::IntoRawHandle;
        use windows::Win32::Foundation::{CloseHandle, HANDLE};
        let handle = file.into_raw_handle();
        // SAFETY: consume the retained file HANDLE exactly once, with no later access.
        unsafe { CloseHandle(HANDLE(handle)) }.map_err(|e| io::Error::other(e.to_string()))
    }
}
