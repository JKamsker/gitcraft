//! The window's thread never waits for another machine: a command for one asked from it
//! fails at once (and is counted, for the tests that drive the window) instead of freezing the
//! window for as long as the connection takes.

use std::cell::Cell;

thread_local! {
    static ON_UI_THREAD: Cell<bool> = const { Cell::new(false) };
    /// Commands for another machine refused on this thread, the window's.
    static REFUSED: Cell<usize> = const { Cell::new(0) };
}

/// Marks the calling thread as the window's (the one that draws it).
pub fn mark_ui_thread() {
    ON_UI_THREAD.with(|on| on.set(true));
}

/// How many commands for another machine were asked from the window's thread so far, asked
/// on that thread (each window's thread counts its own: tests run side by side).
pub fn refused_on_ui_thread() -> usize {
    REFUSED.with(Cell::get)
}

/// An error when called on the window's thread: what is asked of another machine is asked
/// from a background thread, always (see the module's doc).
pub(super) fn off_ui_thread(what: &str) -> Result<(), String> {
    if !ON_UI_THREAD.with(Cell::get) {
        return Ok(());
    }
    REFUSED.with(|n| n.set(n.get() + 1));
    #[cfg(debug_assertions)]
    eprintln!(
        "gitcraft: {what} was asked of another machine on the window's thread\n{}",
        std::backtrace::Backtrace::force_capture()
    );
    Err(format!(
        "gitcraft asked another machine for {what} from the window's thread, which must never \
         wait for a network: please report this"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_marked_thread_is_refused() {
        assert!(off_ui_thread("x").is_ok());
        let counted = std::thread::spawn(|| {
            mark_ui_thread();
            let refused = off_ui_thread("x").is_err();
            (refused, refused_on_ui_thread())
        })
        .join()
        .ok();
        assert_eq!(counted, Some((true, 1)));
        assert!(off_ui_thread("x").is_ok());
        assert_eq!(refused_on_ui_thread(), 0);
    }
}
