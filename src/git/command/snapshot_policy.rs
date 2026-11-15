//! Hook suppression for all Git invocations in a snapshot operation, including unwind cleanup.
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
        command.args(["-c", "core.hooksPath=/dev/null"]);
    }
}
