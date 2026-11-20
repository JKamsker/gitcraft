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
    // Elevated runners hold SeSecurityPrivilege: record what the SACL scope does there, step
    // by step, without depending on it (a failure narrows to the label scope).
    eprintln!(
        "[policy-probe] full-scope copy: {:?}",
        copy_in_scope(&placeholder, &plain, ALL_POLICY)
    );
    if let Ok(target) = open(&plain, read_access(ALL_POLICY) | WRITE_DAC | WRITE_OWNER) {
        let source = open(&placeholder, read_access(ALL_POLICY));
        let descriptor = source.and_then(|s| Descriptor::read(&s, ALL_POLICY));
        if let Ok(d) = descriptor {
            for info in [APPLIED, ALL_POLICY, 0x7, 0x8, 0x10] {
                eprintln!(
                    "[policy-probe] apply {info:#x}: {:?}",
                    d.apply(
                        &target,
                        info | if info & 0x4 != 0 { 0x2000_0000 } else { 0 }
                    )
                );
            }
        }
    } else {
        eprintln!("[policy-probe] no full-scope target handle");
    }
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

#[test]
fn any_failure_with_the_sacl_in_scope_narrows_like_a_missing_privilege() {
    // Elevated: the SACL-scope attempt fails with access denied; the label scope is used.
    let mut tried = Vec::new();
    let result = widest(|scope| {
        tried.push(scope);
        if scope & SACL_INFORMATION != 0 {
            Err(PolicyError::Native(5))
        } else {
            Ok(scope)
        }
    });
    assert_eq!(result, Ok(LABEL_POLICY));
    assert_eq!(tried, [ALL_POLICY, LABEL_POLICY]);
    // Verification and descriptor failures in the SACL scope narrow too.
    for error in [
        PolicyError::Differs,
        PolicyError::Malformed,
        PolicyError::StreamQuery(1),
    ] {
        let result = widest(|scope| {
            if scope == ALL_POLICY {
                Err(error)
            } else {
                Ok(scope)
            }
        });
        assert_eq!(result, Ok(LABEL_POLICY), "{error:?}");
    }
}

#[test]
fn below_the_sacl_only_a_missing_privilege_narrows_and_refusals_never_do() {
    let denied = widest(|scope| {
        if scope == LABEL_POLICY {
            Err(PolicyError::Native(5))
        } else if scope == ALL_POLICY {
            Err(PolicyError::Native(PRIVILEGE_NOT_HELD))
        } else {
            Ok(scope)
        }
    });
    assert_eq!(
        denied,
        Err(PolicyError::Native(5)),
        "a real error is reported"
    );
    let unprivileged = widest(|scope| match scope {
        BASIC_POLICY => Ok(scope),
        _ => Err(PolicyError::Native(PRIVILEGE_NOT_HELD)),
    });
    assert_eq!(unprivileged, Ok(BASIC_POLICY));
    for refusal in [PolicyError::ReparsePoint, PolicyError::NamedStreams] {
        let mut tried = 0;
        let result: Result<u32, _> = widest(|_| {
            tried += 1;
            Err(refusal)
        });
        assert_eq!((result, tried), (Err(refusal), 1));
    }
    let nothing: Result<u32, _> = widest(|_| Err(PolicyError::Native(PRIVILEGE_NOT_HELD)));
    assert_eq!(nothing, Err(PolicyError::Native(PRIVILEGE_NOT_HELD)));
}
