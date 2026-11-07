//! Install the build-time packaged helper in a fresh private directory. Its binary is
//! bundled for the target; no compiler/interpreter is required on the user's machine.
use std::ffi::CString;
use std::fs::OpenOptions;
use std::io::{self, Read, Write};
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::os::unix::process::{CommandExt, ExitStatusExt};
use std::path::PathBuf;
use std::process::{Command, ExitStatus};
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

pub(super) struct Status {
    reader: std::io::PipeReader,
    bytes: Vec<u8>,
    terminal: Option<ExitStatus>,
}
impl Status {
    pub(super) fn observe(&mut self) -> io::Result<Option<ExitStatus>> {
        if self.terminal.is_some() {
            return Ok(self.terminal);
        }
        let mut bytes = [0u8; 64];
        match self.reader.read(&mut bytes) {
            Ok(0) => {
                return Err(io::Error::other(
                    "group anchor lost before command status; cleanup unknown",
                ));
            }
            Ok(n) => self.bytes.extend_from_slice(&bytes[..n]),
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => return Ok(None),
            Err(e) => return Err(e),
        }
        if self.bytes.len() > 64 {
            return Err(io::Error::other("invalid group anchor status"));
        }
        if self.bytes.contains(&b'\n') {
            self.terminal = Some(parse_status(&self.bytes)?);
        }
        Ok(self.terminal)
    }
}
fn parse_status(bytes: &[u8]) -> io::Result<ExitStatus> {
    let text = std::str::from_utf8(bytes).map_err(io::Error::other)?;
    if !text.ends_with('\n') || text.bytes().filter(|&b| b == b'\n').count() != 1 {
        return Err(io::Error::other("invalid group anchor status"));
    }
    let mut fields = text.split_ascii_whitespace();
    let kind = fields.next();
    let code: i32 = fields
        .next()
        .ok_or_else(|| io::Error::other("missing anchor status"))?
        .parse()
        .map_err(io::Error::other)?;
    if fields.next().is_some() {
        return Err(io::Error::other("extra anchor status fields"));
    }
    match kind {
        Some("S") if (0..=65535).contains(&code) => Ok(ExitStatus::from_raw(code)),
        Some("E") => Err(io::Error::from_raw_os_error(code)),
        _ => Err(io::Error::other("invalid group anchor status")),
    }
}

fn arguments(path: &CString, fd: i32, command: &Command) -> io::Result<Vec<CString>> {
    let mut arguments = vec![
        path.clone(),
        CString::new(fd.to_string()).map_err(io::Error::other)?,
        CString::new(command.get_program().as_bytes()).map_err(io::Error::other)?,
    ];
    for arg in command.get_args() {
        arguments.push(CString::new(arg.as_bytes()).map_err(io::Error::other)?);
    }
    Ok(arguments)
}

pub(super) fn prepare(command: &mut Command) -> io::Result<Status> {
    let path = CString::new(helper()?.as_os_str().as_bytes()).map_err(io::Error::other)?;
    let (reader, original_writer) = std::io::pipe()?;
    // Reserve a descriptor outside stdio even in GUI processes with closed fd 0/1/2.
    // SAFETY: duplicate a valid owned descriptor with close-on-exec; adopt exactly once.
    let fd = unsafe { libc::fcntl(original_writer.as_raw_fd(), libc::F_DUPFD_CLOEXEC, 3) };
    if fd < 0 {
        return Err(io::Error::last_os_error());
    }
    let writer = unsafe { OwnedFd::from_raw_fd(fd) };
    drop(original_writer);
    if unsafe { libc::fcntl(reader.as_raw_fd(), libc::F_SETFL, libc::O_NONBLOCK) } < 0 {
        return Err(io::Error::last_os_error());
    }
    let arguments = arguments(&path, writer.as_raw_fd(), command)?;
    // These allocations stay alive in pre_exec. Only their already-allocated pointers
    // are passed to execv; no environment reconstruction or allocation occurs after fork.
    let mut pointers: Vec<usize> = arguments.iter().map(|s| s.as_ptr() as usize).collect();
    pointers.push(0);
    // SAFETY: after fork this closure uses only async-signal-safe fcntl/execv and error
    // capture. All allocations/argument preparation happen above. std has already applied
    // cwd, env (including env_clear), stdio and process_group; execv preserves them exactly.
    unsafe {
        command.pre_exec(move || {
            let _keep_arguments_alive = &arguments;
            if libc::fcntl(writer.as_raw_fd(), libc::F_SETFD, 0) < 0 {
                return Err(io::Error::last_os_error());
            }
            libc::execv(path.as_ptr(), pointers.as_ptr().cast());
            Err(io::Error::last_os_error())
        });
    }
    Ok(Status {
        reader,
        bytes: Vec::new(),
        terminal: None,
    })
}

#[cfg(test)]
mod recording_tests {
    use super::*;
    #[test]
    fn helper_arguments_preserve_each_word_without_shell_interpretation() -> io::Result<()> {
        let mut command = Command::new("git path ü");
        command.args(["a b", "'quoted'", "$(no-command)", "", "line\nnext"]);
        let result = arguments(
            &CString::new("/private/helper").map_err(io::Error::other)?,
            42,
            &command,
        )?;
        let words: Vec<_> = result
            .iter()
            .map(|s| s.to_string_lossy().into_owned())
            .collect();
        assert_eq!(
            words,
            [
                "/private/helper",
                "42",
                "git path ü",
                "a b",
                "'quoted'",
                "$(no-command)",
                "",
                "line\nnext"
            ]
        );
        Ok(())
    }
    #[test]
    fn only_complete_single_status_frames_are_accepted() -> io::Result<()> {
        assert_eq!(parse_status(b"S 1792\n")?.code(), Some(7));
        for invalid in [
            b"S 1".as_slice(),
            b"S 1 extra\n",
            b"S 1\nS 2\n",
            b"S -1\n",
            b"S 65536\n",
            b"P 123\n",
        ] {
            assert!(parse_status(invalid).is_err());
        }
        assert!(parse_status(b"E 2\n").is_err());
        Ok(())
    }
}
