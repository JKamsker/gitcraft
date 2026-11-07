//! Put a suspended child into a kill-on-close job before it can start descendants.
//! The safe owner returned to the event loop also covers children surviving their parent.

use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
use windows::Win32::Foundation::HANDLE;
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, TH32CS_SNAPTHREAD, THREADENTRY32, Thread32First, Thread32Next,
};
use windows::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
    SetInformationJobObject,
};
use windows::Win32::System::Threading::{
    OpenProcess, OpenThread, PROCESS_SET_QUOTA, PROCESS_TERMINATE, ResumeThread,
    THREAD_SUSPEND_RESUME,
};

fn owned(handle: HANDLE) -> OwnedHandle {
    // SAFETY: only freshly created, successful Win32 owned-handle results reach here.
    // OwnedHandle closes each exactly once, including on all subsequent error paths.
    unsafe { OwnedHandle::from_raw_handle(handle.0) }
}
fn raw(handle: &OwnedHandle) -> HANDLE {
    HANDLE(handle.as_raw_handle())
}

/// `pid` must name a child created with CREATE_SUSPENDED. On success the primary thread
/// runs, and dropping the returned job stops that child and all of its descendants. On
/// failure the caller must kill and reap its still-suspended child.
pub fn supervise_suspended_process(pid: u32) -> Result<OwnedHandle, String> {
    // SAFETY: no inherited security attributes and no name; all returned handles are owned.
    let job = owned(unsafe { CreateJobObjectW(None, None) }.map_err(|e| e.to_string())?);
    let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
    limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
    // SAFETY: a valid job, matching information class, initialized structure and exact size.
    unsafe {
        SetInformationJobObject(
            raw(&job),
            JobObjectExtendedLimitInformation,
            (&limits as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
            std::mem::size_of_val(&limits) as u32,
        )
    }
    .map_err(|e| e.to_string())?;
    // SAFETY: access is restricted to the rights AssignProcessToJobObject needs; an invalid
    // or inaccessible pid yields an error. The handle is not inherited by the child.
    let process = owned(
        unsafe { OpenProcess(PROCESS_SET_QUOTA | PROCESS_TERMINATE, false, pid) }
            .map_err(|e| e.to_string())?,
    );
    // SAFETY: valid owned job/process handles; the suspended child cannot race assignment
    // by spawning a descendant before it belongs to the job.
    unsafe { AssignProcessToJobObject(raw(&job), raw(&process)) }.map_err(|e| e.to_string())?;
    // std/Tokio expose the process handle but not its primary thread. Enumerate threads
    // while the new process is suspended; only its primary thread can exist at this point.
    // SAFETY: a thread snapshot has no pointer parameters; a failure returns no handle.
    let snapshot = owned(
        unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0) }.map_err(|e| e.to_string())?,
    );
    let mut entry = THREADENTRY32 {
        dwSize: std::mem::size_of::<THREADENTRY32>() as u32,
        ..Default::default()
    };
    // SAFETY: valid snapshot and initialized, correctly sized output structure.
    unsafe { Thread32First(raw(&snapshot), &mut entry) }.map_err(|e| e.to_string())?;
    loop {
        if entry.th32OwnerProcessID == pid {
            // SAFETY: a thread ID from the snapshot, with only resume access requested.
            let thread = owned(
                unsafe { OpenThread(THREAD_SUSPEND_RESUME, false, entry.th32ThreadID) }
                    .map_err(|e| e.to_string())?,
            );
            // SAFETY: the handle remains valid for this call; failure leaves the job owner
            // responsible for terminating the child rather than returning it suspended.
            if unsafe { ResumeThread(raw(&thread)) } == u32::MAX {
                return Err(std::io::Error::last_os_error().to_string());
            }
            return Ok(job);
        }
        // SAFETY: same snapshot and structure as Thread32First above.
        if unsafe { Thread32Next(raw(&snapshot), &mut entry) }.is_err() {
            return Err("could not find the suspended child's primary thread".into());
        }
    }
}
