//! SSH/WSL shell construction and successful-connection acknowledgement.
use super::*;

/// For the tests: the SSH host whose shells are `sh` on this machine, so that everything
/// sent to another machine (the framing, the input, the pools) runs as it does over SSH.
#[cfg(all(test, unix))]
pub const LOOPBACK: &str = "gitcraft-loopback";

/// A new shell kept open on `host`.
pub(in crate::remote) fn start_shell(host: &Host) -> Result<Shell, String> {
    let (cmd, attempt) = shell_command(host, true);
    let shell = Shell::start(cmd, &host.label())?;
    // Signed in: a password to remember is kept now (a failed start drops it).
    if let Some(attempt) = attempt {
        attempt.connected();
    }
    Ok(shell)
}

/// What a new shell is sent first: its temporary folder (removed when the shell ends, also on
/// a hang-up or a signal), its PATH, then [`READY`] and its process (see [`parse_ready`]).
pub(in crate::remote) fn setup_script() -> String {
    format!("{MORE_PATH}\n{}", super::super::supervisor::setup())
}

/// The process that runs `sh` on `host`, reading its script from its input, and (over SSH)
/// the attempt to tell once it has signed in.
pub(super) struct Connection {
    attempt: Option<super::super::askpass::Attempt>,
    _opening: Option<super::super::transport::Opening>,
}

impl Connection {
    pub(super) fn connected(self) {
        if let Some(attempt) = self.attempt {
            attempt.connected();
        }
    }
}

pub(super) fn shell_command(host: &Host, serialize: bool) -> (Command, Option<Connection>) {
    #[cfg(all(test, unix))]
    if *host == Host::Ssh(LOOPBACK.to_owned()) {
        return (Command::new("sh"), None);
    }
    let mut connection = None;
    // Changed further only on Windows (no console window).
    #[cfg_attr(not(windows), allow(unused_mut))]
    let mut cmd = match host {
        Host::Ssh(dest) => {
            let mut cmd = Command::new("ssh");
            // No terminal, no `~.` escapes: the stream is the protocol.
            cmd.args(["-T", "-e", "none"]);
            // Its questions (a password, a passphrase, a new host key) asked in the window
            // (see `askpass`); where that can't be, none asked: it fails, and says why.
            let mut attempt = None;
            match super::super::askpass::env_for(&host.label()) {
                Some((env, started)) => {
                    cmd.envs(env);
                    attempt = Some(started);
                }
                None => {
                    cmd.args(["-o", "BatchMode=yes"]);
                }
            }
            let opening = super::super::transport::configure(&mut cmd, dest, serialize);
            // The Unix wrapper includes the original configuration before its defaults.
            // Unsupported platforms / inaccessible private storage keep the old behavior.
            if opening.is_none()
                && let Some(config) =
                    std::env::var_os("GITCRAFT_SSH_CONFIG").filter(|c| !c.is_empty())
            {
                cmd.arg("-F").arg(config);
            }
            connection = Some(Connection {
                attempt,
                _opening: opening,
            });
            cmd.args([
                "-o",
                "ConnectTimeout=15",
                // A dropped network ends the shell (and its waiting command) within ~45 s.
                "-o",
                "ServerAliveInterval=15",
                "-o",
                "ServerAliveCountMax=3",
                "--",
            ]);
            cmd.arg(super::super::ssh_destination(dest));
            cmd.arg("sh");
            cmd
        }
        Host::Wsl(distro) => {
            let mut cmd = Command::new("wsl.exe");
            cmd.args(["-d", distro, "--exec", "sh"]);
            cmd
        }
    };
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    (cmd, connection)
}
