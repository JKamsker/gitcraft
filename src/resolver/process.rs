//! The agent's process, on this machine: started with its prompt on its input, its output read
//! line by line as it comes (what it does, for the progress panel), stopped on request with
//! everything it started (see `ai::run::stop_here`).

use std::collections::VecDeque;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::Path;
use std::process::{Child, Stdio};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use super::agent::{self, Activity, Engine};
use crate::ai::run::{CommandLine, shell_here, stop_here};

/// The most activities kept.
const MAX_ACTIVITIES: usize = 300;
/// The most of its error output kept, from its end.
const MAX_STDERR: usize = 8 * 1024;
/// How often it is checked on.
const TICK: Duration = Duration::from_millis(200);
/// How long its output may stay open after it ended.
const LINGER: Duration = Duration::from_secs(3);

/// Sends on its channel when dropped: a reader's way of saying it is done.
struct Closed(std::sync::mpsc::Sender<()>);

impl Drop for Closed {
    fn drop(&mut self) {
        let _ = self.0.send(());
    }
}

/// How the process ended.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Exit {
    pub code: Option<i32>,
    /// It was stopped on request.
    pub stopped: bool,
    /// The end of what it printed as errors.
    pub stderr: String,
}

/// What came of the process so far.
#[derive(Debug, Default)]
pub struct Feed {
    pub activities: VecDeque<Activity>,
    pub exit: Option<Exit>,
}

/// A running agent. Dropped, it is stopped.
pub struct Running {
    child: Arc<Mutex<Option<Child>>>,
    feed: Arc<Mutex<Feed>>,
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

impl Running {
    /// Starts `command` in `dir` with `prompt` on its input; `changed` is called whenever
    /// there is something new to show.
    pub fn start(
        engine: Engine,
        command: &CommandLine,
        dir: &Path,
        prompt: String,
        changed: impl Fn() + Send + Sync + 'static,
    ) -> Result<Running, String> {
        let mut cmd = shell_here(command);
        cmd.current_dir(dir)
            .env("NO_COLOR", "1")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        crate::shell_path::apply(&mut cmd);
        let mut child = cmd
            .spawn()
            .map_err(|e| format!("Couldn't start {}: {e}", command.program()))?;
        if let Some(mut stdin) = child.stdin.take() {
            std::thread::spawn(move || {
                let _ = stdin.write_all(prompt.as_bytes());
            });
        }
        let feed = Arc::new(Mutex::new(Feed::default()));
        let changed = Arc::new(changed);
        let dir_text = dir.display().to_string();
        let stdout = child.stdout.take().map(|out| {
            let (feed, changed) = (Arc::clone(&feed), Arc::clone(&changed));
            let (done, read_all) = std::sync::mpsc::channel::<()>();
            std::thread::spawn(move || {
                // Told once the pipe closes, however the loop ends.
                let _done = Closed(done);
                for line in BufReader::new(out).lines() {
                    let Ok(line) = line else { break };
                    let found = agent::activities(engine, &line, &dir_text);
                    if found.is_empty() {
                        continue;
                    }
                    {
                        let mut feed = lock(&feed);
                        for a in found {
                            if feed.activities.len() == MAX_ACTIVITIES {
                                feed.activities.pop_front();
                            }
                            feed.activities.push_back(a);
                        }
                    }
                    changed();
                }
            });
            read_all
        });
        let stderr = child.stderr.take().map(|mut err| {
            let (tx, rx) = std::sync::mpsc::channel::<String>();
            std::thread::spawn(move || {
                let mut kept = Vec::new();
                let mut buf = [0u8; 4096];
                while let Ok(n) = err.read(&mut buf) {
                    if n == 0 {
                        break;
                    }
                    kept.extend_from_slice(&buf[..n]);
                    if kept.len() > MAX_STDERR * 2 {
                        kept.drain(..kept.len() - MAX_STDERR);
                    }
                }
                let from = kept.len().saturating_sub(MAX_STDERR);
                let _ = tx.send(String::from_utf8_lossy(&kept[from..]).into_owned());
            });
            rx
        });
        let child = Arc::new(Mutex::new(Some(child)));
        let (watched, fed) = (Arc::clone(&child), Arc::clone(&feed));
        std::thread::spawn(move || {
            let (code, stopped) = loop {
                {
                    let mut held = lock(&watched);
                    match held.as_mut().map(Child::try_wait) {
                        // Taken by `stop`.
                        None => break (None, true),
                        Some(Ok(Some(status))) => break (status.code(), false),
                        Some(Ok(None)) => {}
                        Some(Err(_)) => break (None, false),
                    }
                }
                std::thread::sleep(TICK);
            };
            // What it printed last (its result) is read before it counts as ended; but
            // something it left running may hold its output open, so not for ever.
            let until = std::time::Instant::now() + LINGER;
            if let Some(read_all) = stdout {
                let _ = read_all.recv_timeout(LINGER);
            }
            let stderr = stderr
                .and_then(|rx| {
                    rx.recv_timeout(until.saturating_duration_since(std::time::Instant::now()))
                        .ok()
                })
                .unwrap_or_default();
            lock(&fed).exit = Some(Exit {
                code,
                stopped,
                stderr,
            });
            changed();
        });
        Ok(Running { child, feed })
    }

