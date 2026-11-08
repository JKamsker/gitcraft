//! Transient secrets are bound to one Git credential endpoint and one account.
//! The POSIX helper also runs on repository machines where this executable is absent.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CredentialScope {
    protocol: String,
    authority: String,
    default_authority: String,
    path: String,
    account: String,
}

fn field(value: &str) -> Result<(), String> {
    if value.len() > 16384 || value.chars().any(char::is_control) {
        return Err(crate::i18n::text(
            "backend-credential-fields-must-be-bounded-single-line-values-16b34d",
        )
        .into());
    }
    Ok(())
}
fn decode(value: &str) -> Result<String, String> {
    let mut bytes = Vec::new();
    let mut at = 0;
    while at < value.len() {
        if value.as_bytes()[at] == b'%' {
            let hex = value.get(at + 1..at + 3).ok_or(crate::i18n::text(
                "backend-invalid-credential-url-escape-c15863",
            ))?;
            bytes.push(
                u8::from_str_radix(hex, 16).map_err(|_| {
                    crate::i18n::text("backend-invalid-credential-url-escape-c15863")
                })?,
            );
            at += 3;
        } else {
            bytes.push(value.as_bytes()[at]);
            at += 1;
        }
    }
    let text = String::from_utf8(bytes)
        .map_err(|_| crate::i18n::text("backend-credential-url-must-be-utf-8-60a721"))?;
    field(&text)?;
    Ok(text)
}
impl CredentialScope {
    /// Capture the actual attempted URL, never a remote name or a default fetch URL.
    /// Paths use Git credential protocol's percent-decoded representation.
    pub fn for_url(url: &str, account: &str) -> Result<Self, String> {
        field(url)?;
        field(account)?;
        if account.is_empty() {
            return Err(crate::i18n::text(
                "backend-a-transient-credential-requires-a-named-account-d8810e",
            )
            .into());
        }
        let (protocol, rest) = url.split_once("://").ok_or(crate::i18n::text(
            "backend-credentials-require-an-explicit-http-s-endpoint-ba6f8d",
        ))?;
        let protocol = protocol.to_ascii_lowercase();
        let default_port = match protocol.as_str() {
            "https" => 443,
            "http" => 80,
            _ => {
                return Err(crate::i18n::text(
                    "backend-transient-credentials-are-only-supported-for-http-s-endpoints-3e5bb6",
                )
                .into());
            }
        };
        let (authority, path) = rest.split_once('/').unwrap_or((rest, ""));
        if authority.contains(['?', '#', '\\']) {
            return Err(
                crate::i18n::text("backend-invalid-credential-url-authority-e092fb").into(),
            );
        }
        let authority = if let Some((userinfo, host)) = authority.rsplit_once('@') {
            if userinfo.contains(':') || decode(userinfo)? != account {
                return Err(crate::i18n::text("backend-the-url-account-does-not-match-the-selected-account-or-embeds-a-p-26be25").into());
            }
            host
        } else {
            authority
        };
        let (host, port) = if let Some(ipv6) = authority.strip_prefix('[') {
            let (ip, tail) = ipv6.split_once(']').ok_or(crate::i18n::text(
                "backend-invalid-ipv6-credential-host-8550c4",
            ))?;
            ip.parse::<std::net::Ipv6Addr>()
                .map_err(|_| crate::i18n::text("backend-invalid-ipv6-credential-host-8550c4"))?;
            let port = if tail.is_empty() {
                default_port
            } else {
                tail.strip_prefix(':')
                    .ok_or(crate::i18n::text("backend-invalid-credential-port-374dd6"))?
                    .parse::<u16>()
                    .map_err(|_| crate::i18n::text("backend-invalid-credential-port-374dd6"))?
            };
            (format!("[{ip}]"), port)
        } else {
            let (host, port) =
                authority
                    .split_once(':')
                    .map_or(Ok((authority, default_port)), |(h, p)| {
                        p.parse::<u16>().map(|p| (h, p)).map_err(|_| {
                            crate::i18n::text("backend-invalid-credential-port-374dd6")
                        })
                    })?;
            if host.is_empty()
                || host.starts_with('-')
                || !host
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'_'))
            {
                return Err(crate::i18n::text("backend-invalid-credential-host-fb5ee2").into());
            }
            (host.to_ascii_lowercase(), port)
        };
        if port == 0 {
            return Err(crate::i18n::text("backend-invalid-credential-port-374dd6").into());
        }
        let mut path = decode(path.trim_start_matches('/'))?;
        // Git keeps its first decoded character even when that character is '/'.
        // Thus encoded slash-only paths remain distinct from an absent root path.
        while path.len() > 1 && path.ends_with('/') {
            path.pop();
        }
        Ok(Self {
            protocol,
            authority: format!("{host}:{port}"),
            default_authority: if port == default_port {
                host
            } else {
                String::new()
            },
            path,
            account: account.into(),
        })
    }
    pub fn matches_url(&self, url: &str) -> bool {
        Self::for_url(url, &self.account).is_ok_and(|other| other == *self)
    }
    #[expect(
        dead_code,
        reason = "Retained scope label API; labels must not reconstruct a transport URL"
    )]
    pub fn endpoint(&self) -> String {
        // Labels are not a way to rebuild a transport URL (path is decoded).
        format!("{}://{}/{}", self.protocol, self.authority, self.path)
    }
    pub(crate) fn is_https(&self) -> bool {
        self.protocol == "https"
    }
    #[expect(dead_code, reason = "Retained explicit-account scope inspection API")]
    pub fn account(&self) -> &str {
        &self.account
    }
}

