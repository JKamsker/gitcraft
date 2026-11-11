use std::collections::{BTreeMap, HashMap};
use std::ffi::OsString;
use std::fs::{DirBuilder, OpenOptions};
use std::io::Write;
use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Condvar, Mutex, OnceLock};
use std::time::Duration;

use sha2::{Digest, Sha256};

use super::super::shell::lock;

static MANAGER: OnceLock<Option<Manager>> = OnceLock::new();

struct Manager {
    dir: PathBuf,
    configs: Mutex<HashMap<String, Arc<Config>>>,
}

struct Config {
    path: PathBuf,
    opening: Mutex<bool>,
    freed: Condvar,
}

pub(super) struct Opening(Arc<Config>);
impl Drop for Opening {
    fn drop(&mut self) {
        *lock(&self.0.opening) = false;
        self.0.freed.notify_one();
    }
}

impl Config {
    fn take(self: &Arc<Self>) -> Option<Opening> {
        let cancel = crate::tasks::cancellation();
        let mut opening = lock(&self.opening);
        while *opening {
            if cancel.is_cancelled() {
                return None;
            }
            opening = self
                .freed
                .wait_timeout(opening, Duration::from_millis(50))
                .map(|(state, _)| state)
                .unwrap_or_else(|e| e.into_inner().0);
        }
        *opening = true;
        Some(Opening(Arc::clone(self)))
    }
}

impl Manager {
    fn new() -> std::io::Result<Self> {
        // A non-OpenSSH client (or an old version without multiplexing) retains the
        // original command line. This probe reads no user configuration and connects nowhere.
        let supported = Command::new("ssh")
            .args([
                "-G",
                "-F",
                "/dev/null",
                "-o",
                "ControlMaster=auto",
                "-o",
                "ControlPersist=300",
                "--",
                "localhost",
            ])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()?;
        if !supported.success() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::Unsupported,
                "SSH multiplexing is unavailable",
            ));
        }
        // /tmp deliberately keeps Unix socket names below sockaddr_un's short limit,
        // including on macOS where the user's TMPDIR can already consume most of it.
        let dir = Path::new("/tmp").join(format!(
            "gc-{}",
            &super::super::askpass::random_token()[..16]
        ));
        DirBuilder::new().mode(0o700).create(&dir)?;
        Ok(Self {
            dir,
            configs: Mutex::default(),
        })
    }

    fn config(&self, dest: &str) -> std::io::Result<Arc<Config>> {
        let (key, roots) = security_context(dest);
        if dynamic(dest) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::Unsupported,
                "dynamic SSH Match condition",
            ));
        }
        // -F treats Includes as user configuration. A relative system Include would
        // consequently resolve against ~/.ssh; retain native ssh config loading in that
        // uncommon case rather than silently changing the administrator's policy.
        if roots.iter().any(|p| p == Path::new("/etc/ssh/ssh_config"))
            && !portable_system_includes(Path::new("/etc/ssh/ssh_config"), 0)
        {
            return Err(std::io::Error::new(
                std::io::ErrorKind::Unsupported,
                "relative system SSH Include",
            ));
        }
        self.config_with(&key, &roots)
    }

    fn config_with(&self, key: &str, roots: &[PathBuf]) -> std::io::Result<Arc<Config>> {
        let mut configs = lock(&self.configs);
        if let Some(config) = configs.get(key) {
            return Ok(Arc::clone(config));
        }
        let path = self.dir.join(format!("{key}.conf"));
        let socket = self.dir.join(format!("{}-%C", &key[..key.len().min(16)]));
        let text = configuration(roots, &socket)?;
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .mode(0o600)
            .open(&path)?;
        file.write_all(text.as_bytes())?;
        let config = Arc::new(Config {
            path,
            opening: Mutex::new(false),
            freed: Condvar::new(),
        });
        configs.insert(key.to_owned(), Arc::clone(&config));
        Ok(config)
    }
}

fn configuration(roots: &[PathBuf], socket: &Path) -> std::io::Result<String> {
    let mut text = String::new();
    for root in roots {
        // -F names a literal file, while Include expands tokens/environment even in
        // quotes. Retain the native command line rather than redirecting that filename.
        let bytes = root.as_os_str().as_encoded_bytes();
        if bytes.contains(&b'%') || bytes.windows(2).any(|pair| pair == b"${") {
            return Err(std::io::Error::new(
                std::io::ErrorKind::Unsupported,
                "SSH config root needs literal filename handling",
            ));
        }
        text.push_str(&format!("Host *\nInclude {}\n", quoted(root)?));
    }
    text.push_str(&format!(
        "Host *\nMatch final\n  ControlMaster auto\n  ControlPath {}\n  ControlPersist 300\n",
        quoted(socket)?
    ));
    Ok(text)
}

fn quoted(path: &Path) -> std::io::Result<String> {
    let Some(path) = path.to_str().filter(|p| !p.contains(['\n', '\r', '\0'])) else {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "SSH configuration path cannot be quoted",
        ));
    };
    Ok(format!(
        "\"{}\"",
        path.replace('\\', "\\\\").replace('"', "\\\"")
    ))
}

fn config(dest: &str) -> Option<Arc<Config>> {
    MANAGER
        .get_or_init(|| Manager::new().ok())
        .as_ref()?
        .config(dest)
        .ok()
}

pub(super) struct Configured {
    _opening: Option<Opening>,
}

pub(super) fn configure(command: &mut Command, dest: &str, serialize: bool) -> Option<Configured> {
    let config = config(dest)?;
    // A prepared streaming process can wait in the process service queue. Holding a
    // startup gate there would block remote cancellation cleanup behind that queue.
    let opening = if serialize {
        Some(config.take()?)
    } else {
        None
    };
    command.arg("-F").arg(&config.path);
    Some(Configured { _opening: opening })
}

pub(super) fn context(dest: &str) -> String {
    security_context(dest).0
}

pub(super) fn shutdown() {
    let Some(Some(manager)) = MANAGER.get() else {
        return;
    };
    // Closing one command must never stop a master. At app shutdown all jobs and shells
    // have ended. Ask only sockets in this instance's private directory to stop listening;
    // existing channels (if any) can drain, and ControlPersist bounds their remaining life.
    if let Ok(entries) = std::fs::read_dir(&manager.dir) {
        for entry in entries.flatten() {
            if entry.file_type().is_ok_and(|t| {
                use std::os::unix::fs::FileTypeExt;
                t.is_socket()
            }) {
                let _ = Command::new("ssh")
                    .args(["-S"])
                    .arg(entry.path())
                    .args(["-O", "stop", "--", "localhost"])
                    .stdin(std::process::Stdio::null())
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .status();
            }
        }
    }
    let _ = std::fs::remove_dir_all(&manager.dir);
}

#[cfg(test)]
mod tests;

mod security;
pub(super) use security::dynamic;
use security::*;
