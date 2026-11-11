//! Opt-in, test-thread-only stages for the unresolved native Windows CI hangs.
//! No timers, background threads, process inspection, or changed test deadlines.
use std::{cell::Cell, fmt, time::Instant};

thread_local! {
    static TEST: Cell<Option<&'static str>> = const { Cell::new(None) };
}

pub(crate) struct Trace(Option<&'static str>);

pub(crate) fn trace(test: &'static str) -> Trace {
    Trace(TEST.with(|slot| slot.replace(Some(test))))
}

impl Drop for Trace {
    fn drop(&mut self) {
        TEST.with(|slot| slot.set(self.0));
    }
}

pub(crate) struct Stage {
    test: &'static str,
    label: String,
    started: Instant,
}

pub(crate) fn stage(label: fmt::Arguments<'_>) -> Option<Stage> {
    let test = TEST.with(Cell::get)?;
    let stage = Stage {
        test,
        label: label.to_string(),
        started: Instant::now(),
    };
    eprintln!("[hang-stage {test}] begin {}", stage.label);
    Some(stage)
}

impl Drop for Stage {
    fn drop(&mut self) {
        eprintln!(
            "[hang-stage {}] {} {} ({:?})",
            self.test,
            if std::thread::panicking() {
                "unwind"
            } else {
                "end"
            },
            self.label,
            self.started.elapsed()
        );
    }
}

pub(crate) fn run<T>(label: fmt::Arguments<'_>, work: impl FnOnce() -> T) -> T {
    let _stage = stage(label);
    work()
}

/// Includes entry and exit for each poll: a blocked poll can exceed a wait loop's deadline.
/// Read fields directly; diagnostics must not acquire scheduler or egui global locks.
pub(crate) fn poll(tab: &mut super::RepoTab, ctx: &egui::Context) {
    let _stage = stage(format_args!(
        "poll loading={} history_gen={} comparison_data={} interaction={}",
        tab.loading,
        tab.history_gen,
        tab.versions
            .view
            .as_ref()
            .is_some_and(|view| view.data.is_some()),
        tab.interactions.going(),
    ));
    tab.poll(ctx);
}

/// Test-thread breadcrumbs around synchronous poll work, without inspecting other threads.
pub(crate) fn checkpoint(label: fmt::Arguments<'_>) {
    if let Some(test) = TEST.with(Cell::get) {
        eprintln!("[hang-stage {test}] reached {label}");
    }
}

/// Labels synchronous message handling in an active trace; inactive traces return no guard.
pub(super) fn message(msg: &super::messages::Msg) -> Option<Stage> {
    use super::messages::Msg;
    let label = match msg {
        Msg::Loaded(_) => "Loaded",
        Msg::Watcher(..) => "Watcher",
        Msg::Then(_) => "Then",
        Msg::Status(..) => "Status",
        Msg::Submodules(..) => "Submodules",
        Msg::Detail(..) => "Detail",
        Msg::RefsCheck(_) => "RefsCheck",
        _ => "other",
    };
    stage(format_args!(
        "handle {label} {:?}",
        std::mem::discriminant(msg)
    ))
}
