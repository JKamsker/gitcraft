//! Install the build-time packaged helper in a fresh private directory. Its binary is
//! bundled for the target; no compiler/interpreter is required on the user's machine.
use std::fs::OpenOptions;
use std::io::{self, Read, Write};
use std::os::fd::AsRawFd;
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::Command;
use std::sync::OnceLock;

const IMAGE: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/gitcraft-group-anchor"));
static HELPER: OnceLock<Result<PathBuf, String>> = OnceLock::new();

fn helper() -> io::Result<&'static PathBuf> {
    HELPER
        .get_or_init(|| install().map_err(|e| e.to_string()))
        .as_ref()
        .map_err(|e| io::Error::other(e.clone()))
}

pub(super) fn retire() {
    if let Some(Ok(path)) = HELPER.get() {
        // This exact private artifact was installed by this process. No process selectors,
        // directory scans or guessed paths are used. Call only after all owners shut down.
        let _ = std::fs::remove_file(path);
        if let Some(parent) = path.parent() {
            let _ = std::fs::remove_dir(parent);
        }
    }
}
fn install() -> io::Result<PathBuf> {
    let mut random = [0u8; 16];
    // SAFETY: writable initialized buffer and its actual size; getentropy is available on
    // supported Linux/macOS targets. No process identifiers grant helper-file authority.
    if unsafe { libc::getentropy(random.as_mut_ptr().cast(), random.len()) } != 0 {
        return Err(io::Error::last_os_error());
    }
    let suffix: String = random.iter().map(|b| format!("{b:02x}")).collect();
    let dir = std::env::temp_dir().join(format!("gitcraft-anchor-{suffix}"));
    std::fs::DirBuilder::new().mode(0o700).create(&dir)?;
    let path = dir.join("owner");
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o700)
        .custom_flags(libc::O_NOFOLLOW)
        .open(&path)?;
    file.write_all(IMAGE)?;
    file.sync_all()?;
    Ok(path)
}

/// A separate retained group leader. The original target Command is never rebuilt or
/// intercepted: std applies its own full launch semantics, including env_clear and PATH.
pub(super) struct Anchor {
    child: Option<std::process::Child>,
}
impl Anchor {
    fn start() -> io::Result<Self> {
        let mut command = Command::new(helper()?);
        command
            .env_clear()
            .current_dir("/")
            .process_group(0)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null());
        let mut anchor = Self {
            child: Some(command.spawn()?),
        };
        if let Err(primary) = anchor.ready() {
            return Err(with_cleanup(primary, anchor.retire()));
        }
        Ok(anchor)
    }
    fn ready(&mut self) -> io::Result<()> {
        let mut pipe = self
            .child
            .as_mut()
            .and_then(|c| c.stdout.take())
            .ok_or_else(|| io::Error::other("missing anchor readiness pipe"))?;
        // SAFETY: this newly owned pipe descriptor remains alive until readiness finishes.
        let flags = unsafe { libc::fcntl(pipe.as_raw_fd(), libc::F_GETFL) };
        if flags < 0
            || unsafe { libc::fcntl(pipe.as_raw_fd(), libc::F_SETFL, flags | libc::O_NONBLOCK) } < 0
        {
            return Err(io::Error::last_os_error());
        }
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        let mut bytes = Vec::new();
        loop {
            // A startup failure is reaped and permanently loses signal authority before
            // returning. No target can start from this failed anchor.
            self.confirm_running()?;
            let mut part = [0u8; 16];
            match pipe.read(&mut part) {
                Ok(0) => {
                    if readiness(&bytes, true)? {
                        return self.confirm_running();
                    }
                }
                Ok(n) => {
                    bytes.extend_from_slice(&part[..n]);
                    readiness(&bytes, false)?;
                }
                Err(e)
                    if e.kind() == io::ErrorKind::WouldBlock
                        || e.kind() == io::ErrorKind::Interrupted => {}
                Err(e) => return Err(e),
            }
            if std::time::Instant::now() >= deadline {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "group anchor readiness timed out",
                ));
            }
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
    }
    fn confirm_running(&mut self) -> io::Result<()> {
        let result = self
            .child
            .as_mut()
            .ok_or_else(|| io::Error::other("anchor retired"))?
            .try_wait();
        match result {
            Ok(None) => Ok(()),
            Ok(Some(status)) => {
                self.child = None;
                Err(io::Error::other(format!(
                    "group anchor exited during startup: {status}"
                )))
            }
            // Once wait ownership is uncertain, numeric signaling is forbidden.
            Err(e) => {
                self.child = None;
                Err(io::Error::other(format!(
                    "group anchor wait ownership could not be confirmed: {e}"
                )))
            }
        }
    }
    fn join(&mut self, command: &mut Command) -> io::Result<()> {
        self.confirm_running()?;
        let child = self
            .child
            .as_ref()
            .ok_or_else(|| io::Error::other("anchor retired"))?;
        command.process_group(owned_group(child.id())?);
        Ok(())
    }
    pub(super) fn stop(&self) -> io::Result<()> {
        let child = self
            .child
            .as_ref()
            .ok_or_else(|| io::Error::other("anchor retired"))?;
        let group = owned_group(child.id())?;
        // SAFETY: this exact private Child established its group and has not been reaped.
        // The target joins that group only while this owner retains the anchor lifetime.
        if unsafe { libc::killpg(group, libc::SIGKILL) } != 0 {
            let error = io::Error::last_os_error();
            if error.raw_os_error() != Some(libc::ESRCH) {
                return Err(error);
            }
        }
        Ok(())
    }
    fn retire(&mut self) -> io::Result<()> {
        if self.child.is_none() {
            return Ok(());
        }
        let stopped = self.stop();
        let reaped = self.reap();
        match stopped {
            Ok(()) => reaped,
            Err(primary) => Err(with_cleanup(primary, reaped)),
        }
    }
    pub(super) fn reap(&mut self) -> io::Result<()> {
        let Some(mut child) = self.child.take() else {
            return Ok(());
        };
        // Direct-child fallback bounds a refused group signal. The Child is consumed
        // before waiting, so neither Drop nor a repeated finish can signal after reap.
        let _ = child.kill();
        child.wait().map(|_| ())
    }
}
impl Drop for Anchor {
    fn drop(&mut self) {
        let _ = self.retire();
    }
}
fn with_cleanup(primary: io::Error, cleanup: io::Result<()>) -> io::Error {
    match cleanup {
        Ok(()) => primary,
        Err(cleanup) => io::Error::other(format!("{primary}; owned anchor cleanup: {cleanup}")),
    }
}

