//! Opt-in, test-thread-only stages for the unresolved native Windows CI hangs.
//! Traces stay opt-in; bounded tests also supervise blocking calls and teardown.
use std::{
    cell::{Cell, RefCell},
    fmt,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

thread_local! {
    static ACTIVE: RefCell<Option<Arc<Mutex<Vec<String>>>>> = const { RefCell::new(None) };
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
    active: Option<Arc<Mutex<Vec<String>>>>,
}

pub(crate) fn stage(label: fmt::Arguments<'_>) -> Option<Stage> {
    let test = TEST.with(Cell::get)?;
    let stage = Stage {
        test,
        label: label.to_string(),
        started: Instant::now(),
        active: ACTIVE.with(|slot| slot.borrow().clone()),
    };
    if let Some(active) = &stage.active {
        active
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(stage.label.clone());
    }
    eprintln!("[hang-stage {test}] begin {}", stage.label);
    Some(stage)
}

impl Drop for Stage {
    fn drop(&mut self) {
        if let Some(active) = &self.active {
            active
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .pop();
        }
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

/// A deadline on the whole owned body, including synchronous Git, polls and destructors.
/// Never join after timeout: the worker may be blocked. CI's owned-process watchdog
/// captures its stacks before terminating the test process; no host PID selectors.
#[allow(clippy::expect_used, clippy::panic)] // Harness launch/deadline failures must fail libtest.
pub(crate) fn bounded<T: Send + 'static>(
    test: &'static str,
    work: impl FnOnce() -> T + Send + 'static,
) -> T {
    let active = Arc::new(Mutex::new(Vec::<String>::new()));
    let stages = active.clone();
    let (send, receive) = std::sync::mpsc::sync_channel(1);
    std::thread::Builder::new()
        .name(std::thread::current().name().unwrap_or(test).to_owned())
        .spawn(move || {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let _trace = trace(test);
                ACTIVE.with(|slot| *slot.borrow_mut() = Some(stages));
                crate::connections::Context::default().run(|| run(format_args!("test body"), work))
            }));
            let _ = send.send(result);
        })
        .expect("spawn bounded test");
    match receive.recv_timeout(Duration::from_secs(90)) {
        Ok(Ok(value)) => value,
        Ok(Err(panic)) => std::panic::resume_unwind(panic),
        Err(error) => {
            let stages = active
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .clone();
            eprintln!("[hang-deadline {test}] {error}; active stages: {stages:?}");
            // Give the Windows CI watchdog time to snapshot/dump the still-live worker.
            if cfg!(windows) && std::env::var_os("GITHUB_ACTIONS").is_some() {
                std::thread::sleep(Duration::from_secs(15));
            }
            panic!("{test} exceeded its 90s body deadline: {stages:?}");
        }
    }
}

/// Preserve normal scope drop order and trace cleanup on success, errors and unwinding.
pub(crate) struct Owned<T> {
    label: &'static str,
    value: Option<T>,
}

pub(crate) fn owned<T>(label: &'static str, value: T) -> Owned<T> {
    Owned {
        label,
        value: Some(value),
    }
}

#[allow(clippy::expect_used)] // The slot is emptied only during Drop, after all access ends.
impl<T> std::ops::Deref for Owned<T> {
    type Target = T;
    fn deref(&self) -> &T {
        self.value.as_ref().expect("owned value")
    }
}
#[allow(clippy::expect_used)] // The slot is emptied only during Drop, after all access ends.
impl<T> std::ops::DerefMut for Owned<T> {
    fn deref_mut(&mut self) -> &mut T {
        self.value.as_mut().expect("owned value")
    }
}
impl<T> Drop for Owned<T> {
    fn drop(&mut self) {
        run(format_args!("{}", self.label), || drop(self.value.take()));
    }
}
