//! Pin regular metadata files once; never block opening FIFOs or follow symlink swaps.
use std::{
    fs::File,
    io::{Read, Write},
    path::Path,
};
const LIMIT: u64 = 64 * 1024 * 1024;
fn budget() -> String {
    crate::i18n::text("staged-budget").into()
}
fn regular() -> String {
    crate::i18n::text("staged-metadata").into()
}
pub(super) fn read(path: &Path) -> Result<Option<Vec<u8>>, String> {
    crate::processes::deadline::check()?;
    #[cfg(unix)]
    let opened = rustix::fs::open(
        path,
        rustix::fs::OFlags::RDONLY
            | rustix::fs::OFlags::NONBLOCK
            | rustix::fs::OFlags::NOFOLLOW
            | rustix::fs::OFlags::CLOEXEC,
        rustix::fs::Mode::empty(),
    )
    .map(File::from)
    .map_err(std::io::Error::from);
    #[cfg(windows)]
    let opened = {
        use std::os::windows::fs::OpenOptionsExt;
        // Reparse points are opened themselves, then rejected through handle metadata.
        File::options()
            .read(true)
            .custom_flags(windows_sys::Win32::Storage::FileSystem::FILE_FLAG_OPEN_REPARSE_POINT)
            .open(path)
    };
    #[cfg(not(any(unix, windows)))]
    let opened = File::open(path);
    let file = match opened {
        Ok(file) => file,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(regular()),
    };
    let metadata = file.metadata().map_err(|_| regular())?;
    if !metadata.is_file() || metadata.is_symlink() {
        return Err(regular());
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        use windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT;
        if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return Err(regular());
        }
    }
    if metadata.len() > LIMIT {
        return Err(budget());
    }
    chunks(file, LIMIT).map(Some)
}
fn chunks(reader: impl Read, limit: u64) -> Result<Vec<u8>, String> {
    let mut reader = reader.take(limit + 1);
    let mut bytes = Vec::new();
    let mut chunk = [0; 8192];
    loop {
        crate::processes::deadline::check()?;
        let count = reader.read(&mut chunk).map_err(|e| e.to_string())?;
        if count == 0 {
            return Ok(bytes);
        }
        if bytes.len() as u64 + count as u64 > limit {
            return Err(budget());
        }
        bytes.extend_from_slice(&chunk[..count]);
    }
}
pub(super) fn write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let mut file = File::options()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|e| e.to_string())?;
    for chunk in bytes.chunks(8192) {
        crate::processes::deadline::check()?;
        file.write_all(chunk).map_err(|e| e.to_string())?;
    }
    Ok(())
}
#[cfg(test)]
mod tests;
