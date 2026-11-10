//! A timing log of what the window waits for, to find out afterwards why something was slow:
//! `timing.log` in gitcraft's own folder (see [`crate::portable::data_dir`]), on this machine
//! only and never in a report. What a diff read did, step by step (queued, each git call, each
//! stage of preparing it, shown or dropped), what made the working tree be read again, and any
//! git call that took long. No repository, path, argument or output is written: a file is told
//! by its extension, a change on disk by its top folder.
//!
//! Lines are written by a thread of their own; past 4 MiB the log becomes `timing.old.log` and
//! a new one is begun. `GITCRAFT_TIMING=0` turns it off; `GITCRAFT_TIMING=all` writes every git
//! call with its arguments, and every read answered without git (`native:cmd:…`), for finding
//! what is left to make faster.

use std::cell::RefCell;
use std::fmt::Display;
use std::io::Write;
use std::path::Path;
use std::sync::OnceLock;
use std::sync::mpsc::{Sender, channel};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// The log's name in gitcraft's folder.
const FILE: &str = "timing.log";
const OLD: &str = "timing.old.log";
const LIMIT: u64 = 4 << 20;
/// A git call outside any traced work is written only when it took at least this long.
const SLOW: Duration = Duration::from_millis(500);

thread_local! {
    /// The work this thread does now (`diff#12`): its git calls are written under that name.
    static SCOPE: RefCell<Option<String>> = const { RefCell::new(None) };
    /// Where the time of the last process this thread waited for went, until its git call is
    /// written (see [`git`]).
    static PROCESS: std::cell::Cell<Option<crate::processes::Phases>> =
        const { std::cell::Cell::new(None) };
}

/// `GITCRAFT_TIMING=all`: every git call is written, and its arguments with it. For working on
/// gitcraft itself: arguments may name files and branches.
pub(crate) fn all() -> bool {
    static ALL: OnceLock<bool> = OnceLock::new();
    *ALL.get_or_init(|| std::env::var_os("GITCRAFT_TIMING").is_some_and(|v| v == "all"))
}

/// Whether filesystem-event diagnostics can be written. Cached so each notify callback avoids
/// an environment lookup, and disabled in tests where the timing writer is also disabled.
pub(crate) fn enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED
        .get_or_init(|| !cfg!(test) && std::env::var_os("GITCRAFT_TIMING").is_none_or(|v| v != "0"))
}

fn writer() -> Option<&'static Sender<String>> {
    static WRITER: OnceLock<Option<Sender<String>>> = OnceLock::new();
    WRITER
        .get_or_init(|| {
            if !enabled() {
                return None;
            }
            let dir = crate::portable::data_dir()?;
            let (tx, rx) = channel::<String>();
            std::thread::Builder::new()
                .name("timing-log".into())
                .spawn(move || {
                    let mut file = open(&dir);
                    if let Some(f) = &mut file {
                        let _ = writeln!(
                            f,
                            "--- gitcraft {} started (pid {}, {} UTC)",
                            crate::update::CURRENT,
                            std::process::id(),
                            date_time(SystemTime::now())
                        );
                    }
                    while let Ok(line) = rx.recv() {
                        if file
                            .as_ref()
                            .and_then(|f| f.metadata().ok())
                            .is_some_and(|m| m.len() > LIMIT)
                        {
                            // Closed first: Windows renames no open file.
                            drop(file.take());
                            let _ = std::fs::rename(dir.join(FILE), dir.join(OLD));
                            file = open(&dir);
                        }
                        if let Some(f) = &mut file {
                            let _ = f.write_all(line.as_bytes());
                        }
                    }
                })
                .ok()?;
            Some(tx)
        })
        .as_ref()
}

fn open(dir: &Path) -> Option<std::fs::File> {
    std::fs::create_dir_all(dir).ok()?;
    std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join(FILE))
        .ok()
}

/// Writes `what` with the time and the thread's scope, if any.
pub(crate) fn log(what: impl Display) {
    let Some(tx) = writer() else { return };
    let scope = SCOPE.with(|s| s.borrow().clone());
    let time = time_of_day(SystemTime::now());
    let line = match scope {
        Some(scope) => format!("{time} {scope} {what}\n"),
        None => format!("{time} {what}\n"),
    };
    let _ = tx.send(line);
}

/// Names the work this thread does until the guard is dropped: what it logs, and the git calls
/// it makes, are written under `name`.
pub(crate) fn scope(name: impl Into<String>) -> Scope {
    let previous = SCOPE.with(|s| s.replace(Some(name.into())));
    Scope { previous }
}

