//! Preparation policy for all snapshot Git invocations, including unwind cleanup.
use std::cell::Cell;
thread_local! { static ACTIVE: Cell<bool> = const { Cell::new(false) }; }
pub(in crate::git) fn without_hooks<T>(work: impl FnOnce() -> T) -> T {
    struct Restore(bool);
    impl Drop for Restore {
        fn drop(&mut self) {
            ACTIVE.with(|a| a.set(self.0));
        }
    }
    let _restore = Restore(ACTIVE.with(|a| a.replace(true)));
    work()
}
pub(super) fn apply(command: &mut std::process::Command) {
    if ACTIVE.with(Cell::get) {
        command.args([
            "-c",
            "core.hooksPath=/dev/null",
            "-c",
            "core.fsmonitor=false",
            // A relative global attributes file could resolve to unstaged source bytes
            // here and different staged bytes in the checkout. Use tree/info attributes only.
            "-c",
            "core.attributesFile=/dev/null",
        ]);
    }
}
