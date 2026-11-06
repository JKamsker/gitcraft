//! One gitcraft per user: a start while gitcraft is running hands what it was started with to
//! the running window (a folder to open, or just "come to the front") and ends. Two windows
//! would each save their tabs, recent repositories and settings over the other's, and the one
//! closed last would win.
//!
//! The running instance holds an exclusive lock in the app's data folder (see [`lock`]) and
//! listens on 127.0.0.1 with a random token (see [`server`]); the port and token are in a
//! file there only the user can read (see [`discovery`]). A later start that can't take the
//! lock reads that file and sends its request (see [`client`] and [`message`]). The data
//! folder is the same for every profile (they are kept in one save), so there is one instance
//! per user, whichever profile it shows.
//!
//! A crash leaves nothing in the way: the system lets go of the lock with the process, and the
//! next start takes it and writes its own file. A start that finds the lock held but no answer
//! keeps trying for a while (the running one may be starting, or closing), then says so and
//! ends rather than open a second window.

mod client;
mod discovery;
mod lock;
mod message;
mod server;
#[cfg(test)]
mod tests;

use std::path::Path;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

pub use message::Request;
use server::Primary;

/// How long a start waits for the running instance to answer before it gives up.
const PATIENCE: Duration = Duration::from_secs(10);

/// How long between two tries.
const RETRY: Duration = Duration::from_millis(100);

/// What this start is to do.
#[derive(Debug)]
pub enum Start {
    /// Open the window: this is the running instance (or there's nothing to share with one).
    Window,
    /// End: the running instance took the request.
    HandedOver,
    /// End, and tell the user this.
    Failed(String),
}

/// This process, when it is the running instance.
static PRIMARY: OnceLock<Primary> = OnceLock::new();

/// Settles whether this process opens the window or hands `arg` (what it was started with)
/// to the instance already running. Called before the window is made.
pub fn start(arg: Option<&Path>) -> Start {
    // No data folder: nothing saved for two windows to write over.
    let Some(dir) = eframe::storage_dir("gitcraft") else {
        return Start::Window;
    };
    match claim(&dir, &request_for(arg), PATIENCE) {
        Claim::Primary(primary) => {
            let _ = PRIMARY.set(primary);
            Start::Window
        }
        Claim::Alone(e) => {
            eprintln!("gitcraft: can't tell whether gitcraft is already running: {e}");
            Start::Window
        }
        Claim::HandedOver => Start::HandedOver,
        Claim::Failed(message) => Start::Failed(message),
    }
}

/// Has the window redrawn when a later start asks something.
pub fn set_repaint(ctx: egui::Context) {
    if let Some(primary) = PRIMARY.get() {
        primary.set_repaint(ctx);
    }
}

/// What later starts asked since the last call, the oldest first.
pub fn take_requests() -> Vec<Request> {
    PRIMARY
        .get()
        .map(Primary::take_requests)
        .unwrap_or_default()
}

/// The window is closing: what later starts ask is no longer taken (they wait for the next
/// running instance).
pub fn closing() {
    if let Some(primary) = PRIMARY.get() {
        primary.closing();
    }
}

/// The window closed and what it keeps is on disk: the next start is the running instance.
pub fn shut_down() {
    if let Some(primary) = PRIMARY.get() {
        primary.shut_down();
    }
}

/// What a start becomes.
enum Claim {
    Primary(Primary),
    HandedOver,
    Failed(String),
    /// There's no lock to take (see [`lock::Lock::Unavailable`]).
    Alone(std::io::Error),
}

/// Takes the lock in `dir`, or hands `request` to whoever holds it, trying for `patience`.
fn claim(dir: &Path, request: &Result<Request, String>, patience: Duration) -> Claim {
    let deadline = Instant::now() + patience;
    loop {
        match lock::try_take(dir) {
            lock::Lock::Held(held) => return Claim::Primary(Primary::start(dir, held)),
            lock::Lock::Unavailable(e) => return Claim::Alone(e),
            lock::Lock::Taken => {
                let request = match request {
                    Ok(request) => request,
                    Err(e) => return Claim::Failed(e.clone()),
                };
                // Not there yet (the running one is starting), or a crashed one's: tried again.
                if let Some(found) = discovery::read(dir) {
                    match client::hand_over(&found, request) {
                        client::Handoff::Taken => return Claim::HandedOver,
                        client::Handoff::Refused(why) => {
                            return Claim::Failed(format!(
                                "The running gitcraft couldn't take this: {why}."
                            ));
                        }
                        client::Handoff::NoAnswer => {}
                    }
                }
            }
        }
        if Instant::now() >= deadline {
            return Claim::Failed(
                "gitcraft is already running, but it doesn't answer. Close it (or end it in \
                 the task manager) and start gitcraft again."
                    .to_owned(),
            );
        }
        std::thread::sleep(RETRY);
    }
}

/// What to ask the running instance for `arg`: to open it, read against this start's folder
/// (`.`, a subfolder of a repository), or with none just to come to the front.
fn request_for(arg: Option<&Path>) -> Result<Request, String> {
    let Some(path) = arg.filter(|p| !p.as_os_str().is_empty()) else {
        return Ok(Request::Focus);
    };
    let absolute = if crate::remote::is_remote(path) {
        path.to_owned()
    } else {
        std::path::absolute(path).map_err(|e| format!("{}: {e}", path.display()))?
    };
    let text = absolute.to_str().ok_or_else(|| {
        format!(
            "gitcraft is already running, and {} can't be handed to it: the name isn't valid \
             UTF-8. Open it from the running window instead.",
            path.display()
        )
    })?;
    message::valid_path(text)
        .map(Request::Open)
        .map_err(|why| format!("{} can't be opened: {why}.", path.display()))
}

/// A new, empty folder for a test, under the system's temporary folder.
#[cfg(test)]
fn temp_dir(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "gitcraft-instance-{name}-{}-{}",
        std::process::id(),
        crate::remote::askpass::random_token()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}
