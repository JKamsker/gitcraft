//! Native, bounded Windows file-policy snapshot and copy, in-process.
//!
//! The same protocol the PowerShell metadata helper runs (and is checked against): refuse
//! named data streams; read OWNER|GROUP|DACL|SACL|LABEL|ATTRIBUTE|SCOPE|PROCESS_TRUST_LABEL|
//! ACCESS_FILTER, retrying OWNER|GROUP|DACL only for ERROR_PRIVILEGE_NOT_HELD; compare the
//! canonical descriptor bound to its captured scope; copy with explicit protection and
//! verify both target and source afterwards.
use crate::security_descriptor::{self, DACL_PROTECTED, DescriptorError, SACL_PROTECTED};
use std::os::windows::ffi::OsStrExt;
use std::path::Path;
use windows::Win32::Foundation::{ERROR_HANDLE_EOF, HLOCAL, LocalFree, WIN32_ERROR};
use windows::Win32::Security::Authorization::{
    GetNamedSecurityInfoW, SE_FILE_OBJECT, SetNamedSecurityInfoW,
};
use windows::Win32::Security::{
    ACL, GetSecurityDescriptorLength, OBJECT_SECURITY_INFORMATION, PSECURITY_DESCRIPTOR, PSID,
};
use windows::Win32::Storage::FileSystem::{
    FindClose, FindFirstStreamW, FindNextStreamW, FindStreamInfoStandard, WIN32_FIND_STREAM_DATA,
};
use windows::core::PCWSTR;

const ALL_POLICY: u32 = 0x0000_01ff;
const BASIC_POLICY: u32 = 0x0000_0007;
const SACL_INFORMATION: u32 = 0x0000_0008;
const PRIVILEGE_NOT_HELD: u32 = 1314;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PolicyError {
    /// The file has a named data stream, which a bounded save cannot preserve.
    NamedStreams,
    /// A Win32 error reading or applying the policy.
    Native(u32),
    /// A Win32 error listing the file's data streams.
    StreamQuery(u32),
    /// The descriptor is empty or exceeds 1 MiB.
    TooLarge,
    /// The OS returned a descriptor that is not well formed.
    Malformed,
    /// The target's policy differs from the source's after the copy.
    Differs,
    /// The source's policy changed while it was copied.
    SourceChanged,
}

impl From<DescriptorError> for PolicyError {
    fn from(error: DescriptorError) -> Self {
        match error {
            DescriptorError::TooLarge => Self::TooLarge,
            DescriptorError::Malformed => Self::Malformed,
        }
    }
}

/// The policy of the file at `path`, comparable with any other snapshot.
pub fn file_policy_snapshot(path: &Path) -> Result<Vec<u8>, PolicyError> {
    refuse_named_streams(path)?;
    Descriptor::read(&wide(path)?)?.snapshot()
}

/// Apply the policy of `source` to `target`, then verify both.
pub fn copy_file_policy(source: &Path, target: &Path) -> Result<(), PolicyError> {
    refuse_named_streams(source)?;
    let (source, target) = (wide(source)?, wide(target)?);
    let descriptor = Descriptor::read(&source)?;
    let before = descriptor.snapshot()?;
    let (_, control) = security_descriptor::canonical(descriptor.bytes()?)?;
    let mut protection = if control & DACL_PROTECTED != 0 {
        0x8000_0000
    } else {
        0x2000_0000
    };
    // SACL protection flags also require SeSecurityPrivilege: set them only when the SACL
    // itself was captured, and never pass an unrequested SACL.
    let sacl_captured = descriptor.info & SACL_INFORMATION != 0;
    if sacl_captured {
        protection |= if control & SACL_PROTECTED != 0 {
            0x4000_0000
        } else {
            0x1000_0000
        };
    }
    descriptor.apply(&target, descriptor.info | protection, sacl_captured)?;
    if Descriptor::read(&target)?.snapshot()? != before {
        return Err(PolicyError::Differs);
    }
    if Descriptor::read(&source)?.snapshot()? != before {
        return Err(PolicyError::SourceChanged);
    }
    Ok(())
}

fn wide(path: &Path) -> Result<Vec<u16>, PolicyError> {
    let mut wide: Vec<u16> = path.as_os_str().encode_wide().collect();
    if wide.contains(&0) {
        return Err(PolicyError::Native(123)); // ERROR_INVALID_NAME
    }
    wide.push(0);
    Ok(wide)
}

