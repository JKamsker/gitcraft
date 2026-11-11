//! Failure hooks sit after actual production effects, never in a parallel state model.
#[cfg(any(test, recovery_recording))]
thread_local! {
    pub(super) static TRACE: std::cell::RefCell<(Vec<String>, Option<usize>)> = const {
        std::cell::RefCell::new((Vec::new(), None))
    };
}

#[cfg(recovery_recording)]
type BoundaryHook = std::cell::RefCell<Option<Box<dyn FnMut(&str)>>>;

#[cfg(recovery_recording)]
thread_local! {
    pub(super) static ABRUPT: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    pub(super) static HOOK: BoundaryHook = Default::default();
}

pub(super) fn boundary(label: &str) -> Result<(), String> {
    #[cfg(recovery_recording)]
    HOOK.with(|hook| {
        if let Some(hook) = hook.borrow_mut().as_mut() {
            hook(label);
        }
    });
    #[cfg(any(test, recovery_recording))]
    return TRACE.with(|trace| {
        let mut trace = trace.borrow_mut();
        trace.0.push(label.to_owned());
        if trace.1 == Some(trace.0.len()) {
            #[cfg(recovery_recording)]
            if ABRUPT.with(|mode| mode.get()) {
                // Escape the production call without running its error/rollback handlers.
                // Only RAII handles unwind; this is no process-kill/power-loss claim.
                std::panic::panic_any("recorded abrupt stop");
            }
            Err(format!("Injected interruption after {label}"))
        } else {
            Ok(())
        }
    });
    #[cfg(not(any(test, recovery_recording)))]
    {
        let _ = label;
        Ok(())
    }
}
