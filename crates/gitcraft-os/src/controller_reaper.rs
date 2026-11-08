//! Retain only directly owned native controllers after uncertain cleanup. This registry
//! never signals: EOF/deadline and each controller's own Service perform target cleanup.
use std::io;
use std::process::Child;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{OnceLock, mpsc};
use std::time::Duration;

const MAX: usize = 64;
static COUNT: AtomicUsize = AtomicUsize::new(0);
static REAPER: OnceLock<Result<mpsc::SyncSender<(Child, Permit)>, String>> = OnceLock::new();
pub(super) struct Permit;
impl Drop for Permit {
    fn drop(&mut self) {
        COUNT.fetch_sub(1, Ordering::AcqRel);
    }
}
pub(super) fn reserve() -> io::Result<Permit> {
    sender()?; // Establish the one bounded reaper before any helper may start.
    COUNT
        .try_update(Ordering::AcqRel, Ordering::Acquire, |n| {
            (n < MAX).then_some(n + 1)
        })
        .map_err(|_| io::Error::other("native controller recovery capacity is full"))?;
    Ok(Permit)
}
fn sender() -> io::Result<&'static mpsc::SyncSender<(Child, Permit)>> {
    REAPER
        .get_or_init(|| {
            let (tx, rx) = mpsc::sync_channel::<(Child, Permit)>(MAX);
            std::thread::Builder::new()
                .name("gitcraft-owner-reap".into())
                .spawn(move || {
                    let mut retained: Vec<(Child, Permit)> = Vec::new();
                    loop {
                        match rx.recv_timeout(Duration::from_millis(20)) {
                            Ok(child) => retained.push(child),
                            Err(mpsc::RecvTimeoutError::Timeout) => {}
                            Err(mpsc::RecvTimeoutError::Disconnected) => break,
                        }
                        retained.retain_mut(|(child, _)| match child.try_wait() {
                            Ok(Some(_)) => false,
                            // Do not signal a stale/unknown object on an observation error.
                            Ok(None) | Err(_) => true,
                        });
                    }
                })
                .map_err(|e| e.to_string())?;
            Ok(tx)
        })
        .as_ref()
        .map_err(|e| io::Error::other(e.clone()))
}
pub(super) fn retain(child: Child, permit: Permit) {
    // Admission reserved one of MAX slots before spawn. At most MAX items exist across
    // active owners, this queue and the reaper, so this send cannot fill a live queue.
    if let Ok(tx) = sender() {
        let _ = tx.send((child, permit));
    }
}
