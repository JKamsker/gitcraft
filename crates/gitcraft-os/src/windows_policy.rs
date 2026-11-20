//! Native, bounded Windows file-policy snapshot and copy, in-process and handle-based.
//!
//! The protocol of the PowerShell metadata helper (and checked against it), with two
//! deliberate improvements: every call goes through one handle opened without following
//! the final path component (a symbolic link, junction or other name-surrogate reparse
//! point is refused; a Cloud Files placeholder or Dedup file is the file itself), and an
//! unprivileged read keeps the mandatory integrity label instead of dropping it.
//!
//! Scope: OWNER|GROUP|DACL|SACL|LABEL|ATTRIBUTE|SCOPE|PROCESS_TRUST_LABEL|ACCESS_FILTER,
//! with SeSecurityPrivilege enabled on this thread for the call when the token holds it (as
//! GetNamedSecurityInfo did). Only ERROR_PRIVILEGE_NOT_HELD narrows it, first to
//! OWNER|GROUP|DACL|LABEL, then to OWNER|GROUP|DACL. A snapshot is the canonical descriptor
//! bound to the scope captured.
//!
//! These are synchronous file-system calls without a process deadline. Callers run them on
//! background workers (staging mutations, standalone tool jobs), never on a UI thread; a
//! hung network share blocks only that worker, for as long as the OS I/O timeouts allow.
use crate::security_descriptor::{self, DACL_PROTECTED, DescriptorError, SACL_PROTECTED};
use std::fs::File;
use std::os::windows::fs::OpenOptionsExt;
use std::os::windows::io::AsRawHandle;
use std::path::Path;
use windows::Win32::Foundation::{HANDLE, HLOCAL, LocalFree};
use windows::Win32::Security::Authorization::{GetSecurityInfo, SE_FILE_OBJECT, SetSecurityInfo};
use windows::Win32::Security::{
    ACL, GetSecurityDescriptorLength, OBJECT_SECURITY_INFORMATION, PSECURITY_DESCRIPTOR, PSID,
};

mod privilege;
mod streams;

const ALL_POLICY: u32 = 0x0000_01ff;
const LABEL_POLICY: u32 = 0x0000_0017;
const BASIC_POLICY: u32 = 0x0000_0007;
const SACL_INFORMATION: u32 = 0x0000_0008;
const LABEL_INFORMATION: u32 = 0x0000_0010;
/// Narrowed only when the wider scope needs a privilege the caller does not hold.
const SCOPES: [u32; 3] = [ALL_POLICY, LABEL_POLICY, BASIC_POLICY];

const READ_CONTROL: u32 = 0x0002_0000;
const WRITE_DAC: u32 = 0x0004_0000;
const WRITE_OWNER: u32 = 0x0008_0000;
const ACCESS_SYSTEM_SECURITY: u32 = 0x0100_0000;
const FILE_READ_ATTRIBUTES: u32 = 0x0000_0080;
const SHARE_ALL: u32 = 0x1 | 0x2 | 0x4;
/// `FILE_FLAG_BACKUP_SEMANTICS` (folders too) | `FILE_FLAG_OPEN_REPARSE_POINT` (no follow).
const NO_FOLLOW: u32 = 0x0200_0000 | 0x0020_0000;

