//! What the already opened (never followed) handle is: its reparse tag and data streams.
use super::PolicyError;
use std::fs::File;
use std::os::windows::io::AsRawHandle;
use windows::Win32::Foundation::{ERROR_HANDLE_EOF, ERROR_MORE_DATA, HANDLE, WIN32_ERROR};
use windows::Win32::Storage::FileSystem::{
    FILE_ATTRIBUTE_TAG_INFO, FileAttributeTagInfo, FileStreamInfo, GetFileInformationByHandleEx,
};

/// The object's attributes and reparse tag (`FileAttributeTagInfo`).
pub(super) fn attribute_tag(file: &File) -> Result<(u32, u32), PolicyError> {
    let mut info = FILE_ATTRIBUTE_TAG_INFO::default();
    // SAFETY: the live handle of `file` and an initialized output structure of exactly the
    // size passed.
    unsafe {
        GetFileInformationByHandleEx(
            HANDLE(file.as_raw_handle()),
            FileAttributeTagInfo,
            (&raw mut info).cast(),
            std::mem::size_of::<FILE_ATTRIBUTE_TAG_INFO>() as u32,
        )
    }
    .map_err(|e| {
        PolicyError::Native(WIN32_ERROR::from_error(&e).map_or(e.code().0 as u32, |c| c.0))
    })?;
    Ok((info.FileAttributes, info.ReparseTag))
}

/// More stream records than fit here means named streams exist; no need to list them all.
const LIMIT: usize = 64 * 1024;

pub(super) fn refuse_named(file: &File) -> Result<(), PolicyError> {
    let mut words = vec![0u64; 512];
    loop {
        let bytes = words.len() * 8;
        // SAFETY: the live handle of `file` and a writable, 8-byte aligned buffer of exactly
        // `bytes` bytes, which the call fills with at most that many.
        let result = unsafe {
            GetFileInformationByHandleEx(
                HANDLE(file.as_raw_handle()),
                FileStreamInfo,
                words.as_mut_ptr().cast(),
                bytes as u32,
            )
        };
        match result {
            Ok(()) => {
                let records: Vec<u8> = words.iter().flat_map(|w| w.to_le_bytes()).collect();
                return match crate::stream_info::only_unnamed_data(&records) {
                    Some(true) => Ok(()),
                    Some(false) => Err(PolicyError::NamedStreams),
                    None => Err(PolicyError::Malformed),
                };
            }
            Err(e) => match WIN32_ERROR::from_error(&e) {
                // A folder or empty object can have no streams at all.
                Some(ERROR_HANDLE_EOF) => return Ok(()),
                Some(ERROR_MORE_DATA) if bytes < LIMIT => words.resize(words.len() * 2, 0),
                Some(ERROR_MORE_DATA) => return Err(PolicyError::NamedStreams),
                code => {
                    return Err(PolicyError::StreamQuery(
                        code.map_or(e.code().0 as u32, |c| c.0),
                    ));
                }
            },
        }
    }
}
