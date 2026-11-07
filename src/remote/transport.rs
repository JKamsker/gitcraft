//! OpenSSH transports shared by independent shells. Defaults are appended *after* the
//! user's configuration, so an explicit ControlMaster/ControlPath/ControlPersist wins.
//! Sockets belong to this process, never to another GitCraft instance or credential context.

#[cfg(unix)]
mod unix;

/// A changed SSH configuration/authentication context must not reuse an old pooled shell.
pub(super) fn context(host: &super::Host) -> String {
    #[cfg(unix)]
    if let super::Host::Ssh(dest) = host {
        return unix::context(dest);
    }
    let _ = host;
    String::new()
}

/// Hold until the new shell answers (or its start fails). Only channel establishment is
/// serialized; commands on established shells continue concurrently.
pub(super) struct Opening {
    #[cfg(unix)]
    _inner: unix::Configured,
}

pub(super) fn configure(
    command: &mut std::process::Command,
    dest: &str,
    serialize: bool,
) -> Option<Opening> {
    #[cfg(unix)]
    if let Some(opening) = unix::configure(command, dest, serialize) {
        return Some(Opening { _inner: opening });
    }
    let _ = (command, dest, serialize);
    None
}

pub(super) fn shutdown() {
    #[cfg(unix)]
    unix::shutdown();
}

/// Terminal command construction happens on the UI thread. Preserve the original SSH
/// transport there, including the custom config, without filesystem reads or capability probes.
pub(super) fn terminal_args(dest: &str) -> Vec<String> {
    let _ = dest;
    std::env::var("GITCRAFT_SSH_CONFIG")
        .ok()
        .filter(|p| !p.is_empty())
        .map(|p| vec!["-F".to_owned(), p])
        .unwrap_or_default()
}

/// Dynamic SSH Match conditions cannot be fingerprinted without reevaluating external
/// programs/network state, so each operation receives a fresh connection in that context.
pub(super) fn reusable(host: &super::Host) -> bool {
    #[cfg(unix)]
    if let super::Host::Ssh(dest) = host {
        return !unix::dynamic(dest);
    }
    let _ = host;
    true
}
