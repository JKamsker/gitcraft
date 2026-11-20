//! The owned child of the kill tests (`kill.rs`): the same test binary, re-executed with
//! one boundary to abort at. The parent spawns it, waits for that one handle only and reads
//! its log; nothing is looked up by PID and no signal is sent to anything else.
use super::recording_kill::act;
use super::*;
use std::io::Write as _;
use std::process::{Command, ExitStatus, Stdio};
use std::time::{Duration, Instant};

/// `<action>\n<boundary number>\n<fixture>`: set only by [`kill`] for its own child.
const CHILD_ENV: &str = "GITCRAFT_RECOVERY_KILL";
pub(super) const MARKER: &str = "recovery-kill: aborting at boundary ";
const CHILD_DEADLINE: Duration = Duration::from_secs(120);
/// The child: runs `action` on the parent's fixture and aborts at the boundary it was given.
#[test]
#[ignore = "child of the kill/restart tests: only their owned parent process runs it"]
fn child() {
    let Ok(spec) = std::env::var(CHILD_ENV) else {
        return;
    };
    let [action, stop, repo]: [&str; 3] = spec.splitn(3, '\n').collect::<Vec<_>>()[..]
        .try_into()
        .unwrap();
    let stop: usize = stop.parse().unwrap();
    let repo = PathBuf::from(repo);
    let parked = (action != "park").then(|| list(&repo).unwrap().remove(0));
    let mut seen = 0;
    effects::HOOK.with(|h| {
        *h.borrow_mut() = Some(Box::new(move |label| {
            seen += 1;
            if seen == stop {
                // Not libtest's captured print: straight to the parent's log file.
                let _ = writeln!(std::io::stderr(), "{MARKER}{stop}: {label}");
                std::process::abort();
            }
        }));
    });
    let result = act(action, &repo, parked.as_ref());
    let _ = writeln!(
        std::io::stderr(),
        "recovery-kill: {stop} not reached: {result:?}"
    );
}

#[cfg(unix)]
fn aborted(status: ExitStatus) -> bool {
    std::os::unix::process::ExitStatusExt::signal(&status) == Some(6)
}

/// std's abort is `__fastfail`: STATUS_STACK_BUFFER_OVERRUN, no handler or unwinding.
#[cfg(windows)]
fn aborted(status: ExitStatus) -> bool {
    status.code() == Some(0xC000_0409_u32 as i32)
}

/// Runs the child for `action` on `repo`, killed at boundary `stop`; returns its log.
pub(super) fn kill(action: &str, stop: usize, repo: &Path) -> String {
    let name = module_path!().split_once("::").unwrap().1.to_owned() + "::child";
    let mut log = repo.as_os_str().to_owned();
    log.push(".kill.log");
    let log = PathBuf::from(log);
    let out = std::fs::File::create(&log).unwrap();
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args([
            &name,
            "--exact",
            "--ignored",
            "--nocapture",
            "--test-threads=1",
        ])
        .env(CHILD_ENV, format!("{action}\n{stop}\n{}", repo.display()))
        .stdin(Stdio::null())
        .stdout(out.try_clone().unwrap())
        .stderr(out)
        .spawn()
        .unwrap();
    let deadline = Instant::now() + CHILD_DEADLINE;
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        if Instant::now() > deadline {
            // Only this owned handle is stopped; nothing else is looked up or signalled.
            let _ = child.kill();
            let _ = child.wait();
            panic!("kill child for {action}:{stop} exceeded {CHILD_DEADLINE:?}");
        }
        std::thread::sleep(Duration::from_millis(5));
    };
    let text = String::from_utf8_lossy(&std::fs::read(&log).unwrap()).into_owned();
    std::fs::remove_file(&log).unwrap();
    assert!(aborted(status), "{action}:{stop} ended {status:?}\n{text}");
    text
}