fn refuse_named_streams(path: &Path) -> Result<(), PolicyError> {
    let path = wide(path)?;
    let mut data = WIN32_FIND_STREAM_DATA::default();
    // SAFETY: a NUL-terminated path and an initialized, correctly typed output structure.
    let found = unsafe {
        FindFirstStreamW(
            PCWSTR(path.as_ptr()),
            FindStreamInfoStandard,
            (&raw mut data).cast(),
            None,
        )
    };
    let handle = match found {
        Ok(handle) => handle,
        Err(e) if WIN32_ERROR::from_error(&e) == Some(ERROR_HANDLE_EOF) => return Ok(()),
        Err(e) => return Err(PolicyError::StreamQuery(native_code(&e))),
    };
    let result = (|| loop {
        let name = String::from_utf16_lossy(&data.cStreamName);
        let name = name.trim_end_matches('\0');
        if !matches!(name, ":$DATA" | "$DATA" | "::$DATA") {
            return Err(PolicyError::NamedStreams);
        }
        // SAFETY: the live find handle and the same initialized output structure.
        match unsafe { FindNextStreamW(handle, (&raw mut data).cast()) } {
            Ok(()) => {}
            Err(e) if WIN32_ERROR::from_error(&e) == Some(ERROR_HANDLE_EOF) => return Ok(()),
            Err(e) => return Err(PolicyError::StreamQuery(native_code(&e))),
        }
    })();
    // SAFETY: the find handle is owned here and closed exactly once. A failed close
    // leaves nothing to undo and does not change what was found.
    let _ = unsafe { FindClose(handle) };
    result
}

fn native_code(error: &windows::core::Error) -> u32 {
    WIN32_ERROR::from_error(error).map_or(error.code().0 as u32, |e| e.0)
}

/// A descriptor returned by `GetNamedSecurityInfoW`, freed on drop, with its parts.
struct Descriptor {
    info: u32,
    descriptor: PSECURITY_DESCRIPTOR,
    owner: PSID,
    group: PSID,
    dacl: *mut ACL,
    sacl: *mut ACL,
}

impl Descriptor {
    fn read(path: &[u16]) -> Result<Self, PolicyError> {
        match Self::read_scope(path, ALL_POLICY) {
            // Normal users cannot request the audit SACL; only this error permits the
            // owner/group/DACL-only retry, never an unreadable DACL.
            Err(PolicyError::Native(PRIVILEGE_NOT_HELD)) => Self::read_scope(path, BASIC_POLICY),
            result => result,
        }
    }
    fn read_scope(path: &[u16], info: u32) -> Result<Self, PolicyError> {
        let mut read = Self {
            info,
            descriptor: PSECURITY_DESCRIPTOR::default(),
            owner: PSID::default(),
            group: PSID::default(),
            dacl: std::ptr::null_mut(),
            sacl: std::ptr::null_mut(),
        };
        // SAFETY: a NUL-terminated path; every out pointer refers to a field of `read`,
        // whose drop frees the returned descriptor (the parts point into it).
        let error = unsafe {
            GetNamedSecurityInfoW(
                PCWSTR(path.as_ptr()),
                SE_FILE_OBJECT,
                OBJECT_SECURITY_INFORMATION(info),
                Some(&raw mut read.owner),
                Some(&raw mut read.group),
                Some(&raw mut read.dacl),
                Some(&raw mut read.sacl),
                &raw mut read.descriptor,
            )
        };
        if error.0 != 0 {
            return Err(PolicyError::Native(error.0));
        }
        Ok(read)
    }
    fn bytes(&self) -> Result<&[u8], PolicyError> {
        if self.descriptor.0.is_null() {
            return Err(PolicyError::Malformed);
        }
        // SAFETY: a valid self-relative descriptor returned by GetNamedSecurityInfoW.
        let len = unsafe { GetSecurityDescriptorLength(self.descriptor) } as usize;
        if len == 0 || len > security_descriptor::DESCRIPTOR_LIMIT {
            return Err(PolicyError::TooLarge);
        }
        // SAFETY: the descriptor is `len` contiguous bytes, alive as long as `self`.
        Ok(unsafe { std::slice::from_raw_parts(self.descriptor.0.cast::<u8>(), len) })
    }
    fn snapshot(&self) -> Result<Vec<u8>, PolicyError> {
        Ok(security_descriptor::snapshot_bytes(
            self.info,
            self.bytes()?,
        )?)
    }
    fn apply(&self, target: &[u16], info: u32, with_sacl: bool) -> Result<(), PolicyError> {
        // SAFETY: a NUL-terminated path; the parts point into this live descriptor.
        let error = unsafe {
            SetNamedSecurityInfoW(
                PCWSTR(target.as_ptr()),
                SE_FILE_OBJECT,
                OBJECT_SECURITY_INFORMATION(info),
                Some(self.owner),
                Some(self.group),
                Some(self.dacl.cast_const()),
                with_sacl.then_some(self.sacl.cast_const()),
            )
        };
        if error.0 != 0 {
            return Err(PolicyError::Native(error.0));
        }
        Ok(())
    }
}

impl Drop for Descriptor {
    fn drop(&mut self) {
        if !self.descriptor.0.is_null() {
            // SAFETY: allocated by GetNamedSecurityInfoW and freed exactly once; a failed
            // free cannot be retried from a destructor.
            let _ = unsafe { LocalFree(Some(HLOCAL(self.descriptor.0))) };
        }
    }
}
