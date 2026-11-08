//! Agent execution through the application-owned process service. Its controller owns
//! descendant containment, cancellation, pipe draining and reaping.
use super::agent::{self, Activity, Engine};
use crate::ai::run::{CommandLine, shell_here};
use std::collections::VecDeque;
use std::path::Path;
use std::sync::{Arc, Mutex, PoisonError};
const MAX_ACTIVITIES: usize = 300;
const MAX_EVENT: usize = 4 * 1024 * 1024;
const MAX_STDERR: usize = 8 * 1024;
const MAX_OUTPUT: usize = 64 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Exit {
    pub code: Option<i32>,
    pub safe_to_inspect: bool,
    pub stopped: bool,
    pub stderr: String,
}
#[derive(Debug, Default)]
pub struct Feed {
    pub activities: VecDeque<Activity>,
    pub exit: Option<Exit>,
}
pub struct Running {
    cancel: crate::tasks::Cancellation,
    worker: Mutex<Option<std::thread::JoinHandle<()>>>,
    feed: Arc<Mutex<Feed>>,
}
fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

#[derive(Default)]
struct Lines {
    line: Vec<u8>,
    too_long: bool,
}
impl Lines {
    fn push(&mut self, bytes: &[u8], max: usize, mut each: impl FnMut(&str)) {
        for part in bytes.split_inclusive(|b| *b == b'\n') {
            let ended = part.last() == Some(&b'\n');
            let part = if ended { &part[..part.len() - 1] } else { part };
            if self.line.len().saturating_add(part.len()) > max {
                self.too_long = true;
                self.line.clear();
            } else if !self.too_long {
                self.line.extend_from_slice(part);
            }
            if ended {
                self.finish(&mut each);
            }
        }
    }
    fn finish(&mut self, mut each: impl FnMut(&str)) {
        if !self.too_long && !self.line.is_empty() {
            each(String::from_utf8_lossy(&self.line).trim_end_matches('\r'));
        }
        self.line.clear();
        self.too_long = false;
    }
}
fn record(feed: &Mutex<Feed>, engine: Engine, dir: &str, line: &str) {
    let found = agent::activities(engine, line, dir);
    let mut feed = lock(feed);
    for activity in found {
        if feed.activities.len() == MAX_ACTIVITIES {
            feed.activities.pop_front();
        }
        feed.activities.push_back(activity);
    }
}
fn failure_exit(error: crate::processes::Failure) -> Exit {
    use crate::processes::Failure;
    Exit {
        code: None,
        safe_to_inspect: matches!(
            error,
            Failure::CancelledBeforeStart | Failure::TimedOutBeforeStart
        ),
        stopped: matches!(error, Failure::CancelledBeforeStart),
        stderr: error.message(),
    }
}

impl Running {
    pub fn start(
        engine: Engine,
        command: &CommandLine,
        dir: &Path,
        prompt: String,
        changed: impl Fn() + Send + Sync + 'static,
    ) -> Result<Running, String> {
        let mut cmd = shell_here(command);
        cmd.current_dir(dir).env("NO_COLOR", "1");
        crate::shell_path::apply(&mut cmd);
        let cancel = crate::tasks::Cancellation::default();
        let feed = Arc::new(Mutex::new(Feed::default()));
        let lines = Arc::new(Mutex::new(Lines::default()));
        let dir = dir.display().to_string();
        let changed = Arc::new(changed);
        let errors = Arc::new(Mutex::new(Vec::<u8>::new()));
        let tail = errors.clone();
        let (fed, parsed, notify, cwd) =
            (feed.clone(), lines.clone(), changed.clone(), dir.clone());
        let options = crate::processes::Options {
            input: Some(prompt.into_bytes()),
            background: true,
            cancel: cancel.clone(),
            output_limit: MAX_OUTPUT,
            stderr_limit: Some(MAX_STDERR),
            stderr_filter: Some(Box::new(move |bytes| {
                let mut tail = lock(&tail);
                let keep = bytes.len().min(MAX_STDERR);
                let remove = tail.len().saturating_add(keep).saturating_sub(MAX_STDERR);
                tail.drain(..remove);
                tail.extend_from_slice(&bytes[bytes.len() - keep..]);
                Vec::new()
            })),
            stdout: Some(Box::new(move |bytes| {
                lock(&parsed).push(bytes, MAX_EVENT, |line| record(&fed, engine, &cwd, line));
                notify();
            })),
            ..Default::default()
        };
        let fed = feed.clone();
        let worker = std::thread::Builder::new()
            .name("resolver-process".into())
            .spawn(move || {
                let result = crate::processes::run_checked(cmd, options);
                lock(&lines).finish(|line| record(&fed, engine, &dir, line));
                let exit = match result {
                    Ok(out) => Exit {
                        code: out.stopped.is_none().then(|| out.status.code()).flatten(),
                        safe_to_inspect: out.stop_kind
                            != Some(crate::processes::StopKind::Uncertain),
                        stopped: out.stop_kind == Some(crate::processes::StopKind::Cancelled),
                        stderr: out.stopped.unwrap_or_else(|| {
                            String::from_utf8_lossy(&lock(&errors)).into_owned()
                        }),
                    },
                    Err(error) => failure_exit(error),
                };
                lock(&fed).exit = Some(exit);
                changed();
            })
            .map_err(|e| e.to_string())?;
        Ok(Running {
            cancel,
            worker: Mutex::new(Some(worker)),
            feed,
        })
    }
    /// A worker panic may leave no feed exit at all; surface it as uncertainty.
    pub fn exit(&self) -> Option<Exit> {
        let exit = self.with_feed(|f| f.exit.clone());
        if exit.is_some() {
            return exit;
        }
        lock(&self.worker)
            .as_ref()
            .filter(|w| w.is_finished())
            .map(|_| {
                failure_exit(crate::processes::Failure::Failed(
                    "agent worker ended without cleanup proof".into(),
                ))
            })
    }

