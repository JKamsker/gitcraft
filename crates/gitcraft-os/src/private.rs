//! Private supervisor artifacts and unpredictable capabilities.
use std::io;
use std::path::Path;

pub fn random_bytes(bytes: &mut [u8]) -> io::Result<()> {
    #[cfg(unix)]
    {
        // getentropy accepts at most 256 bytes per call.
        for part in bytes.chunks_mut(256) {
            // SAFETY: valid initialized writable slice and exact length.
            if unsafe { libc::getentropy(part.as_mut_ptr().cast(), part.len()) } != 0 {
                return Err(io::Error::last_os_error());
            }
        }
        Ok(())
    }
    #[cfg(windows)]
    {
        use windows::Win32::Security::Cryptography::{
            BCRYPT_USE_SYSTEM_PREFERRED_RNG, BCryptGenRandom,
        };
        // SAFETY: system RNG, no provider handle, writable slice owned by the caller.
        unsafe { BCryptGenRandom(None, bytes, BCRYPT_USE_SYSTEM_PREFERRED_RNG) }
            .ok()
            .map_err(|e| io::Error::other(e.to_string()))
    }
}

pub fn create_private_directory(path: &Path) -> io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        std::fs::DirBuilder::new().mode(0o700).create(path)
    }
    #[cfg(windows)]
    {
        windows_private(path)
    }
}

#[cfg(windows)]
fn windows_private(path: &Path) -> io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows::Win32::Foundation::{CloseHandle, HANDLE, HLOCAL, LocalFree};
    use windows::Win32::Security::Authorization::{
        ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW,
        SDDL_REVISION_1,
    };
    use windows::Win32::Security::{
        GetTokenInformation, PSECURITY_DESCRIPTOR, SECURITY_ATTRIBUTES, TOKEN_QUERY, TOKEN_USER,
        TokenUser,
    };
    use windows::Win32::Storage::FileSystem::CreateDirectoryW;
    use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};
    use windows::core::{PCWSTR, PWSTR};
    let fail = |e: windows::core::Error| io::Error::other(e.to_string());
    // SAFETY: obtain this process's own user SID; every acquired resource is released below.
    unsafe {
        let mut token = HANDLE::default();
        OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token).map_err(fail)?;
        let mut needed = 0;
        let _ = GetTokenInformation(token, TokenUser, None, 0, &mut needed);
        let mut words = vec![0usize; (needed as usize).div_ceil(std::mem::size_of::<usize>())];
        let got = GetTokenInformation(
            token,
            TokenUser,
            Some(words.as_mut_ptr().cast()),
            needed,
            &mut needed,
        );
        let _ = CloseHandle(token);
        got.map_err(fail)?;
        let user = &*(words.as_ptr().cast::<TOKEN_USER>());
        let mut sid = PWSTR::null();
        ConvertSidToStringSidW(user.User.Sid, &mut sid).map_err(fail)?;
        let sid_text = sid.to_string().map_err(|e| io::Error::other(e.to_string()));
        let _ = LocalFree(Some(HLOCAL(sid.0.cast())));
        let descriptor = format!("D:P(A;OICI;FA;;;{})", sid_text?);
        let wide: Vec<u16> = descriptor.encode_utf16().chain(Some(0)).collect();
        let mut security = PSECURITY_DESCRIPTOR::default();
        ConvertStringSecurityDescriptorToSecurityDescriptorW(
            PCWSTR(wide.as_ptr()),
            SDDL_REVISION_1,
            &mut security,
            None,
        )
        .map_err(fail)?;
        let attributes = SECURITY_ATTRIBUTES {
            nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: security.0,
            bInheritHandle: false.into(),
        };
        let name: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
        let result = CreateDirectoryW(PCWSTR(name.as_ptr()), Some(&attributes));
        let _ = LocalFree(Some(HLOCAL(security.0)));
        result.map_err(fail)
    }
}

/// Detach this freshly started controller before it starts user code.
pub fn detach_controller() -> io::Result<()> {
    #[cfg(unix)]
    {
        // SAFETY: affects only the calling process, during private helper startup.
        if unsafe { libc::setsid() } < 0 {
            return Err(io::Error::last_os_error());
        }
    }
    Ok(())
}
