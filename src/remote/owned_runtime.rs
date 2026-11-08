//! Small shared support for the three held, exact-filter owned-runtime fixtures.
//! No app initialization, global environment changes, or PID-based cleanup.
use crate::processes::{Options, Output, Service, Ticket};
use std::io::{self, Write};
use std::os::unix::fs::DirBuilderExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

const MODE: &str = "GITCRAFT_FINITE_FIXTURE_MODE";
const DIRECTORY: &str = "GITCRAFT_FINITE_FIXTURE_DIRECTORY";
const HELPER: &str = "remote::native_owner::tests::helper_entrypoint";
pub(super) const EFFECT_DELAY: Duration = Duration::from_secs(3);
pub(super) const LIMIT: Duration = Duration::from_secs(12);

pub(super) struct Scratch(pub PathBuf, bool);
impl Scratch {
    pub fn new() -> io::Result<Self> {
        let mut random = [0; 16];
        gitcraft_os::random_bytes(&mut random)?;
        let suffix: String = random.iter().map(|b| format!("{b:02x}")).collect();
        let path = std::env::temp_dir().join(format!("gitcraft-finite-fixture-{suffix}"));
        std::fs::DirBuilder::new().mode(0o700).create(&path)?;
        Ok(Self(path, false))
    }
    pub fn complete(&mut self) {
        self.1 = true;
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        // Only this exclusively created namespace. Services/owners drop before Scratch.
        if self.1 {
            let _ = std::fs::remove_dir_all(&self.0);
        } else {
            eprintln!("finite fixture evidence retained: {}", self.0.display());
        }
    }
}

pub(super) fn command(mode: &str, directory: &Path) -> io::Result<Command> {
    let mut command = Command::new(std::env::current_exe()?);
    command
        .args(["--exact", HELPER, "--nocapture", "--test-threads=1"])
        .env(MODE, mode)
        .env(DIRECTORY, directory)
        .env_remove("GITCRAFT_NATIVE_HELPER")
        .env_remove("GITCRAFT_CLAIM_HELPER");
    Ok(command)
}

pub(super) fn options() -> Options {
    Options {
        cancel: crate::tasks::Cancellation::default(),
        timeout: Some(LIMIT),
        output_limit: 8192,
        stderr_limit: Some(8192),
        ..Default::default()
    }
}

pub(super) fn stdout(payload: &[u8]) -> Vec<u8> {
    // Fixed libtest invocation above; the child exits before libtest prints its trailer.
    let mut bytes = format!("\nrunning 1 test\ntest {HELPER} ... ").into_bytes();
    bytes.extend_from_slice(payload);
    bytes
}

pub(super) fn until(mut ready: impl FnMut() -> Result<bool, String>) -> Result<(), String> {
    let deadline = Instant::now() + LIMIT;
    while !ready()? {
        if Instant::now() >= deadline {
            return Err("finite fixture barrier expired".into());
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    Ok(())
}

pub(super) fn wait(ticket: &Ticket) -> Result<Output, String> {
    let mut result = None;
    until(|| {
        result = ticket.try_result();
        Ok(result.is_some())
    })?;
    result.ok_or("missing finite fixture result")?
}

pub(super) fn canary(service: &Service, directory: &Path) -> Result<Ticket, String> {
    service.submit(
        command("canary", directory).map_err(|e| e.to_string())?,
        Options {
            timeout: Some(Duration::from_secs(60)),
            ..options()
        },
    )
}

pub(super) fn challenge(ticket: &Ticket, directory: &Path, value: &[u8]) -> Result<(), String> {
    std::fs::write(directory.join("ping"), value).map_err(|e| e.to_string())?;
    until(|| {
        if ticket.try_result().is_some() {
            return Err("independent owned canary exited".into());
        }
        Ok(std::fs::read(directory.join("pong")).is_ok_and(|bytes| bytes == value))
    })
}

// The leaf must inherit the target group. A second Service launch would create another
// group. Its direct Child stays in this target-local RAII guard; the outer helper Service
// retains the group's only anchor. No signal target is reconstructed from a number.
struct Leaf(Child);
impl Drop for Leaf {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// Called by the existing exact helper dispatcher, never a standalone validating test.
pub(super) fn dispatch() -> Result<bool, String> {
    let Some(mode) = std::env::var_os(MODE) else {
        return Ok(false);
    };
    let directory = PathBuf::from(std::env::var_os(DIRECTORY).ok_or("fixture directory missing")?);
    let run = || -> io::Result<()> {
        match mode.to_str() {
            Some("exit") => {
                io::stdout().write_all(b"finite-stdout\0\xff\n")?;
                io::stderr().write_all(b"finite-stderr\0\xfe\n")?;
                io::stdout().flush()?;
                io::stderr().flush()?;
                std::process::exit(7);
            }
            Some("leaf") => {
                std::fs::write(directory.join("leaf-ready"), b"ready")?;
                std::thread::sleep(EFFECT_DELAY);
                std::fs::write(directory.join("unexpected-effect"), b"escaped")?;
            }
            Some("leader") => {
                let child = command("leaf", &directory)?
                    .stdin(Stdio::null())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .spawn()?;
                let _leaf = Leaf(child);
                until(|| Ok(directory.join("leaf-ready").exists())).map_err(io::Error::other)?;
                std::fs::write(directory.join("target-ready"), b"ready")?;
                until(|| Ok(directory.join("release").exists())).map_err(io::Error::other)?;
            }
            Some("gate") => {
                std::fs::write(directory.join("target-ready"), b"ready")?;
                until(|| Ok(directory.join("release").exists())).map_err(io::Error::other)?;
                std::fs::write(directory.join("mutation-effect"), b"owned mutation")?;
                io::stdout().write_all(b"mutation-complete\n")?;
                io::stdout().flush()?;
            }
            Some("touch") => {
                std::fs::write(directory.join("unexpected-effect"), b"stale dispatch")?;
            }
            Some("canary") => {
                let deadline = Instant::now() + Duration::from_secs(60);
                while Instant::now() < deadline {
                    if let Ok(bytes) = std::fs::read(directory.join("ping")) {
                        std::fs::write(directory.join("pong"), bytes)?;
                    }
                    std::thread::sleep(Duration::from_millis(5));
                }
            }
            _ => return Err(io::Error::other("unknown finite fixture mode")),
        }
        Ok(())
    };
    run().map_err(|e| e.to_string())?;
    // Bypass libtest's output decoration, so the caller asserts exact target bytes.
    std::process::exit(0);
}
