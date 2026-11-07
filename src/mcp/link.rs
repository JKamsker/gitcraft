//! How `gitcraft mcp` reaches the running window: over the single-instance channel (see
//! `crate::instance`), a 127.0.0.1 listener whose port and token are in a file only this user
//! can read, believed only while the running instance holds its lock. An agent's change goes
//! there as a request of its own kind (`agent`), and its connection stays open until the window
//! answers how it ended; closing it (the call was cancelled, the agent quit) tells the window
//! the agent is gone.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::time::Duration;

use super::wire::{Reply, WriteOp};
use crate::instance::{self, AgentCall, Primary};

/// The running instance's data folder, unless `GITCRAFT_MCP_DIR` names another (the tests do,
/// to have a world of their own).
pub fn dir() -> Option<PathBuf> {
    match std::env::var_os("GITCRAFT_MCP_DIR") {
        Some(dir) if !dir.is_empty() => Some(PathBuf::from(dir)),
        _ => eframe::storage_dir("gitcraft"),
    }
}

/// How long the window may take to answer: an operation that runs long (a push), or an
/// approval card waiting for the user, ends before this (see `repo::agent`).
pub const ANSWER_WITHIN: Duration = Duration::from_secs(15 * 60);

/// An agent's call as the window takes it, to answer through `reply`. Dropping `reply`
/// unanswered tells the agent the window couldn't take it.
pub struct Incoming {
    pub repo: String,
    pub agent: String,
    pub op: WriteOp,
    pub reply: Sender<Reply>,
    /// Set once the agent stopped waiting (it cancelled the call, or ended): a request not
    /// started yet is dropped, its card taken away.
    pub gone: Arc<AtomicBool>,
}

/// Asks the running window to run `op` in `repo` for `agent`. `wait` bounds how long the answer
/// may take; `cancelled` stops the wait.
pub fn ask(
    dir: &Path,
    repo: &str,
    agent: &str,
    op: &WriteOp,
    wait: Duration,
    cancelled: &dyn Fn() -> bool,
) -> Reply {
    let call = AgentCall {
        repo: repo.to_owned(),
        agent: agent.to_owned(),
        op: op.clone(),
    };
    instance::call_agent(dir, &call, wait, cancelled)
}

/// A window for tests to talk to: the running instance in a folder, handing every agent's call
/// to a function as the window would take it. Dropped, it is no longer running.
pub struct Listener {
    primary: Arc<Primary>,
    stop: Arc<AtomicBool>,
}

impl Drop for Listener {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        self.primary.shut_down();
    }
}

/// Becomes the running instance in `dir` and hands agents' calls to `deliver`; fails when
/// another process is the running instance there.
pub fn listen(dir: &Path, deliver: impl Fn(Incoming) + Send + 'static) -> Result<Listener, String> {
    let primary = Arc::new(
        instance::primary_in(dir)
            .ok_or_else(|| format!("another gitcraft is running in {}", dir.display()))?,
    );
    let stop = Arc::new(AtomicBool::new(false));
    let (taking, stopping) = (Arc::clone(&primary), Arc::clone(&stop));
    std::thread::spawn(move || {
        while !stopping.load(Ordering::SeqCst) {
            for incoming in taking.take_agent_calls() {
                deliver(incoming);
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    });
    Ok(Listener { primary, stop })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcp::wire::Outcome;

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "gitcraft-link-{name}-{}-{}",
            std::process::id(),
            crate::remote::askpass::random_token()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    fn stage() -> WriteOp {
        WriteOp::Stage {
            paths: vec!["a.txt".into()],
        }
    }

    const WAIT: Duration = Duration::from_secs(5);

    #[test]
    fn a_call_reaches_the_window_and_its_answer_comes_back() {
        let dir = temp("answer");
        let listener = listen(&dir, |incoming: Incoming| {
            let message = format!("{} {} {:?}", incoming.agent, incoming.repo, incoming.op);
            let _ = incoming.reply.send(Reply::new(Outcome::Done, message));
        });
        assert!(listener.is_ok());
        let reply = ask(&dir, "/r", "Claude Code", &stage(), WAIT, &|| false);
        drop(listener);
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(reply.outcome, Outcome::Done);
        assert!(reply.message.starts_with("Claude Code /r Stage"));
    }

    #[test]
    fn without_a_window_the_agent_is_told_to_start_one() {
        let dir = temp("none");
        let reply = ask(&dir, "/r", "a", &stage(), WAIT, &|| false);
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(reply.outcome, Outcome::NotOpen);
        assert!(reply.message.contains("isn't running"));
    }

    #[test]
    fn a_window_gone_since_isnt_believed() {
        let dir = temp("gone");
        let listener = listen(&dir, |_| {});
        drop(listener);
        // Its file may be left behind (a crash): with nobody holding the lock, it isn't read.
        let reply = ask(&dir, "/r", "a", &stage(), WAIT, &|| false);
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(reply.outcome, Outcome::NotOpen);
    }

    #[test]
    fn a_cancelled_call_stops_waiting_and_the_window_sees_it_gone() {
        let dir = temp("cancel");
        let held = Arc::new(std::sync::Mutex::new(Vec::<Incoming>::new()));
        let keep = Arc::clone(&held);
        let listener = listen(&dir, move |incoming| {
            if let Ok(mut held) = keep.lock() {
                held.push(incoming);
            }
        });
        let cancel = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&cancel);
        let asking_dir = dir.clone();
        let asking = std::thread::spawn(move || {
            ask(&asking_dir, "/r", "a", &stage(), WAIT, &|| {
                flag.load(Ordering::SeqCst)
            })
        });
        let started = std::time::Instant::now();
        while held.lock().map(|h| h.is_empty()).unwrap_or(true)
            && started.elapsed() < Duration::from_secs(5)
        {
            std::thread::sleep(Duration::from_millis(10));
        }
        cancel.store(true, Ordering::SeqCst);
        let reply = asking.join();
        let gone = || {
            held.lock()
                .map(|h| !h.is_empty() && h.iter().all(|i| i.gone.load(Ordering::SeqCst)))
                .unwrap_or(false)
        };
        let started = std::time::Instant::now();
        while !gone() && started.elapsed() < Duration::from_secs(5) {
            std::thread::sleep(Duration::from_millis(20));
        }
        let seen_gone = gone();
        drop(listener);
        let _ = std::fs::remove_dir_all(&dir);
        assert!(
            reply.is_ok_and(|r| r.outcome == Outcome::Failed && r.message.contains("cancelled"))
        );
        assert!(seen_gone, "the window learns the agent left");
    }

    #[test]
    fn a_request_too_big_is_refused_before_it_is_sent() {
        let dir = temp("big");
        let listener = listen(&dir, |_| {});
        let huge = WriteOp::Commit {
            message: "x".repeat(200 * 1024),
            amend: false,
        };
        let reply = ask(&dir, "/r", "a", &huge, WAIT, &|| false);
        drop(listener);
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(reply.outcome, Outcome::Failed);
        assert!(reply.message.contains("too big"));
    }
}
