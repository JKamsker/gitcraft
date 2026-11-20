//! SeSecurityPrivilege, enabled only for one policy call and only on this thread.
//!
//! Opening a handle with ACCESS_SYSTEM_SECURITY needs the privilege *enabled*; an elevated
//! token holds it disabled. Rather than change the process token, this thread impersonates
//! an enabled duplicate of it for the guard's lifetime and then reverts to self. A token that
//! does not hold the privilege (ERROR_NOT_ALL_ASSIGNED) yields no guard, and the caller's
//! scope narrows as before. A thread that already impersonates is left untouched.
use std::marker::PhantomData;
use windows::Win32::Foundation::{CloseHandle, ERROR_NOT_ALL_ASSIGNED, GetLastError, HANDLE, LUID};
use windows::Win32::Security::{
    AdjustTokenPrivileges, DuplicateTokenEx, LUID_AND_ATTRIBUTES, LookupPrivilegeValueW,
    SE_PRIVILEGE_ENABLED, SecurityImpersonation, TOKEN_ADJUST_PRIVILEGES, TOKEN_DUPLICATE,
    TOKEN_IMPERSONATE, TOKEN_PRIVILEGES, TOKEN_QUERY, TokenImpersonation,
};
use windows::Win32::System::Threading::{
    GetCurrentProcess, GetCurrentThread, OpenProcessToken, OpenThreadToken, SetThreadToken,
};
use windows::core::w;

/// While alive, this thread runs with SeSecurityPrivilege enabled. Not `Send`: it must be
/// dropped on the thread it impersonates.
pub(super) struct SecurityPrivilege {
    token: HANDLE,
    _thread_bound: PhantomData<*const ()>,
}

/// Closes a token handle exactly once.
struct Token(HANDLE);
impl Drop for Token {
    fn drop(&mut self) {
        // SAFETY: a handle this module opened or duplicated, closed once; a failed close
        // cannot be retried from a destructor.
        let _ = unsafe { CloseHandle(self.0) };
    }
}

impl SecurityPrivilege {
    pub(super) fn enable() -> Option<Self> {
        let mut current = HANDLE::default();
        // SAFETY: the pseudo-handle of this thread and a live out pointer. Success means the
        // thread already impersonates; leave that identity alone.
        if unsafe { OpenThreadToken(GetCurrentThread(), TOKEN_QUERY, true, &raw mut current) }
            .is_ok()
        {
            drop(Token(current));
            return None;
        }
        let mut process = HANDLE::default();
        // SAFETY: the pseudo-handle of this process and a live out pointer.
        unsafe {
            OpenProcessToken(
                GetCurrentProcess(),
                TOKEN_DUPLICATE | TOKEN_QUERY,
                &raw mut process,
            )
        }
        .ok()?;
        let process = Token(process);
        let mut duplicate = HANDLE::default();
        // SAFETY: a live token handle opened with TOKEN_DUPLICATE and a live out pointer; no
        // security attributes are inherited or passed.
        unsafe {
            DuplicateTokenEx(
                process.0,
                TOKEN_ADJUST_PRIVILEGES | TOKEN_IMPERSONATE | TOKEN_QUERY,
                None,
                SecurityImpersonation,
                TokenImpersonation,
                &raw mut duplicate,
            )
        }
        .ok()?;
        let duplicate = Token(duplicate);
        let mut luid = LUID::default();
        // SAFETY: a static NUL-terminated privilege name and a live out pointer.
        unsafe { LookupPrivilegeValueW(None, w!("SeSecurityPrivilege"), &raw mut luid) }.ok()?;
        let enabled = TOKEN_PRIVILEGES {
            PrivilegeCount: 1,
            Privileges: [LUID_AND_ATTRIBUTES {
                Luid: luid,
                Attributes: SE_PRIVILEGE_ENABLED,
            }],
        };
        // SAFETY: the private duplicate (TOKEN_ADJUST_PRIVILEGES) and an initialized,
        // single-entry structure; no previous state is requested.
        unsafe {
            AdjustTokenPrivileges(duplicate.0, false, Some(&raw const enabled), 0, None, None)
        }
        .ok()?;
        // Success still reports ERROR_NOT_ALL_ASSIGNED when the token lacks the privilege.
        // SAFETY: reads this thread's last-error value, set by the call just above.
        if unsafe { GetLastError() } == ERROR_NOT_ALL_ASSIGNED {
            return None;
        }
        // SAFETY: impersonate the private duplicate on this thread only (None = current).
        unsafe { SetThreadToken(None, Some(duplicate.0)) }.ok()?;
        let token = duplicate.0;
        std::mem::forget(duplicate);
        Some(Self {
            token,
            _thread_bound: PhantomData,
        })
    }
}

impl Drop for SecurityPrivilege {
    fn drop(&mut self) {
        // SAFETY: on the impersonating thread (the guard is not Send): None reverts it to
        // the process identity, as it was before `enable`. Then the duplicate is closed.
        let _ = unsafe { SetThreadToken(None, None) };
        drop(Token(self.token));
    }
}
