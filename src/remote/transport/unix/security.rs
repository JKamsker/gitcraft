//! Authentication fingerprints and conservative OpenSSH reuse eligibility.
use super::*;

/// Fingerprint only, never logged: configuration includes, agent/security context and
/// referenced key/host-key file state. Repository paths and askpass attempt IDs are absent,
/// so tabs of the same host share, while credential/config changes start fresh shells.
pub(super) fn security_context(dest: &str) -> (String, Vec<PathBuf>) {
    let env = std::env::vars_os().collect();
    security_context_with_home(dest, &env, account_home())
}

pub(super) fn account_home() -> Option<&'static Path> {
    static HOME: OnceLock<Option<PathBuf>> = OnceLock::new();
    HOME.get_or_init(|| gitcraft_os::account_home().ok())
        .as_deref()
}

#[cfg(test)]
pub(super) fn security_context_with(
    dest: &str,
    env: &BTreeMap<OsString, OsString>,
) -> (String, Vec<PathBuf>) {
    let home = env.get(&OsString::from("HOME")).map(PathBuf::from);
    security_context_with_home(dest, env, home.as_deref())
}

pub(super) fn security_context_with_home(
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
    for epoch in crate::remote::askpass::security_epoch(&format!("SSH · {dest}")) {
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

pub(super) fn fingerprint_file(path: &Path, digest: &mut Sha256) {
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

pub(super) fn fingerprint_public_companion(file: &Path, digest: &mut Sha256) {
    let mut public = file.as_os_str().to_owned();
    public.push(".pub");
    fingerprint_file(Path::new(&public), digest);
}

pub(super) fn fingerprint_identity_companions(identity: &Path, digest: &mut Sha256) {
    // OpenSSH loads an identity's public companion and, in the absence of explicit
    // CertificateFile settings, its certificate both with and without the .pub suffix.
    fingerprint_public_companion(identity, digest);
    let mut certificate = identity.as_os_str().to_owned();
    certificate.push("-cert");
    fingerprint_file(Path::new(&certificate), digest);
    fingerprint_public_companion(Path::new(&certificate), digest);
}

pub(super) fn expand_path(
    value: &str,
    home: Option<&Path>,
    env: &BTreeMap<OsString, OsString>,
) -> PathBuf {
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

pub(super) fn source_include_base(root: &Path, home: Option<&Path>, custom: bool) -> PathBuf {
    if !custom && root == Path::new("/etc/ssh/ssh_config") {
        PathBuf::from("/etc/ssh")
    } else {
        home.map(|home| home.join(".ssh")).unwrap_or_default()
    }
}

pub(super) struct ConfigSource<'a> {
    pub(super) include_base: &'a Path,
    pub(super) home: Option<&'a Path>,
    pub(super) env: &'a BTreeMap<OsString, OsString>,
}

pub(super) fn fingerprint_config(
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
            for value in crate::remote::discover::config_words(rest) {
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
                    for file in crate::remote::discover::expand(&file) {
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
pub(in crate::remote::transport) fn dynamic(dest: &str) -> bool {
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

pub(in crate::remote::transport) fn dynamic_file(
    path: &Path,
    source: &ConfigSource<'_>,
    depth: usize,
) -> bool {
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
        let words = crate::remote::discover::config_words(
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
                if crate::remote::discover::expand(&file)
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

pub(super) fn portable_system_includes(path: &Path, depth: usize) -> bool {
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
        for value in crate::remote::discover::config_words(
            rest.trim_start_matches(|c: char| c.is_whitespace() || c == '='),
        ) {
            let path = Path::new(&value);
            if !path.is_absolute() || value.contains(['$', '~', '[', ']', '\\']) {
                return false;
            }
            if crate::remote::discover::expand(path)
                .iter()
                .any(|p| !portable_system_includes(p, depth + 1))
            {
                return false;
            }
        }
    }
    true
}

#[cfg(test)]
pub(super) mod tests;
