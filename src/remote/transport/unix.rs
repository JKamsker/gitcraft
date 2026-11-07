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

/// Fingerprint only, never logged: configuration includes, agent/security context and
/// referenced key/host-key file state. Repository paths and askpass attempt IDs are absent,
/// so tabs of the same host share, while credential/config changes start fresh shells.
fn security_context(dest: &str) -> (String, Vec<PathBuf>) {
    let env = std::env::vars_os().collect();
    security_context_with_home(dest, &env, account_home())
}

fn account_home() -> Option<&'static Path> {
    static HOME: OnceLock<Option<PathBuf>> = OnceLock::new();
    HOME.get_or_init(|| gitcraft_os::account_home().ok())
        .as_deref()
}

#[cfg(test)]
fn security_context_with(dest: &str, env: &BTreeMap<OsString, OsString>) -> (String, Vec<PathBuf>) {
    let home = env.get(&OsString::from("HOME")).map(PathBuf::from);
    security_context_with_home(dest, env, home.as_deref())
}

fn security_context_with_home(
    dest: &str,
    env: &BTreeMap<OsString, OsString>,
    home: Option<&Path>,
) -> (String, Vec<PathBuf>) {
    let home = home.map(Path::to_owned);
    let mut roots = match env
        .get(&OsString::from("GITCRAFT_SSH_CONFIG"))
        .filter(|p| !p.is_empty())
    {
        Some(custom) => vec![PathBuf::from(custom)],
        None => home
            .as_ref()
            .map(|h| h.join(".ssh/config"))
            .into_iter()
            .chain([PathBuf::from("/etc/ssh/ssh_config")])
            .collect(),
    };
    for root in &mut roots {
        if root.is_relative()
            && let Ok(cwd) = std::env::current_dir()
        {
            *root = cwd.join(&*root);
        }
    }
    let mut digest = Sha256::new();
    digest.update(dest.as_bytes());
    digest.update([0]);
    for epoch in super::super::askpass::security_epoch(&format!("SSH · {dest}")) {
        digest.update(epoch.to_le_bytes());
    }
    // Environment expansion and Match exec may use any of these. Session-specific askpass
    // values are applied directly to children, not to the parent environment.
    for (name, value) in env {
        digest.update(name.as_encoded_bytes());
        digest.update([0]);
        digest.update(value.as_encoded_bytes());
        digest.update([0]);
    }
    if let Ok(cwd) = std::env::current_dir() {
        digest.update(cwd.as_os_str().as_encoded_bytes());
    }
    let custom = env
        .get(&OsString::from("GITCRAFT_SSH_CONFIG"))
        .is_some_and(|value| !value.is_empty());
    for root in &roots {
        // Include resolution follows the original user/system parse context, including
        // inside absolute Includes located outside that configuration's directory.
        let base = source_include_base(root, home.as_deref(), custom);
        let source = ConfigSource {
            include_base: &base,
            home: home.as_deref(),
            env,
        };
        let mut seen = Vec::new();
        fingerprint_config(root, &source, &mut seen, &mut digest, 0);
    }
    if let Some(home) = &home {
        for name in [
            "id_rsa",
            "id_ecdsa",
            "id_ecdsa_sk",
            "id_ed25519",
            "id_ed25519_sk",
            "id_dsa",
            "known_hosts",
            "known_hosts2",
        ] {
            let file = home.join(".ssh").join(name);
            fingerprint_file(&file, &mut digest);
            if name.starts_with("id_") {
                fingerprint_identity_companions(&file, &mut digest);
            }
        }
    }
    for name in ["/etc/ssh/ssh_known_hosts", "/etc/ssh/ssh_known_hosts2"] {
        fingerprint_file(Path::new(name), &mut digest);
    }
    for name in ["SSH_AUTH_SOCK", "KRB5CCNAME"] {
        if let Some(path) = env.get(&OsString::from(name)) {
            fingerprint_file(Path::new(path), &mut digest);
        }
    }
    // 128 bits keeps socket names short, including OpenSSH's temporary bind suffix.
    let key: String = digest
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    (key[..32].to_owned(), roots)
}

