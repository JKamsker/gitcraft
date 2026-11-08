//! Resource limits for the separate, already lifecycle-owned bundle helper process.
//! Never call this in the GUI process: Linux hard limits deliberately cannot be restored.

pub const BUNDLE_MEMORY_BYTES: u64 = 1 << 30;
pub const BUNDLE_FILE_BYTES: u64 = 512 << 20;

pub struct BundleResources {
    #[cfg(windows)]
    _job: std::os::windows::io::OwnedHandle,
}
impl BundleResources {
    /// Install enforced limits before opening or interpreting any incoming pack.
    pub fn install() -> Result<Self, String> {
        #[cfg(target_os = "linux")]
        {
            install_linux()?;
            Ok(Self {})
        }
        #[cfg(windows)]
        {
            Ok(Self {
                _job: windows_job()?,
            })
        }
        #[cfg(not(any(target_os = "linux", windows)))]
        Err("Bundle unpacking is supported only with Linux hard address-space limits or a Windows memory-limited Job. This platform cannot establish the required memory containment; no pack was opened.".into())
    }
}

#[cfg(any(target_os = "linux", test))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Limit {
    soft: u64,
    hard: u64,
}
#[cfg(any(target_os = "linux", test))]
fn lowered(old: Limit, ceiling: u64) -> Limit {
    let hard = old.hard.min(ceiling);
    Limit {
        soft: old.soft.min(hard),
        hard,
    }
}
#[cfg(any(target_os = "linux", test))]
fn install_limit(
    ceiling: u64,
    mut read: impl FnMut() -> Result<Limit, String>,
    mut write: impl FnMut(Limit) -> Result<(), String>,
) -> Result<(), String> {
    let desired = lowered(read()?, ceiling);
    write(desired)?;
    if read()? != desired {
        return Err("Kernel did not confirm the requested hard resource limit".into());
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn install_linux() -> Result<(), String> {
    for (resource, ceiling) in [
        (libc::RLIMIT_AS, BUNDLE_MEMORY_BYTES),
        (libc::RLIMIT_FSIZE, BUNDLE_FILE_BYTES),
        (libc::RLIMIT_CORE, 0),
        (libc::RLIMIT_CPU, 120),
    ] {
        install_limit(
            ceiling,
            || {
                let mut value = libc::rlimit {
                    rlim_cur: 0,
                    rlim_max: 0,
                };
                // SAFETY: initialized output of the exact getrlimit structure type.
                if unsafe { libc::getrlimit(resource, &mut value) } != 0 {
                    return Err(std::io::Error::last_os_error().to_string());
                }
                Ok(Limit {
                    soft: value.rlim_cur,
                    hard: value.rlim_max,
                })
            },
            |value| {
                let value = libc::rlimit {
                    rlim_cur: value.soft,
                    rlim_max: value.hard,
                };
                // SAFETY: this helper only lowers its own inherited limits, before exec.
                if unsafe { libc::setrlimit(resource, &value) } != 0 {
                    return Err(std::io::Error::last_os_error().to_string());
                }
                Ok(())
            },
        )?;
    }
    Ok(())
}

#[cfg(windows)]
fn windows_job() -> Result<std::os::windows::io::OwnedHandle, String> {
    use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::System::JobObjects::*;
    use windows::Win32::System::Threading::GetCurrentProcess;
    // SAFETY: unnamed Job with non-inherited handle, owned exactly once on success.
    let job = unsafe { CreateJobObjectW(None, None) }.map_err(|e| e.to_string())?;
    let job = unsafe { OwnedHandle::from_raw_handle(job.0) };
    let handle = HANDLE(job.as_raw_handle());
    let flags = JOB_OBJECT_LIMIT_JOB_MEMORY | JOB_OBJECT_LIMIT_ACTIVE_PROCESS;
    let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
    limits.BasicLimitInformation.LimitFlags = flags;
    limits.BasicLimitInformation.ActiveProcessLimit = 8;
    limits.JobMemoryLimit = BUNDLE_MEMORY_BYTES as usize;
    // SAFETY: exact initialized structure, matching class and retained Job HANDLE.
    unsafe {
        SetInformationJobObject(
            handle,
            JobObjectExtendedLimitInformation,
            (&limits as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
            std::mem::size_of_val(&limits) as u32,
        )
    }
    .map_err(|e| e.to_string())?;
    // Attach this actual helper, never a reopened PID. Its future descendants inherit
    // the nested memory Job and remain in the existing outer lifecycle owner's Job.
    // No breakaway, notification-only limits, or new lifecycle/termination authority.
    unsafe { AssignProcessToJobObject(handle, GetCurrentProcess()) }.map_err(|e| e.to_string())?;
    let mut actual = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
    unsafe {
        QueryInformationJobObject(
            Some(handle),
            JobObjectExtendedLimitInformation,
            (&mut actual as *mut JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
            std::mem::size_of_val(&actual) as u32,
            None,
        )
    }
    .map_err(|e| e.to_string())?;
    if actual.BasicLimitInformation.LimitFlags != flags
        || actual.JobMemoryLimit != BUNDLE_MEMORY_BYTES as usize
        || actual.BasicLimitInformation.ActiveProcessLimit != 8
    {
        return Err("Windows did not confirm the enforced bundle Job limits".into());
    }
    Ok(job)
}

#[cfg(test)]
mod recording_tests {
    use super::*;
    #[test]
    fn hard_limits_are_lowered_read_back_and_never_relaxed() {
        for old in [
            Limit {
                soft: u64::MAX,
                hard: u64::MAX,
            },
            Limit {
                soft: 12,
                hard: 100,
            },
            Limit { soft: 0, hard: 0 },
        ] {
            let state = std::cell::Cell::new(old);
            let writes = std::cell::Cell::new(0);
            install_limit(
                64,
                || Ok(state.get()),
                |v| {
                    writes.set(writes.get() + 1);
                    state.set(v);
                    Ok(())
                },
            )
            .unwrap();
            assert_eq!(state.get(), lowered(old, 64));
            assert!(state.get().soft <= old.soft && state.get().hard <= old.hard);
            assert_eq!(writes.get(), 1);
        }
    }
    #[test]
    fn refused_or_unconfirmed_limits_do_not_allow_dispatch() {
        let old = Limit {
            soft: 128,
            hard: 128,
        };
        assert!(install_limit(64, || Ok(old), |_| Err("refused".into())).is_err());
        assert!(install_limit(64, || Ok(old), |_| Ok(())).is_err());
        assert!(
            install_limit(
                64,
                || Err("unavailable".into()),
                |_| panic!("must not write")
            )
            .is_err()
        );
    }
}
