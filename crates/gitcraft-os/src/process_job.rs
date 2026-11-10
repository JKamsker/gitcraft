//! Put a suspended child into a kill-on-close job before it can start descendants.
//! The safe owner returned to the event loop also covers children surviving their parent.

use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
use windows::Win32::Foundation::{HANDLE, WAIT_TIMEOUT};
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, TH32CS_SNAPTHREAD, THREADENTRY32, Thread32First, Thread32Next,
};
use windows::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    JOBOBJECT_BASIC_ACCOUNTING_INFORMATION, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
    JobObjectBasicAccountingInformation, JobObjectExtendedLimitInformation,
    QueryInformationJobObject, SetInformationJobObject, TerminateJobObject,
};
use windows::Win32::System::Threading::{
    GetProcessIdOfThread, OpenThread, ResumeThread, THREAD_QUERY_LIMITED_INFORMATION,
    THREAD_SUSPEND_RESUME, WaitForSingleObject,
};

fn owned(handle: HANDLE) -> OwnedHandle {
    // SAFETY: only freshly created, successful Win32 owned-handle results reach here.
    // OwnedHandle closes each exactly once, including on all subsequent error paths.
    unsafe { OwnedHandle::from_raw_handle(handle.0) }
}
fn raw(handle: &OwnedHandle) -> HANDLE {
    HANDLE(handle.as_raw_handle())
}

/// Owns the job's kill-on-close lifetime, with an explicit failure status when stopped.
pub struct ProcessJob {
    handle: OwnedHandle,
}

impl ProcessJob {
    /// Stop every process in this job with a nonzero exit code. Closing the handle remains
    /// a fallback; kill-on-close alone can report a successful child exit on Windows.
    pub fn terminate(&self) -> Result<(), String> {
        // SAFETY: this owner holds a valid job handle created with termination access.
        unsafe { TerminateJobObject(raw(&self.handle), 1) }.map_err(|e| e.to_string())?;
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        super::suspended::wait_empty(
            || {
                let mut info = JOBOBJECT_BASIC_ACCOUNTING_INFORMATION::default();
                // SAFETY: this exact retained Job HANDLE, initialized matching structure.
                unsafe {
                    QueryInformationJobObject(
                        Some(raw(&self.handle)),
                        JobObjectBasicAccountingInformation,
                        (&mut info as *mut JOBOBJECT_BASIC_ACCOUNTING_INFORMATION).cast(),
                        std::mem::size_of_val(&info) as u32,
                        None,
                    )
                }
                .map_err(|e| std::io::Error::other(e.to_string()))?;
                Ok(info.ActiveProcesses)
            },
            || std::time::Instant::now() >= deadline,
            || std::thread::sleep(std::time::Duration::from_millis(10)),
        )
        .map_err(|e| e.to_string())
    }
}

/// Attach only a directly owned, still-suspended child. Resume is a separate ordered step.
pub(super) fn assign(child: &std::process::Child) -> Result<ProcessJob, String> {
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
    // Assign the retained child HANDLE directly; never reopen a process by PID.
    let process = HANDLE(child.as_raw_handle());
    // SAFETY: the freshly spawned child is still suspended; its retained handle binds the
    // actual object even if startup fails. The Job is held through resume and final drain.
    unsafe { AssignProcessToJobObject(raw(&job), process) }.map_err(|e| e.to_string())?;
    Ok(ProcessJob { handle: job })
}

#[link(name = "ntdll", kind = "raw-dylib")]
unsafe extern "system" {
    /// The thread of `process` after `thread` (the first for a null one), opened with
    /// `access`. Exported by ntdll since Windows Vista.
    fn NtGetNextThread(
        process: HANDLE,
        thread: HANDLE,
        access: u32,
        attributes: u32,
        flags: u32,
        next: *mut HANDLE,
    ) -> i32;
}

pub(super) fn resume(child: &std::process::Child) -> Result<(), String> {
    let pid = child.id();
    // std/Tokio expose the process handle but not its primary thread. GitCraft just spawned
    // this child suspended, before it could create other threads; the assumption is that no
    // external process injects one before this first resume. A successful lookup followed by
    // failed ownership validation is final (never retry a different thread). Lookup through the
    // child process HANDLE avoids a system-wide thread snapshot, which can be expensive.
    let mut first = HANDLE::default();
    // SAFETY: the retained child HANDLE (all access) and a valid out pointer; on success the
    // returned thread handle is owned here and closed once.
    let status = unsafe {
        NtGetNextThread(
            HANDLE(child.as_raw_handle()),
            HANDLE::default(),
            (THREAD_SUSPEND_RESUME | THREAD_QUERY_LIMITED_INFORMATION).0,
            0,
            0,
            &mut first,
        )
    };
    if status >= 0 && !first.is_invalid() {
        return resume_owned(child, pid, &owned(first));
    }
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
                unsafe {
                    OpenThread(
                        THREAD_SUSPEND_RESUME | THREAD_QUERY_LIMITED_INFORMATION,
                        false,
                        entry.th32ThreadID,
                    )
                }
                .map_err(|e| e.to_string())?,
            );
            return resume_owned(child, pid, &thread);
        }
        // SAFETY: same snapshot and structure as Thread32First above.
        if unsafe { Thread32Next(raw(&snapshot), &mut entry) }.is_err() {
            return Err("could not find the suspended child's primary thread".into());
        }
    }
}

/// Resumes `thread` once it is proven the still-running child's own.
fn resume_owned(child: &std::process::Child, pid: u32, thread: &OwnedHandle) -> Result<(), String> {
    // SAFETY: both retained handles remain open. A signaled process HANDLE proves
    // the old lifetime ended even when its numeric exit code happens to be 259.
    let actual = unsafe { GetProcessIdOfThread(raw(thread)) };
    let active = unsafe { WaitForSingleObject(HANDLE(child.as_raw_handle()), 0) } == WAIT_TIMEOUT;
    super::suspended::validate_thread_owner(pid, actual, active).map_err(|e| e.to_string())?;
    // SAFETY: the handle remains valid for this call; failure leaves the job owner
    // responsible for terminating the child rather than returning it suspended.
    if unsafe { ResumeThread(raw(thread)) } == u32::MAX {
        return Err(std::io::Error::last_os_error().to_string());
    }
    Ok(())
}