    /// Stops it, and what it started, in the background.
    pub fn stop(&self) {
        if let Some(mut child) = lock(&self.child).take() {
            std::thread::spawn(move || stop_here(&mut child));
        }
    }

    /// Stops it, and what it started, before returning: for when nothing may be left behind
    /// (gitcraft is closing, or its folder is removed next).
    pub fn stop_now(&self) {
        let taken = lock(&self.child).take();
        if let Some(mut child) = taken {
            stop_here(&mut child);
        }
    }

    /// One that runs nothing and has done `activities`, for drawing the card in tests.
    #[cfg(test)]
    pub fn fake(activities: Vec<Activity>) -> Running {
        Running {
            child: Arc::new(Mutex::new(None)),
            feed: Arc::new(Mutex::new(Feed {
                activities: activities.into(),
                exit: None,
            })),
        }
    }

    /// Runs `f` on what came of it so far.
    pub fn with_feed<T>(&self, f: impl FnOnce(&Feed) -> T) -> T {
        f(&lock(&self.feed))
    }
}

impl Drop for Running {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    fn wait_for(running: &Running, until: impl Fn(&Feed) -> bool) -> bool {
        let start = Instant::now();
        while start.elapsed() < Duration::from_secs(20) {
            if running.with_feed(&until) {
                return true;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        false
    }

    /// A stand-in for the agent: a script of gitcraft's that echoes Claude Code's events.
    fn echo(lines: &[&str]) -> CommandLine {
        let sh = lines
            .iter()
            .map(|l| format!("printf '%s\\n' {}", crate::remote::sh_quote(l)))
            .collect::<Vec<_>>()
            .join("; ");
        let cmd = lines
            .iter()
            .map(|l| format!("echo {l}"))
            .collect::<Vec<_>>()
            .join(" & ");
        CommandLine::Script { sh, cmd }
    }

    #[test]
    fn its_output_is_read_as_it_comes_and_its_end_is_seen() -> Result<(), String> {
        let line = echo(&[
            r#"{"type":"assistant","message":{"content":[{"type":"text","text":"hello"}]}}"#,
            r#"{"type":"result","subtype":"success","is_error":false,"result":"done"}"#,
        ]);
        let dir = std::env::temp_dir();
        let running = Running::start(Engine::Claude, &line, &dir, String::new(), || {})?;
        assert!(wait_for(&running, |f| f.exit.is_some()), "it ended");
        running.with_feed(|f| {
            assert_eq!(
                f.exit.as_ref().map(|e| (e.code, e.stopped)),
                Some((Some(0), false))
            );
            assert_eq!(f.activities.front(), Some(&Activity::Said("hello".into())));
            assert!(matches!(
                f.activities.back(),
                Some(Activity::Ended { ok: true, .. })
            ));
        });
        Ok(())
    }

    #[test]
    fn a_stopped_agent_says_it_was_stopped() -> Result<(), String> {
        let line = CommandLine::Script {
            sh: "sleep 30".to_owned(),
            cmd: "ping -n 30 127.0.0.1 >NUL".to_owned(),
        };
        let dir = std::env::temp_dir();
        let running = Running::start(Engine::Claude, &line, &dir, String::new(), || {})?;
        running.stop();
        assert!(wait_for(&running, |f| f.exit.is_some()), "it ended");
        assert!(running.with_feed(|f| f.exit.as_ref().is_some_and(|e| e.stopped)));
        Ok(())
    }
}