/// Install together with `credential.useHttpPath=true`; scope environment comes from
/// `Credentials::helper_environment`. Unknown/duplicate fields never broaden a match.
pub const CREDENTIAL_HELPER: &str = concat!(
    "credential.helper=!f() { test \"$1\" = get || return 0; ",
    "if test -z \"$GITCRAFT_CREDENTIAL_OWNER\" || test \"$PPID\" != \"$GITCRAFT_CREDENTIAL_OWNER\"; then printf '%s\\n' 'Scoped sign-in is only available to the designated Git HTTP transport. LFS and credential-fill consumers must use their own explicitly selected account.' >&2; printf 'quit=true\\n'; return 0; fi; ",
    "p= h= x= u= sp= sh= sx= su=; while IFS= read -r line && test -n \"$line\"; do case \"$line\" in ",
    "protocol=*) test -z \"$sp\" || return 0; sp=1; p=${line#protocol=};; ",
    "host=*) test -z \"$sh\" || return 0; sh=1; h=${line#host=};; ",
    "path=*) test -z \"$sx\" || return 0; sx=1; x=${line#path=};; ",
    "username=*) test -z \"$su\" || return 0; su=1; u=${line#username=};; esac; done; ",
    "test -n \"$GITCRAFT_CREDENTIAL_PROTOCOL\" && test -n \"$GITCRAFT_CREDENTIAL_HOST\" && test -n \"$GITCRAFT_GIT_USERNAME\" || return 0; ",
    "test \"$p\" = \"$GITCRAFT_CREDENTIAL_PROTOCOL\" || return 0; ",
    "h=$(printf '%s' \"$h\" | tr '[:upper:]' '[:lower:]'); ",
    "test \"$h\" = \"$GITCRAFT_CREDENTIAL_HOST\" || { test -n \"$GITCRAFT_CREDENTIAL_DEFAULT_HOST\" && test \"$h\" = \"$GITCRAFT_CREDENTIAL_DEFAULT_HOST\"; } || return 0; ",
    "test \"$x\" = \"$GITCRAFT_CREDENTIAL_PATH\" || return 0; ",
    "test -z \"$su\" || test \"$u\" = \"$GITCRAFT_GIT_USERNAME\" || return 0; ",
    "printf 'username=%s\\npassword=%s\\n' \"$GITCRAFT_GIT_USERNAME\" \"$GITCRAFT_GIT_PASSWORD\"; }; f"
);
impl super::Credentials {
    pub fn for_url(url: &str, username: String, password: String) -> Result<Self, String> {
        field(&password)?;
        if password.is_empty() {
            return Err(
                crate::i18n::text("backend-an-empty-password-cannot-be-cached-1f6db3").into(),
            );
        }
        let scope = CredentialScope::for_url(url, &username)?;
        Ok(Self {
            username,
            password,
            scope: Some(scope),
        })
    }
    /// Shared by ordinary Git, preflight and multi-host runners. Never synthesize a
    /// scope for legacy cached secrets: require another explicit scoped sign-in.
    pub fn helper_environment(&self) -> Result<Vec<(&'static str, String)>, String> {
        let scope = self.scope.as_ref().ok_or(crate::i18n::text(
            "backend-unscoped-cached-credentials-refused-sign-in-again-for-the-actual-ec3b92",
        ))?;
        field(&self.username)?;
        field(&self.password)?;
        if scope.account != self.username {
            return Err(crate::i18n::text(
                "backend-cached-credential-account-changed-sign-in-again-9a01c6",
            )
            .into());
        }
        Ok(vec![
            ("GITCRAFT_GIT_USERNAME", self.username.clone()),
            ("GITCRAFT_GIT_PASSWORD", self.password.clone()),
            ("GITCRAFT_CREDENTIAL_PROTOCOL", scope.protocol.clone()),
            ("GITCRAFT_CREDENTIAL_HOST", scope.authority.clone()),
            (
                "GITCRAFT_CREDENTIAL_DEFAULT_HOST",
                scope.default_authority.clone(),
            ),
            ("GITCRAFT_CREDENTIAL_PATH", scope.path.clone()),
        ])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn scopes_bind_protocol_authority_port_path_and_account() -> Result<(), String> {
        let scope = CredentialScope::for_url("https://Host-A.invalid/team/repo.git", "alice")?;
        assert!(scope.matches_url("https://host-a.invalid:443/team/repo.git"));
        for url in [
            "http://host-a.invalid/team/repo.git",
            "https://host-b.invalid/team/repo.git",
            "https://host-a.invalid:444/team/repo.git",
            "https://host-a.invalid/team/other.git",
            "https://host-a.invalid/team/repo.git/info/lfs",
        ] {
            assert!(!scope.matches_url(url), "{url}");
        }
        for url in [
            "https://host-a.invalid/team/repo.git/",
            "https://host-a.invalid//team/repo.git",
            "https://host-a.invalid/team/repo.git%2F",
        ] {
            assert!(scope.matches_url(url), "{url}");
        }
        assert!(
            CredentialScope::for_url("https://bob@host-a.invalid/team/repo.git", "alice").is_err()
        );
        assert!(
            CredentialScope::for_url(
                "https://alice:embedded@host-a.invalid/team/repo.git",
                "alice"
            )
            .is_err()
        );
        for url in [
            "https://host-a.invalid/%2F",
            "https://host-a.invalid/%2F%2F",
            "https://host-a.invalid/%2F/",
        ] {
            let encoded = CredentialScope::for_url(url, "alice")?;
            assert_eq!(encoded.path, "/");
            assert!(!encoded.matches_url("https://host-a.invalid/"));
        }
        assert_ne!(
            scope,
            CredentialScope::for_url("https://host-a.invalid/team/repo.git", "bob")?
        );
        assert_eq!(
            CredentialScope::for_url("https://host-a.invalid/team/a%20b", "alice")?.path,
            "team/a b"
        );
        assert!(CredentialScope::for_url("https://host-a.invalid/%0ainjected", "alice").is_err());
        let legacy = super::super::Credentials {
            username: "alice".into(),
            password: "DUMMY".into(),
            scope: None,
        };
        assert!(legacy.helper_environment().is_err());
        Ok(())
    }
}

#[cfg(test)]
mod helper_tests {
    use super::*;
    #[test]
    fn helper_only_installation_records_no_consumer_authority() -> Result<(), String> {
        let credentials = super::super::Credentials::for_url(
            "https://host-a.invalid/team/repo.git",
            "DUMMY_HOST_A_USER".into(),
            "DUMMY_HOST_A_TOKEN".into(),
        )?;
        let environment = credentials.helper_environment()?;
        assert!(
            !environment
                .iter()
                .any(|(key, _)| *key == "GITCRAFT_CREDENTIAL_OWNER")
        );
        assert_eq!(
            environment
                .iter()
                .find(|(key, _)| *key == "GITCRAFT_CREDENTIAL_PATH")
                .map(|(_, value)| value.as_str()),
            Some("team/repo.git")
        );
        assert!(CREDENTIAL_HELPER.contains("test -z \"$GITCRAFT_CREDENTIAL_OWNER\""));
        assert!(CREDENTIAL_HELPER.contains("quit=true"));
        assert!(!CREDENTIAL_HELPER.contains("DUMMY_HOST_A_TOKEN"));
        // This records an intentionally unsupported helper-only installation.
        // Ordinary HTTP/LFS ancestry remains a held, separately authorized fixture.
        Ok(())
    }
}
