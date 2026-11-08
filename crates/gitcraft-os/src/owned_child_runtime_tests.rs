//! Actual owned-child fixtures. Compile separately from the recording-only gates.
use super::*;
use std::io::{BufRead, Read, Write};
use std::os::fd::AsRawFd;
use std::os::unix::fs::DirBuilderExt;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::{Duration, Instant};

const MODE: &str = "GITCRAFT_ANCHOR_FIXTURE_MODE";
const CHILD_TEST: &str = "owned_child::owned_runtime_tests::fixture_child";
const EFFECT_DELAY: Duration = Duration::from_secs(5);

struct Scratch(PathBuf);
impl Scratch {
    fn new() -> io::Result<Self> {
        let mut random = [0u8; 16];
        // SAFETY: writable buffer of the specified size; no PID is an authority.
        if unsafe { libc::getentropy(random.as_mut_ptr().cast(), random.len()) } != 0 {
            return Err(io::Error::last_os_error());
        }
        let suffix: String = random.iter().map(|b| format!("{b:02x}")).collect();
        let path = std::env::temp_dir().join(format!("gitcraft-anchor-fixture-{suffix}"));
        std::fs::DirBuilder::new().mode(0o700).create(&path)?;
        Ok(Self(path))
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        // Every process owner is declared after Scratch and is dropped first.
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn child_command(mode: &str, directory: &Path) -> io::Result<Command> {
    let mut command = Command::new(std::env::current_exe()?);
    command
        .args(["--exact", CHILD_TEST, "--nocapture", "--test-threads=1"])
        .env(MODE, mode)
        .env("GITCRAFT_ANCHOR_FIXTURE_DIRECTORY", directory)
        .env_remove("GITCRAFT_NATIVE_HELPER")
        .env_remove("GITCRAFT_CLAIM_HELPER")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    Ok(command)
}

fn wait_until(mut ready: impl FnMut() -> io::Result<bool>) -> io::Result<()> {
    let deadline = Instant::now() + Duration::from_secs(3);
    while !ready()? {
        if Instant::now() >= deadline {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "owned fixture barrier",
            ));
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    Ok(())
}

/// Invoked only by the exact current test executable, never a shell or PID selector.
#[test]
fn fixture_child() -> io::Result<()> {
    let Ok(mode) = std::env::var(MODE) else {
        return Ok(());
    };
    let directory = PathBuf::from(
        std::env::var_os("GITCRAFT_ANCHOR_FIXTURE_DIRECTORY")
            .ok_or_else(|| io::Error::other("missing owned fixture directory"))?,
    );
    match mode.as_str() {
        "canary" => {
            for line in io::stdin().lock().lines() {
                let line = line?;
                if !matches!(line.as_str(), "before" | "after-finish" | "after-drop") {
                    return Err(io::Error::other("unexpected canary request"));
                }
                writeln!(io::stdout().lock(), "\nCANARY-ACK:{line}")?;
                io::stdout().flush()?;
            }
        }
        "descendant" => {
            std::fs::write(directory.join("ready"), b"ready")?;
            std::thread::sleep(EFFECT_DELAY);
            std::fs::write(directory.join("unexpected-effect"), b"escaped")?;
        }
        "leader" => {
            // This child deliberately inherits the leader's owned group. Dropping this
            // diagnostic Child does not detach it: the outer OwnedChild retains the
            // live group anchor and is the sole cleanup authority even on test failure.
            let descendant = child_command("descendant", &directory)?.spawn()?;
            wait_until(|| directory.join("ready").try_exists())?;
            drop(descendant);
            std::process::exit(7);
        }
        _ => return Err(io::Error::other("unknown owned fixture mode")),
    }
    Ok(())
}

fn canary_ack(input: &mut ChildStdin, output: &mut ChildStdout, request: &str) -> io::Result<()> {
    writeln!(input, "{request}")?;
    input.flush()?;
    let marker = format!("\nCANARY-ACK:{request}\n");
    let mut received = Vec::new();
    wait_until(|| {
        let mut bytes = [0u8; 1024];
        match output.read(&mut bytes) {
            Ok(0) => return Err(io::Error::other("canary exited")),
            Ok(count) => received.extend_from_slice(&bytes[..count]),
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {}
            Err(error) => return Err(error),
        }
        if received.len() > 65536 {
            return Err(io::Error::other("unexpected canary output"));
        }
        Ok(received
            .windows(marker.len())
            .any(|part| part == marker.as_bytes()))
    })
}

#[test]
fn darwin_live_anchor_and_canary() -> io::Result<()> {
    let scratch = Scratch::new()?;
    let mut command = child_command("canary", &scratch.0)?;
    command.stdin(Stdio::piped()).stdout(Stdio::piped());
    let mut canary = OwnedChild::spawn(command)?;
    let mut input = canary
        .stdin()
        .ok_or_else(|| io::Error::other("canary stdin"))?;
    let mut output = canary
        .stdout()
        .ok_or_else(|| io::Error::other("canary stdout"))?;
    // SAFETY: this valid retained pipe belongs only to this fixture. Nonblocking reads
    // make a failed canary acknowledgement bounded without another cleanup thread.
    let flags = unsafe { libc::fcntl(output.as_raw_fd(), libc::F_GETFL) };
    if flags < 0
        || unsafe { libc::fcntl(output.as_raw_fd(), libc::F_SETFL, flags | libc::O_NONBLOCK) } < 0
    {
        return Err(io::Error::last_os_error());
    }
    canary_ack(&mut input, &mut output, "before")?;
    for (name, explicit_finish) in [("finish", true), ("drop", false)] {
        let directory = scratch.0.join(name);
        std::fs::create_dir(&directory)?;
        let mut target = OwnedChild::spawn(child_command("leader", &directory)?)?;
        wait_until(|| directory.join("ready").try_exists())?;
        let effect_deadline = Instant::now() + EFFECT_DELAY + Duration::from_millis(200);
        let mut target_status = None;
        wait_until(|| {
            target_status = target.observe()?;
            Ok(target_status.is_some())
        })?;
        assert_eq!(target_status.and_then(|status| status.code()), Some(7));
        // Observe consumed the user status while retaining the separate anchor. Never
        // independently wait/reap it here; production finish remains its only waiter.
        assert!(target.terminal.is_none());
        assert!(
            target
                .process
                .as_ref()
                .is_some_and(|process| process.anchor.is_some())
        );
        if explicit_finish {
            let first = target.finish()?; // EPERM is a failure, never excused on Darwin.
            assert_eq!(target.finish()?, first);
        }
        drop(target); // Second variant exercises an early-return/Drop cleanup boundary.
        canary_ack(
            &mut input,
            &mut output,
            if explicit_finish {
                "after-finish"
            } else {
                "after-drop"
            },
        )?;
        std::thread::sleep(effect_deadline.saturating_duration_since(Instant::now()));
        assert!(!directory.join("unexpected-effect").try_exists()?);
        assert!(canary.observe()?.is_none());
    }
    canary.finish()?;
    Ok(())
}
