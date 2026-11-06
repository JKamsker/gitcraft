//! Offline and metered mode: whether gitcraft goes on the network by itself.
//!
//! The user's switch ([`Settings::offline`](crate::settings::Settings)) turns it all off:
//! background work pauses, and fetch, pull, push and the like are refused with why rather than
//! left to hang. The system is also asked, every minute, whether there is a connection and
//! whether it is metered (see [`detect`]); without one, or on a metered one (unless the user
//! says otherwise), only the background work pauses: the background fetch, avatars, update
//! checks and downloads, and the polling of builds and pull requests. What the user asks for
//! still runs: the system can be wrong (a proxy, a captive portal). The background work also
//! waits for the system's first answer (a few seconds at most), so that a metered connection
//! is known before the first update check goes out.

mod detect;

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;

pub use detect::Detected;

/// Why gitcraft isn't going on the network by itself.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pause {
    /// The user turned offline mode on.
    Switch,
    /// The system says there is no connection.
    NoConnection,
    /// The connection is metered (and the user wants that respected).
    Metered,
}

impl Pause {
    /// What the toolbar says: "Offline".
    pub fn label(self) -> &'static str {
        match self {
            Pause::Switch => "Offline",
            Pause::NoConnection => "No connection",
            Pause::Metered => "Metered network",
        }
    }

    /// Why, in a sentence.
    pub fn why(self) -> &'static str {
        match self {
            Pause::Switch => "Offline mode is on",
            Pause::NoConnection => "The system reports no network connection",
            Pause::Metered => "The network connection is metered",
        }
    }
}

/// The user's choices, mirrored from the settings every frame.
static OFFLINE: AtomicBool = AtomicBool::new(false);
static PAUSE_ON_METERED: AtomicBool = AtomicBool::new(true);
/// What the system said last.
static DETECTED: Mutex<Detected> = Mutex::new(Detected::UNKNOWN);
/// Counts the times the background work could start again (see [`resumed`]).
static RESUMED: AtomicU64 = AtomicU64::new(0);

/// Takes in the user's choices (the offline switch, and whether a metered network pauses the
/// background work).
pub fn set_choices(offline: bool, pause_on_metered: bool) {
    let before = background_paused();
    OFFLINE.store(offline, Ordering::SeqCst);
    PAUSE_ON_METERED.store(pause_on_metered, Ordering::SeqCst);
    note_resume(before);
}

fn set_detected(detected: Detected) {
    let before = background_may_run();
    if let Ok(mut d) = DETECTED.lock() {
        *d = detected;
    }
    ANSWERED.store(true, Ordering::SeqCst);
    if !before && background_may_run() {
        RESUMED.fetch_add(1, Ordering::SeqCst);
    }
}

fn note_resume(before: Option<Pause>) {
    if before.is_some() && background_paused().is_none() {
        RESUMED.fetch_add(1, Ordering::SeqCst);
    }
}

/// The system has answered once (or was given long enough to).
static ANSWERED: AtomicBool = AtomicBool::new(false);
/// When the system was first asked.
static WATCHING_SINCE: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();

/// How long the background work waits for the system's first answer at most.
const FIRST_ANSWER_WITHIN: Duration = Duration::from_secs(5);

/// Whether the system has said what the connection is (a metered one must be known before the
/// first update check or avatar goes out); true once it was given long enough, or when it
/// isn't asked at all.
fn answered() -> bool {
    ANSWERED.load(Ordering::SeqCst)
        || WATCHING_SINCE
            .get()
            .is_none_or(|t| t.elapsed() > FIRST_ANSWER_WITHIN)
}

/// Whether background work may go on the network now: nothing pauses it, and the system has
/// been asked first.
pub fn background_may_run() -> bool {
    #[cfg(test)]
    if let Some(pause) = test_pause::get() {
        return pause.is_none();
    }
    answered() && background_paused().is_none()
}

/// How many times the background work could start again: what was put off while paused (an
/// avatar) is asked for again once this has moved.
pub fn resumed() -> u64 {
    RESUMED.load(Ordering::SeqCst)
}

fn detected() -> Detected {
    DETECTED.lock().map(|d| *d).unwrap_or(Detected::UNKNOWN)
}

/// Why the background work is paused, if it is.
pub fn background_paused() -> Option<Pause> {
    #[cfg(test)]
    if let Some(pause) = test_pause::get() {
        return pause;
    }
    pause_for(
        OFFLINE.load(Ordering::SeqCst),
        PAUSE_ON_METERED.load(Ordering::SeqCst),
        detected(),
    )
}

/// Why what the user asks for that goes on the network is refused: only the switch refuses it.
pub fn refused() -> Option<Pause> {
    #[cfg(test)]
    if let Some(pause) = test_pause::get() {
        return pause.filter(|p| *p == Pause::Switch);
    }
    OFFLINE.load(Ordering::SeqCst).then_some(Pause::Switch)
}

/// For tests: a pause for this thread only, so that tests running alongside are not paused.
#[cfg(test)]
pub(crate) mod test_pause {
    use std::cell::Cell;

    use super::Pause;

    thread_local! {
        static PAUSE: Cell<Option<Option<Pause>>> = const { Cell::new(None) };
    }

    /// Pauses (or not) this thread's view of the network until the guard is dropped.
    pub(crate) fn set(pause: Option<Pause>) -> Guard {
        PAUSE.with(|p| p.set(Some(pause)));
        Guard
    }

    pub(super) fn get() -> Option<Option<Pause>> {
        PAUSE.with(Cell::get)
    }

    pub(crate) struct Guard;

    impl Drop for Guard {
        fn drop(&mut self) {
            PAUSE.with(|p| p.set(None));
        }
    }
}

/// The rule: the switch first, then no connection, then metered when that pauses.
pub fn pause_for(offline: bool, pause_on_metered: bool, detected: Detected) -> Option<Pause> {
    if offline {
        Some(Pause::Switch)
    } else if detected.connected == Some(false) {
        Some(Pause::NoConnection)
    } else if pause_on_metered && detected.metered == Some(true) {
        Some(Pause::Metered)
    } else {
        None
    }
}

/// How often the system is asked.
const DETECT_EVERY: Duration = Duration::from_secs(60);

/// Starts asking the system about the connection, once, in the background; the window is
/// repainted when the answer changes.
pub fn start_watching(ctx: &egui::Context) {
    static STARTED: AtomicBool = AtomicBool::new(false);
    if STARTED.swap(true, Ordering::SeqCst) {
        return;
    }
    let _ = WATCHING_SINCE.set(std::time::Instant::now());
    let ctx = ctx.clone();
    std::thread::spawn(move || {
        loop {
            // With offline mode on, what the system says changes nothing: not asked.
            if !OFFLINE.load(Ordering::SeqCst) || !ANSWERED.load(Ordering::SeqCst) {
                let now = detect::ask_system();
                if now != detected() || !ANSWERED.load(Ordering::SeqCst) {
                    set_detected(now);
                    ctx.request_repaint();
                }
            }
            std::thread::sleep(DETECT_EVERY);
        }
    });
}

#[cfg(test)]
mod tests;