/// `ERROR_PRIVILEGE_NOT_HELD`.
const PRIVILEGE_NOT_HELD: u32 = 1314;
/// `ERROR_INVALID_NAME`, for a path with an embedded NUL.
const INVALID_NAME: u32 = 123;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PolicyError {
    /// The file has a named data stream, which a bounded save cannot preserve.
    NamedStreams,
    /// The path names a link, junction or other reparse point, which is never followed.
    ReparsePoint,
    /// A Win32 error opening the file or reading or applying its policy.
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

fn descriptor_error(error: DescriptorError) -> PolicyError {
    match error {
        DescriptorError::TooLarge => PolicyError::TooLarge,
        DescriptorError::Malformed => PolicyError::Malformed,
    }
}

/// The policy of the file at `path`, comparable with any other snapshot.
pub fn file_policy_snapshot(path: &Path) -> Result<Vec<u8>, PolicyError> {
    let _privilege = privilege::SecurityPrivilege::enable();
    let (file, descriptor) = read_widest(path)?;
    streams::refuse_named(&file)?;
    descriptor.snapshot()
}

/// The policy in exactly `scope`, never narrowed: for comparison with a reference.
pub fn file_policy_snapshot_in_scope(path: &Path, scope: u32) -> Result<Vec<u8>, PolicyError> {
    let _privilege = (scope & SACL_INFORMATION != 0)
        .then(privilege::SecurityPrivilege::enable)
        .flatten();
    let file = open(path, read_access(scope))?;
    streams::refuse_named(&file)?;
    Descriptor::read(&file, scope)?.snapshot()
}

/// Apply the policy of `source` to `target`, then verify both through their handles.
pub fn copy_file_policy(source: &Path, target: &Path) -> Result<(), PolicyError> {
    let _privilege = privilege::SecurityPrivilege::enable();
    let (source, descriptor) = read_widest(source)?;
    streams::refuse_named(&source)?;
    let before = descriptor.snapshot()?;
    let scope = descriptor.info;
    let (_, control) =
        security_descriptor::canonical(descriptor.bytes()?).map_err(descriptor_error)?;
    let mut protection = if control & DACL_PROTECTED != 0 {
        0x8000_0000
    } else {
        0x2000_0000
    };
    // SACL protection flags require SeSecurityPrivilege: set them only with the SACL itself.
    // A label alone is applied without them.
    if scope & SACL_INFORMATION != 0 {
        protection |= if control & SACL_PROTECTED != 0 {
            0x4000_0000
        } else {
            0x1000_0000
        };
    }
    // Applying LABEL with no label (a null SACL) would mark the target's SACL present and
    // protected. Without a source label, leave the target's untouched: the verification
    // below still refuses a target that carries a label of its own.
    let applied = if scope & SACL_INFORMATION == 0 && descriptor.sacl.is_null() {
        scope & !LABEL_INFORMATION
    } else {
        scope
    };
    let target = open(target, read_access(scope) | WRITE_DAC | WRITE_OWNER)?;
    descriptor.apply(&target, applied | protection)?;
    if Descriptor::read(&target, scope)?.snapshot()? != before {
        return Err(PolicyError::Differs);
    }
    if Descriptor::read(&source, scope)?.snapshot()? != before {
        return Err(PolicyError::SourceChanged);
    }
    Ok(())
}

fn read_access(scope: u32) -> u32 {
    let system = if scope & SACL_INFORMATION != 0 {
        ACCESS_SYSTEM_SECURITY
    } else {
        0
    };
    READ_CONTROL | FILE_READ_ATTRIBUTES | system
}

/// The widest scope this caller may read, with the handle it was read through.
fn read_widest(path: &Path) -> Result<(File, Descriptor), PolicyError> {
    let mut last = PolicyError::Native(PRIVILEGE_NOT_HELD);
    for scope in SCOPES {
        let read = open(path, read_access(scope))
            .and_then(|file| Descriptor::read(&file, scope).map(|d| (file, d)));
        match read {
            Err(e @ PolicyError::Native(PRIVILEGE_NOT_HELD)) => last = e,
            result => return result,
        }
    }
    Err(last)
}

/// Open `path` itself, never what a link names; a reparse point is refused.
fn open(path: &Path, access: u32) -> Result<File, PolicyError> {
    use std::os::windows::ffi::OsStrExt;
    if path.as_os_str().encode_wide().any(|unit| unit == 0) {
        return Err(PolicyError::Native(INVALID_NAME));
    }
    let file = std::fs::OpenOptions::new()
        .access_mode(access)
        .share_mode(SHARE_ALL)
        .custom_flags(NO_FOLLOW)
        .open(path)
        .map_err(native)?;
    // Only a name surrogate (link, junction) stands for another object; a Cloud Files
    // placeholder or Dedup file is the file itself.
    let (attributes, tag) = streams::attribute_tag(&file)?;
    if crate::reparse::is_link(attributes, tag) {
        return Err(PolicyError::ReparsePoint);
    }
    Ok(file)
}

fn native(error: std::io::Error) -> PolicyError {
    PolicyError::Native(error.raw_os_error().map_or(0, |c| c as u32))
}

/// A descriptor returned by `GetSecurityInfo`, freed on drop, with its parts.
struct Descriptor {
    info: u32,
    descriptor: PSECURITY_DESCRIPTOR,
    owner: PSID,
    group: PSID,
    dacl: *mut ACL,
    sacl: *mut ACL,
}

impl Descriptor {
    fn read(file: &File, info: u32) -> Result<Self, PolicyError> {
        let mut read = Self {
            info,
            descriptor: PSECURITY_DESCRIPTOR::default(),
            owner: PSID::default(),
            group: PSID::default(),
            dacl: std::ptr::null_mut(),
            sacl: std::ptr::null_mut(),
        };
        // SAFETY: the live handle of `file`; every out pointer refers to a field of `read`,
        // whose drop frees the returned descriptor (the parts point into it).
        let error = unsafe {
            GetSecurityInfo(
                HANDLE(file.as_raw_handle()),
                SE_FILE_OBJECT,
                OBJECT_SECURITY_INFORMATION(info),
                Some(&raw mut read.owner),
                Some(&raw mut read.group),
                Some(&raw mut read.dacl),
                Some(&raw mut read.sacl),
                Some(&raw mut read.descriptor),
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
        // SAFETY: a valid self-relative descriptor returned by GetSecurityInfo.
        let len = unsafe { GetSecurityDescriptorLength(self.descriptor) } as usize;
        if len == 0 || len > security_descriptor::DESCRIPTOR_LIMIT {
            return Err(PolicyError::TooLarge);
        }
        // SAFETY: the descriptor is `len` contiguous bytes, alive as long as `self`.
        Ok(unsafe { std::slice::from_raw_parts(self.descriptor.0.cast::<u8>(), len) })
    }
    fn snapshot(&self) -> Result<Vec<u8>, PolicyError> {
        security_descriptor::snapshot_bytes(self.info, self.bytes()?).map_err(descriptor_error)
    }
    /// The SACL argument carries the label too, so it is passed for either; never otherwise.
    fn apply(&self, target: &File, info: u32) -> Result<(), PolicyError> {
        let sacl =
            (info & (SACL_INFORMATION | LABEL_INFORMATION) != 0).then_some(self.sacl.cast_const());
        // SAFETY: the live handle of `target`; the parts point into this live descriptor.
        let error = unsafe {
            SetSecurityInfo(
                HANDLE(target.as_raw_handle()),
                SE_FILE_OBJECT,
                OBJECT_SECURITY_INFORMATION(info),
                Some(self.owner),
                Some(self.group),
                Some(self.dacl.cast_const()),
                sacl,
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
            // SAFETY: allocated by GetSecurityInfo and freed exactly once; a failed free
            // cannot be retried from a destructor.
            let _ = unsafe { LocalFree(Some(HLOCAL(self.descriptor.0))) };
        }
    }
}

#[cfg(test)]
mod tests;