fn fingerprint_file(path: &Path, digest: &mut Sha256) {
    digest.update(path.as_os_str().as_encoded_bytes());
    if let Ok(metadata) = std::fs::metadata(path) {
        for value in [
            metadata.dev(),
            metadata.ino(),
            metadata.len(),
            metadata.mtime() as u64,
            metadata.mtime_nsec() as u64,
            metadata.ctime() as u64,
            metadata.ctime_nsec() as u64,
            metadata.mode() as u64,
            metadata.uid() as u64,
        ] {
            digest.update(value.to_le_bytes());
        }
    }
}

fn fingerprint_public_companion(file: &Path, digest: &mut Sha256) {
    let mut public = file.as_os_str().to_owned();
    public.push(".pub");
    fingerprint_file(Path::new(&public), digest);
}

fn fingerprint_identity_companions(identity: &Path, digest: &mut Sha256) {
    // OpenSSH loads an identity's public companion and, in the absence of explicit
    // CertificateFile settings, its certificate both with and without the .pub suffix.
    fingerprint_public_companion(identity, digest);
    let mut certificate = identity.as_os_str().to_owned();
    certificate.push("-cert");
    fingerprint_file(Path::new(&certificate), digest);
    fingerprint_public_companion(Path::new(&certificate), digest);
}

fn expand_path(value: &str, home: Option<&Path>, env: &BTreeMap<OsString, OsString>) -> PathBuf {
    let mut value = value.to_owned();
    for (name, replacement) in env {
        if let (Some(name), Some(replacement)) = (name.to_str(), replacement.to_str()) {
            value = value.replace(&format!("${{{name}}}"), replacement);
        }
    }
    if let Some(rest) = value.strip_prefix("~/")
        && let Some(home) = home
    {
        return home.join(rest);
    }
    PathBuf::from(value)
}

fn source_include_base(root: &Path, home: Option<&Path>, custom: bool) -> PathBuf {
    if !custom && root == Path::new("/etc/ssh/ssh_config") {
        PathBuf::from("/etc/ssh")
    } else {
        home.map(|home| home.join(".ssh")).unwrap_or_default()
    }
}

struct ConfigSource<'a> {
    include_base: &'a Path,
    home: Option<&'a Path>,
    env: &'a BTreeMap<OsString, OsString>,
}

fn fingerprint_config(
    path: &Path,
    source: &ConfigSource<'_>,
    seen: &mut Vec<PathBuf>,
    digest: &mut Sha256,
    depth: usize,
) {
    let ConfigSource {
        include_base,
        home,
        env,
    } = *source;
    fingerprint_file(path, digest);
    if depth > 16 || seen.iter().any(|p| p == path) {
        return;
    }
    seen.push(path.to_owned());
    let Ok(text) = std::fs::read_to_string(path) else {
        return;
    };
    digest.update(text.as_bytes());
    for line in text.lines().map(str::trim).filter(|s| !s.starts_with('#')) {
        let (key, rest) = line
            .split_once(|c: char| c.is_whitespace() || c == '=')
            .unwrap_or((line, ""));
        let rest = rest.trim_start_matches(|c: char| c.is_whitespace() || c == '=');
        let include = key.eq_ignore_ascii_case("include");
        let security_file = [
            "identityfile",
            "certificatefile",
            "userknownhostsfile",
            "globalknownhostsfile",
            "revokedhostkeys",
            "identityagent",
        ]
        .iter()
        .any(|name| key.eq_ignore_ascii_case(name));
        if include || security_file {
            for value in super::super::discover::config_words(rest) {
                let mut file = expand_path(&value, home, env);
                if file.is_relative() {
                    // OpenSSH resolves user Include paths against ~/.ssh, not the file
                    // containing the directive; system Includes are relative to /etc/ssh.
                    let base = if !include {
                        // Unlike Include, relative key/agent/trust paths are relative
                        // to the local ssh process's working directory.
                        std::env::current_dir().unwrap_or_default()
                    } else {
                        include_base.to_owned()
                    };
                    file = base.join(file);
                }
                if include {
                    digest.update(file.as_os_str().as_encoded_bytes());
                    for file in super::super::discover::expand(&file) {
                        fingerprint_config(&file, source, seen, digest, depth + 1);
                    }
                } else {
                    fingerprint_file(&file, digest);
                    if key.eq_ignore_ascii_case("identityfile") {
                        fingerprint_identity_companions(&file, digest);
                    } else if key.eq_ignore_ascii_case("certificatefile") {
                        fingerprint_public_companion(&file, digest);
                    }
                }
            }
        }
    }
}

