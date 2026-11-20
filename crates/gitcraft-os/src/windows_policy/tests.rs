//! Real reparse points on owned temporary files: a non-surrogate one keeps its policy usable,
//! a name surrogate is refused. Third-party tags need no privilege to set on one's own file.
use super::*;
use std::os::windows::fs::OpenOptionsExt;
use std::path::PathBuf;
use windows::Win32::System::IO::DeviceIoControl;

const FSCTL_SET_REPARSE_POINT: u32 = 0x0009_00A4;
const GENERIC_WRITE: u32 = 0x4000_0000;

struct Dir(PathBuf);
impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn temp(name: &str) -> std::io::Result<Dir> {
    let dir =
        std::env::temp_dir().join(format!("gitcraft-os-reparse-{}-{name}", std::process::id()));
    std::fs::create_dir(&dir)?;
    Ok(Dir(dir))
}

/// Attach a third-party reparse point (`REPARSE_GUID_DATA_BUFFER`) with `tag` to `path`.
fn set_reparse(path: &Path, tag: u32) -> std::io::Result<()> {
    let file = std::fs::OpenOptions::new()
        .access_mode(GENERIC_WRITE)
        .custom_flags(NO_FOLLOW)
        .open(path)?;
    let data = *b"gitcraft";
    let mut buffer = tag.to_le_bytes().to_vec();
    buffer.extend_from_slice(&(data.len() as u16).to_le_bytes());
    buffer.extend_from_slice(&[0, 0]);
    buffer.extend_from_slice(&[0x5a; 16]); // ReparseGuid
    buffer.extend_from_slice(&data);
    // SAFETY: the live handle of `file`; the input buffer is live and exactly `len` bytes;
    // no output buffer or overlapped structure is used.
    unsafe {
        DeviceIoControl(
            HANDLE(file.as_raw_handle()),
            FSCTL_SET_REPARSE_POINT,
            Some(buffer.as_ptr().cast()),
            buffer.len() as u32,
            None,
            0,
            None,
            None,
        )
    }
    .map_err(|e| std::io::Error::other(e.to_string()))
}

#[test]
fn a_non_surrogate_reparse_file_keeps_its_policy_and_a_name_surrogate_is_refused()
-> std::io::Result<()> {
    let dir = temp("tags")?;
    let placeholder = dir.0.join("placeholder");
    let surrogate = dir.0.join("surrogate");
    let plain = dir.0.join("plain");
    for path in [&placeholder, &surrogate, &plain] {
        std::fs::write(path, b"data")?;
    }
    set_reparse(&placeholder, 0x0000_1234)?;
    set_reparse(&surrogate, 0x2000_1234)?;
    let (attributes, tag) = streams::attribute_tag(
        &open(&placeholder, read_access(0x7))
            .map_err(|e| std::io::Error::other(format!("{e:?}")))?,
    )
    .map_err(|e| std::io::Error::other(format!("{e:?}")))?;
    assert_ne!(
        attributes & crate::reparse::REPARSE_POINT,
        0,
        "fixture is a reparse point"
    );
    assert_eq!(tag, 0x0000_1234);
    let snapshot = file_policy_snapshot(&placeholder);
    assert!(snapshot.is_ok(), "{snapshot:?}");
    assert_eq!(copy_file_policy(&placeholder, &plain), Ok(()));
    assert_eq!(copy_file_policy(&plain, &placeholder), Ok(()));
    assert_eq!(
        file_policy_snapshot(&surrogate),
        Err(PolicyError::ReparsePoint)
    );
    assert_eq!(
        copy_file_policy(&surrogate, &plain),
        Err(PolicyError::ReparsePoint)
    );
    assert_eq!(
        copy_file_policy(&plain, &surrogate),
        Err(PolicyError::ReparsePoint)
    );
    Ok(())
}

#[test]
fn the_privilege_guard_reverts_this_thread_and_is_absent_without_the_privilege() {
    use windows::Win32::System::Threading::{GetCurrentThread, OpenThreadToken};
    let impersonating = || {
        let mut token = HANDLE::default();
        // SAFETY: this thread's pseudo-handle and a live out pointer; closed below.
        let found = unsafe {
            OpenThreadToken(
                GetCurrentThread(),
                windows::Win32::Security::TOKEN_QUERY,
                true,
                &raw mut token,
            )
        }
        .is_ok();
        if found {
            // SAFETY: the token just opened, closed once.
            let _ = unsafe { windows::Win32::Foundation::CloseHandle(token) };
        }
        found
    };
    assert!(!impersonating());
    {
        let guard = privilege::SecurityPrivilege::enable();
        // Elevated: the thread impersonates its enabled duplicate. Otherwise: no guard.
        assert_eq!(guard.is_some(), impersonating());
    }
    assert!(
        !impersonating(),
        "the thread is back to the process identity"
    );
}