fn owned_group(id: u32) -> io::Result<i32> {
    let id = i32::try_from(id).map_err(io::Error::other)?;
    if id <= 1 {
        Err(io::Error::other("invalid owned anchor"))
    } else {
        Ok(id)
    }
}
fn readiness(bytes: &[u8], eof: bool) -> io::Result<bool> {
    if !b"R1\n".starts_with(bytes) || (eof && bytes != b"R1\n") {
        return Err(io::Error::other("invalid group anchor readiness"));
    }
    Ok(eof && bytes == b"R1\n")
}
trait Launch {
    type Anchor;
    type Target;
    fn anchor(&mut self) -> io::Result<Self::Anchor>;
    fn target(
        &mut self,
        command: &mut Command,
        anchor: &mut Self::Anchor,
    ) -> io::Result<Self::Target>;
    fn retire(&mut self, anchor: &mut Self::Anchor) -> io::Result<()>;
}
fn launch_with<B: Launch>(
    command: &mut Command,
    backend: &mut B,
) -> io::Result<(B::Target, B::Anchor)> {
    let mut anchor = backend.anchor()?;
    match backend.target(command, &mut anchor) {
        Ok(target) => Ok((target, anchor)),
        Err(primary) => Err(with_cleanup(primary, backend.retire(&mut anchor))),
    }
}
struct Native;
impl Launch for Native {
    type Anchor = Anchor;
    type Target = std::process::Child;
    fn retire(&mut self, anchor: &mut Anchor) -> io::Result<()> {
        anchor.retire()
    }
    fn anchor(&mut self) -> io::Result<Anchor> {
        Anchor::start()
    }
    fn target(&mut self, command: &mut Command, anchor: &mut Anchor) -> io::Result<Self::Target> {
        anchor.join(command)?;
        // This is the original opaque Command. There is no pre_exec interposition,
        // environment overlay/reconstruction, argument rebuilding or manual PATH lookup.
        command.spawn()
    }
}
pub(super) fn launch(command: &mut Command) -> io::Result<(std::process::Child, Anchor)> {
    launch_with(command, &mut Native)
}