/// External program/network state cannot safely identify a persistent authentication
/// context. Retain SSH config semantics while opting these hosts out of shell/transport reuse.
pub(super) fn dynamic(dest: &str) -> bool {
    let env: BTreeMap<OsString, OsString> = std::env::vars_os().collect();
    let home = account_home();
    let (_, roots) = security_context_with_home(dest, &env, home);
    let custom = env
        .get(&OsString::from("GITCRAFT_SSH_CONFIG"))
        .is_some_and(|value| !value.is_empty());
    roots.iter().any(|root| {
        let base = source_include_base(root, home, custom);
        dynamic_file(
            root,
            &ConfigSource {
                include_base: &base,
                home,
                env: &env,
            },
            0,
        )
    })
}

fn dynamic_file(path: &Path, source: &ConfigSource<'_>, depth: usize) -> bool {
    let ConfigSource {
        include_base,
        home,
        env,
    } = *source;
    if depth > 16 {
        return true;
    }
    let Ok(text) = std::fs::read_to_string(path) else {
        return false;
    };
    for line in text.lines().map(str::trim) {
        let (key, rest) = line
            .split_once(|c: char| c.is_whitespace() || c == '=')
            .unwrap_or((line, ""));
        let words = super::super::discover::config_words(
            rest.trim_start_matches(|c: char| c.is_whitespace() || c == '='),
        );
        if key.eq_ignore_ascii_case("match")
            && words.iter().any(|w| {
                w.trim_start_matches('!').eq_ignore_ascii_case("exec")
                    || w.trim_start_matches('!')
                        .eq_ignore_ascii_case("localnetwork")
            })
        {
            return true;
        }
        let security_file = [
            "identityfile",
            "certificatefile",
            "userknownhostsfile",
            "globalknownhostsfile",
            "revokedhostkeys",
            "identityagent",
        ]
        .iter()
        .any(|name| key.eq_ignore_ascii_case(name));
        // These tokens depend on OpenSSH's effective user/hostname/canonicalization.
        // Refuse reuse rather than guessing at a security file's resolved identity.
        let include = key.eq_ignore_ascii_case("include");
        if security_file && words.iter().any(|value| value.contains('%')) {
            return true;
        }
        // The lightweight fingerprint parser deliberately does not reinterpret all
        // OpenSSH path grammar. Keep native behavior for named-user tilde expansion,
        // backslash escapes, or bracket globs that it cannot resolve exactly.
        if (security_file || include)
            && (rest.contains('\\')
                || words.iter().any(|value| {
                    value.starts_with('~') && !value.starts_with("~/")
                        || include && value.contains(['[', ']'])
                }))
        {
            return true;
        }
        if include {
            for value in words {
                let mut file = expand_path(&value, home, env);
                if file.is_relative() {
                    file = include_base.join(file);
                }
                if super::super::discover::expand(&file)
                    .iter()
                    .any(|p| dynamic_file(p, source, depth + 1))
                {
                    return true;
                }
            }
        }
    }
    false
}

fn portable_system_includes(path: &Path, depth: usize) -> bool {
    if depth > 16 {
        return false;
    }
    let Ok(text) = std::fs::read_to_string(path) else {
        return true;
    };
    for line in text.lines().map(str::trim) {
        let (key, rest) = line
            .split_once(|c: char| c.is_whitespace() || c == '=')
            .unwrap_or((line, ""));
        if !key.eq_ignore_ascii_case("include") {
            continue;
        }
        for value in super::super::discover::config_words(
            rest.trim_start_matches(|c: char| c.is_whitespace() || c == '='),
        ) {
            let path = Path::new(&value);
            if !path.is_absolute() || value.contains(['$', '~', '[', ']', '\\']) {
                return false;
            }
            if super::super::discover::expand(path)
                .iter()
                .any(|p| !portable_system_includes(p, depth + 1))
            {
                return false;
            }
        }
    }
    true
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
