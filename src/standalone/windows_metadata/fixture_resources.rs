//! Test-only ownership of the native PowerShell metadata runtime.
//!
//! The archived pr184 Windows core trace contains six helper timeouts followed by
//! two UI failures displaying that same error. The helpers each start PowerShell
//! and compile the native policy type with Add-Type. Keep these expensive fixture
//! operations from competing inside one libtest process, even outside CI's serial
//! standalone group. Production scheduling and the 20-second child bound stay intact.
//! This addresses helper overlap; Windows validation must still establish whether
//! it explains the historical failures. It cannot exclude contention from other
//! processes or establish a production root cause.
use std::sync::{Mutex, MutexGuard};

static NATIVE_RUNTIME: Mutex<()> = Mutex::new(());

pub(super) fn acquire() -> MutexGuard<'static, ()> {
    // The mutex protects resource occupancy, not mutable state. An unwinding
    // fixture releases occupancy and cannot invalidate the next fixture's state.
    NATIVE_RUNTIME.lock().unwrap_or_else(|e| e.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{
        Arc, Barrier,
        atomic::{AtomicUsize, Ordering},
    };

    #[test]
    fn recording_helpers_never_overlap_and_release_on_error_returns() {
        let start = Arc::new(Barrier::new(8));
        let active = Arc::new(AtomicUsize::new(0));
        let calls = Arc::new(AtomicUsize::new(0));
        std::thread::scope(|scope| {
            for _ in 0..8 {
                let (start, active, calls) = (start.clone(), active.clone(), calls.clone());
                scope.spawn(move || {
                    start.wait();
                    let record = || -> Result<(), ()> {
                        let _owned = acquire();
                        assert_eq!(active.fetch_add(1, Ordering::SeqCst), 0);
                        std::thread::yield_now();
                        calls.fetch_add(1, Ordering::SeqCst);
                        assert_eq!(active.fetch_sub(1, Ordering::SeqCst), 1);
                        Err(())
                    };
                    assert!(record().is_err());
                    // An error return must not retain the fixture resource.
                    assert!(record().is_err());
                });
            }
        });
        assert_eq!(calls.load(Ordering::SeqCst), 16);
        assert_eq!(active.load(Ordering::SeqCst), 0);
    }
}