pub(crate) struct Scope {
    previous: Option<String>,
}
impl Drop for Scope {
    fn drop(&mut self) {
        let previous = self.previous.take();
        SCOPE.with(|s| *s.borrow_mut() = previous);
    }
}

/// A stage of the scope's work that began at `since`.
pub(crate) fn step(name: &str, since: Instant) {
    if all() || SCOPE.with(|s| s.borrow().is_some()) {
        log(format_args!("{name} {}", ms(since.elapsed())));
    }
}

/// A git call (or a native read) that took `took`: written under the thread's scope, or alone
/// when it was slow.
pub(crate) fn git(name: &str, native: bool, took: Duration) {
    // Git's own are named `cli:diff`, `cli:cat-file`…
    let how = if native { "native:" } else { "" };
    let process = PROCESS.take().filter(|_| !native);
    if SCOPE.with(|s| s.borrow().is_some()) || took >= SLOW || all() {
        let process = process.map_or_else(String::new, |p| {
            format!(
                " (waited for a slot {}, started in {}, ran {}, output open {} after)",
                ms(p.queued),
                ms(p.spawn),
                ms(p.ran),
                ms(p.drained)
            )
        });
        log(format_args!("{how}{name} {}{process}", ms(took)));
    }
}

/// A git call is about to be made on this thread: what an earlier process left is not its own.
pub(crate) fn git_begins() {
    PROCESS.set(None);
}

/// Where the time of a process this thread waited for went.
pub(crate) fn process(phases: &crate::processes::Phases) {
    PROCESS.set(Some(*phases));
}

/// `took` in milliseconds, to a tenth.
pub(crate) fn ms(took: Duration) -> String {
    format!("{:.1} ms", took.as_secs_f64() * 1000.0)
}

/// How a file is told in the log: its extension (`.rs`), or `(none)`.
pub(crate) fn kind_of(path: &str) -> String {
    Path::new(path)
        .extension()
        .map_or_else(|| "(none)".into(), |e| format!(".{}", e.to_string_lossy()))
}

/// How a change on disk is told in the log: the top folder of `path` under `root` (`target/`),
/// or the kind of a file at the top (a tool's own, `.git` or `.vs`, by its name).
pub(crate) fn place_of(path: &Path, root: &Path) -> String {
    let inside = path.strip_prefix(root).unwrap_or(path);
    let mut parts = inside.components();
    match (parts.next(), parts.next()) {
        (Some(top), Some(_)) => format!("{}/", top.as_os_str().to_string_lossy()),
        (Some(top), None) => {
            let name = top.as_os_str().to_string_lossy();
            if name.starts_with('.') && !name[1..].contains('.') {
                name.into_owned()
            } else {
                format!("top-level {}", kind_of(&name))
            }
        }
        _ => "(root)".into(),
    }
}

fn time_of_day(now: SystemTime) -> String {
    let since = now.duration_since(UNIX_EPOCH).unwrap_or_default();
    let secs = since.as_secs() % 86_400;
    format!(
        "{:02}:{:02}:{:02}.{:03}",
        secs / 3600,
        secs / 60 % 60,
        secs % 60,
        since.subsec_millis()
    )
}

fn date_time(now: SystemTime) -> String {
    let since = now.duration_since(UNIX_EPOCH).unwrap_or_default();
    // Days to a civil date (Howard Hinnant's algorithm).
    let days = (since.as_secs() / 86_400) as i64 + 719_468;
    let era = days.div_euclid(146_097);
    let doe = days - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02} {}", &time_of_day(now)[..8])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn times_and_places_are_told_without_names() {
        let at = UNIX_EPOCH + Duration::from_millis(1_762_770_727_123);
        assert_eq!(date_time(at), "2025-11-10 10:32:07");
        assert_eq!(time_of_day(at), "10:32:07.123");
        let root = Path::new("/work/repo");
        assert_eq!(
            place_of(Path::new("/work/repo/target/debug/x.o"), root),
            "target/"
        );
        assert_eq!(
            place_of(Path::new("/work/repo/Cargo.toml"), root),
            "top-level .toml"
        );
        assert_eq!(place_of(Path::new("/work/repo/.git"), root), ".git");
        assert_eq!(kind_of("src/secret-name"), "(none)");
        assert_eq!(ms(Duration::from_micros(12_345)), "12.3 ms");
    }
}
