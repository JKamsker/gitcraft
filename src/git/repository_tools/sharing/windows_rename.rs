//! The one owned metadata mutation that cannot be expressed by safe std APIs.
//! Never re-open the source by name; never replace an existing destination.
use std::fs::File;
use std::io;
use std::os::windows::{ffi::OsStrExt, io::AsRawHandle};

/// Rename the actual `source` file to a path derived from the retained `directory`.
///
/// Microsoft documents FILE_RENAME_INFO.ReplaceIfExists=false as an error when
/// the target exists. Resolve the absolute target through the held directory handle:
/// https://learn.microsoft.com/windows/win32/api/winbase/ns-winbase-file_rename_info
/// The source must have DELETE access. Its no-DELETE-sharing handle and the
/// destination's no-delete-sharing ancestor handles remain held throughout; this
/// changes a directory entry, never file contents.
#[allow(unsafe_code)]
pub(super) fn no_replace(source: &File, directory: &File, name: &str) -> io::Result<()> {
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_RENAME_INFO, FileRenameInfo, SetFileInformationByHandle,
    };
    if name.is_empty()
        || name.len() > 255
        || matches!(name, "." | "..")
        || !name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'_'))
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            crate::i18n::text("backend-invalid-owned-metadata-name-4c8f18"),
        ));
    }
    let mut target = final_path(directory).ok_or_else(|| {
        io::Error::other(crate::i18n::text(
            "backend-the-system-could-not-report-the-held-directory-location-8699e1",
        ))
    })?;
    target.push(name);
    let wide = target.as_os_str().encode_wide().collect::<Vec<_>>();
    let offset = std::mem::offset_of!(FILE_RENAME_INFO, FileName);
    let name_bytes = wide
        .len()
        .checked_mul(std::mem::size_of::<u16>())
        .ok_or_else(|| {
            io::Error::other(crate::i18n::text(
                "backend-rename-metadata-too-large-40ee73",
            ))
        })?;
    let bytes = offset
        .checked_add(name_bytes)
        .ok_or_else(|| {
            io::Error::other(crate::i18n::text(
                "backend-rename-metadata-too-large-40ee73",
            ))
        })?
        .max(std::mem::size_of::<FILE_RENAME_INFO>());
    let mut storage =
        vec![FILE_RENAME_INFO::default(); bytes.div_ceil(std::mem::size_of::<FILE_RENAME_INFO>())];
    let size = u32::try_from(bytes).map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            crate::i18n::text("backend-rename-metadata-too-large-40ee73"),
        )
    })?;
    // SAFETY: defaulted FILE_RENAME_INFO storage has the required native alignment
    // and at least its fixed header plus the full UTF-16 name/NUL.
    // The structure's fields are scalar/union fields with valid zero representations.
    // Both handles are borrowed from live Files for this call only. The name is one
    // validated relative component; copying writes exactly wide.len() u16s into the
    // allocated trailing buffer. Windows receives the actual buffer byte length,
    // reads it synchronously, and keeps neither buffer nor borrowed handles.
    unsafe {
        let info = storage.as_mut_ptr();
        (*info).Anonymous.ReplaceIfExists = false;
        (*info).RootDirectory = std::ptr::null_mut();
        (*info).FileNameLength = u32::try_from(name_bytes).map_err(|_| {
            io::Error::other(crate::i18n::text(
                "backend-rename-metadata-too-large-40ee73",
            ))
        })?;
        std::ptr::copy_nonoverlapping(
            wide.as_ptr(),
            std::ptr::addr_of_mut!((*info).FileName).cast::<u16>(),
            wide.len(),
        );
        if SetFileInformationByHandle(source.as_raw_handle(), FileRenameInfo, info.cast(), size)
            == 0
        {
            Err(io::Error::last_os_error())
        } else {
            Ok(())
        }
    }
}

#[allow(unsafe_code)]
pub(super) fn final_path(file: &std::fs::File) -> Option<std::path::PathBuf> {
    use std::os::windows::ffi::OsStringExt;
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_NAME_NORMALIZED, GetFinalPathNameByHandleW, VOLUME_NAME_DOS,
    };
    let handle = file.as_raw_handle();
    let mut buf: Vec<u16> = vec![0; 512];
    // Twice at most: a first answer that the buffer is too small says how big it must be.
    for _ in 0..3 {
        let capacity = u32::try_from(buf.len()).ok()?;
        // SAFETY: `handle` is the open handle of `file`, which outlives this call; `buf` is a
        // live, writable buffer of exactly `capacity` u16s, and the call writes at most that
        // many. Nothing else is read or kept.
        let len = unsafe {
            GetFinalPathNameByHandleW(
                handle,
                buf.as_mut_ptr(),
                capacity,
                FILE_NAME_NORMALIZED | VOLUME_NAME_DOS,
            )
        };
        let len = usize::try_from(len).ok()?;
        match len {
            // Failed.
            0 => return None,
            // The path, without its terminating NUL.
            n if n < buf.len() => {
                buf.truncate(n);
                return Some(std::ffi::OsString::from_wide(&buf).into());
            }
            // Too small: `n` is the size needed, its NUL included.
            n => buf.resize(n, 0),
        }
    }
    None
}

/// Delete the actual owned temporary inode, never a freshly resolved pathname.
#[allow(unsafe_code)]
pub(super) fn discard(source: &File) -> io::Result<()> {
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_DISPOSITION_INFO, FileDispositionInfo, SetFileInformationByHandle,
    };
    let mut info = FILE_DISPOSITION_INFO { DeleteFile: true };
    // SAFETY: source remains live and was opened with DELETE access and no delete
    // sharing. Windows reads this initialized fixed-size structure synchronously.
    unsafe {
        if SetFileInformationByHandle(
            source.as_raw_handle(),
            FileDispositionInfo,
            (&mut info as *mut FILE_DISPOSITION_INFO).cast(),
            std::mem::size_of::<FILE_DISPOSITION_INFO>() as u32,
        ) == 0
        {
            Err(io::Error::last_os_error())
        } else {
            Ok(())
        }
    }
}