#[cfg(test)]
mod recording_tests {
    use super::*;
    use std::ffi::{OsStr, OsString};
    use std::os::unix::ffi::{OsStrExt, OsStringExt};
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };
    struct Recording {
        original: *mut Command,
        description: String,
        environment: Vec<(OsString, Option<OsString>)>,
        events: Vec<&'static str>,
        anchor_failure: bool,
        target_failure: bool,
        cleanup_failure: bool,
    }
    impl Recording {
        fn new(command: &mut Command) -> Self {
            Self {
                original: command,
                description: format!("{command:?}"),
                environment: command
                    .get_envs()
                    .map(|(k, v)| (k.to_owned(), v.map(OsStr::to_owned)))
                    .collect(),
                events: Vec::new(),
                anchor_failure: false,
                target_failure: false,
                cleanup_failure: false,
            }
        }
    }
    impl Launch for Recording {
        type Anchor = ();
        type Target = ();
        fn anchor(&mut self) -> io::Result<()> {
            self.events.push("owned anchor startup");
            if self.anchor_failure {
                return Err(io::Error::other("readiness refused"));
            }
            self.events.push("ready anchor retained unreaped");
            Ok(())
        }
        fn target(&mut self, command: &mut Command, _: &mut ()) -> io::Result<()> {
            assert!(std::ptr::eq(self.original, command));
            // This compares the original opaque object, not an inferred env_clear flag.
            assert_eq!(format!("{command:?}"), self.description);
            assert_eq!(
                command
                    .get_envs()
                    .map(|(k, v)| (k.to_owned(), v.map(OsStr::to_owned)))
                    .collect::<Vec<_>>(),
                self.environment
            );
            self.events
                .push("original Command reaches normal std spawn");
            if self.target_failure {
                Err(io::Error::other("target pre_exec/exec failed"))
            } else {
                Ok(())
            }
        }
        fn retire(&mut self, _: &mut ()) -> io::Result<()> {
            self.events.push("signal only retained anchor group");
            self.events.push("reap owned anchor");
            if self.cleanup_failure {
                Err(io::Error::other("anchor cleanup unconfirmed"))
            } else {
                Ok(())
            }
        }
    }
    #[test]
    fn opaque_environment_matrix_reaches_normal_std_spawn_without_reconstruction() -> io::Result<()>
    {
        let inherit = Command::new("synthetic-program");
        let mut set = Command::new("synthetic-program");
        set.env("SYNTHETIC_SET", "new value");
        let mut remove = Command::new("synthetic-program");
        remove.env_remove("SYNTHETIC_REMOVE");
        let mut clear = Command::new("synthetic-program");
        clear.env_clear().env("SYNTHETIC_ONLY", "only value");
        let mut clear_empty = Command::new("synthetic-program");
        clear_empty.env_clear();
        // Both have no explicit entries; production must never infer clear from that.
        assert_eq!(inherit.get_envs().count(), 0);
        assert_eq!(clear_empty.get_envs().count(), 0);
        assert_ne!(format!("{inherit:?}"), format!("{clear_empty:?}"));
        let mut binary = Command::new("synthetic-program");
        binary.env(
            "SYNTHETIC_BINARY",
            OsString::from_vec(vec![b'a', 255, b'b']),
        );
        assert_eq!(
            binary.get_envs().next().unwrap().1.unwrap().as_bytes(),
            &[b'a', 255, b'b']
        );
        let mut path = Command::new("relative-program");
        path.env("PATH", "/synthetic/first:/synthetic/second");
        let mut removed_path = Command::new("relative-program");
        removed_path.env_remove("PATH");
        let mut clear_path = Command::new("relative-program");
        clear_path.env_clear().env("PATH", "/synthetic/only");
        let callback = Arc::new(AtomicBool::new(false));
        let seen = callback.clone();
        let mut words = Command::new(OsString::from_vec(b"program-\xff".to_vec()));
        words
            .arg0(OsString::from_vec(b"argv0-\xfe".to_vec()))
            .args(["a b", "'quoted'", "$(data)", "", "line\nnext"])
            .current_dir("/synthetic/cwd");
        // SAFETY: recording never forks or invokes this callback. A real std launch owns
        // the callback and its ordinary documented pre_exec contract, unchanged.
        unsafe {
            words.pre_exec(move || {
                seen.store(true, Ordering::SeqCst);
                Ok(())
            });
        }
        for mut command in [
            inherit,
            set,
            remove,
            clear,
            clear_empty,
            binary,
            path,
            removed_path,
            clear_path,
            words,
        ] {
            let mut backend = Recording::new(&mut command);
            launch_with(&mut command, &mut backend)?;
            assert_eq!(
                backend.events,
                [
                    "owned anchor startup",
                    "ready anchor retained unreaped",
                    "original Command reaches normal std spawn"
                ]
            );
        }
        assert!(!callback.load(Ordering::SeqCst));
        Ok(())
    }
    #[test]
    fn anchor_failure_prevents_target_and_target_failure_preserves_cleanup_failure() {
        let mut command = Command::new("synthetic-program");
        let mut backend = Recording::new(&mut command);
        backend.anchor_failure = true;
        assert!(launch_with(&mut command, &mut backend).is_err());
        assert_eq!(backend.events, ["owned anchor startup"]);
        for refused in [false, true] {
            let mut backend = Recording::new(&mut command);
            backend.target_failure = true;
            backend.cleanup_failure = refused;
            let error = launch_with(&mut command, &mut backend)
                .unwrap_err()
                .to_string();
            assert!(error.contains("target pre_exec/exec failed"));
            assert_eq!(error.contains("anchor cleanup unconfirmed"), refused);
            assert_eq!(
                &backend.events[3..],
                ["signal only retained anchor group", "reap owned anchor"]
            );
        }
    }
    #[test]
    fn readiness_requires_exact_complete_frame_and_eof() -> io::Result<()> {
        assert!(!readiness(b"", false)?);
        assert!(!readiness(b"R", false)?);
        assert!(!readiness(b"R1\n", false)?);
        assert!(readiness(b"R1\n", true)?);
        for invalid in [b"".as_slice(), b"R", b"R1\nextra", b"R1\nR1\n", b"wrong"] {
            assert!(readiness(invalid, true).is_err());
        }
        assert!(owned_group(0).is_err());
        assert!(owned_group(1).is_err());
        assert_eq!(owned_group(42)?, 42);
        Ok(())
    }
}