    pub fn stop(&self) {
        self.cancel.cancel();
    }
    pub fn stop_now(&self) -> bool {
        self.stop();
        // Holding this lock through join also waits for another stop already under way.
        let mut worker = lock(&self.worker);
        if let Some(worker) = worker.take() {
            if worker.join().is_err() {
                lock(&self.feed).exit = None;
                return false;
            }
        }
        self.with_feed(|f| f.exit.as_ref().is_some_and(|e| e.safe_to_inspect))
    }
    #[cfg(test)]
    pub fn fake(activities: Vec<Activity>) -> Running {
        Running {
            cancel: Default::default(),
            worker: Mutex::new(None),
            feed: Arc::new(Mutex::new(Feed {
                activities: activities.into(),
                exit: None,
            })),
        }
    }
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
fn each_line(mut out: impl std::io::Read, max: usize, mut each: impl FnMut(&str)) {
    let mut lines = Lines::default();
    let mut buf = [0; 8192];
    while let Ok(n) = out.read(&mut buf) {
        if n == 0 {
            break;
        }
        lines.push(&buf[..n], max, &mut each);
    }
    lines.finish(each);
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;
    use std::time::{Duration, Instant};

    #[test]
    fn recording_failure_mapping_and_join_proof_fail_closed() {
        use crate::processes::Failure;
        assert!(failure_exit(Failure::CancelledBeforeStart).safe_to_inspect);
        assert!(failure_exit(Failure::TimedOutBeforeStart).safe_to_inspect);
        for reason in ["lost service", "owned cleanup failed", "cancelled"] {
            assert!(!failure_exit(Failure::Failed(reason.into())).safe_to_inspect);
        }
        let running = Running::fake(vec![]);
        assert!(!running.stop_now());
        lock(&running.feed).exit = Some(failure_exit(Failure::CancelledBeforeStart));
        assert!(running.stop_now());
        *lock(&running.worker) = Some(std::thread::spawn(|| panic!("synthetic worker panic")));
        assert!(!running.stop_now());
        assert!(
            !running.stop_now(),
            "panic invalidates even an earlier safe exit"
        );
    }

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

    /// No line takes more than the most read, ended or not; the lines around one too long
    /// still come, and the last one without its line end too.
    #[test]
    fn its_output_is_read_a_bounded_line_at_a_time() {
        let mut text = b"first\r\n".to_vec();
        text.extend(std::iter::repeat_n(b'x', 50));
        text.extend_from_slice(b"\nsecond\n");
        text.extend(std::iter::repeat_n(b'y', 50));
        text.extend_from_slice(b"\nlast");
        /// Hands out three bytes at a time, as a pipe may.
        struct Trickle(std::io::Cursor<Vec<u8>>);
        impl Read for Trickle {
            fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
                let n = buf.len().min(3);
                self.0.read(&mut buf[..n])
            }
        }
        let mut seen = Vec::new();
        each_line(Trickle(std::io::Cursor::new(text)), 10, |l| {
            seen.push(l.to_owned());
        });
        assert_eq!(seen, ["first", "second", "last"]);
        let mut unended = Vec::new();
        each_line(&[b'z'; 40][..], 10, |l| unended.push(l.to_owned()));
        assert!(unended.is_empty());
    }

    /// `stop_now` returns once the agent is gone, a `stop` under way included, and its end
    /// is told as stopped.
    #[test]
    fn stopping_now_waits_for_a_stop_under_way() -> Result<(), String> {
        let line = CommandLine::Script {
            sh: "sleep 30".to_owned(),
            cmd: "ping -n 30 127.0.0.1 >NUL".to_owned(),
        };
        let dir = std::env::temp_dir();
        let running = Running::start(Engine::Claude, &line, &dir, String::new(), || {})?;
        running.stop();
        running.stop_now();
        assert!(lock(&running.worker).is_none());
        assert!(wait_for(&running, |f| f.exit.is_some()), "it ended");
        assert!(running.with_feed(|f| f.exit.as_ref().is_some_and(|e| e.stopped)));
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
