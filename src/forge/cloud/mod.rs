//! Captured authentication and bounded, origin-pinned requests for native cloud providers.
pub(crate) mod ui;
use crate::{connections, forge::Hosting};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    io::Read,
    path::Path,
    time::{Duration, Instant},
};
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Service {
    Azure,
    Bitbucket,
}
impl Service {
    pub fn name(self) -> &'static str {
        match self {
            Self::Azure => "Azure DevOps",
            Self::Bitbucket => "Bitbucket Cloud",
        }
    }
    pub fn account(self, c: &connections::Context) -> &Account {
        match self {
            Self::Azure => &c.choices.azure,
            Self::Bitbucket => &c.choices.bitbucket,
        }
    }
    pub fn server(self, s: &str) -> Result<String, String> {
        let s = s.trim_end_matches('/');
        match self {
            Self::Azure => {
                let org = s
                    .strip_prefix("https://dev.azure.com/")
                    .ok_or(crate::i18n::text(
                        "backend-use-https-dev-azure-com-organization-87fdec",
                    ))?;
                if !plain(org) {
                    return Err(crate::i18n::text(
                        "backend-use-one-plain-azure-organization-name-a62b8b",
                    )
                    .into());
                }
                Ok(format!(
                    "https://dev.azure.com/{}",
                    org.to_ascii_lowercase()
                ))
            }
            Self::Bitbucket if s == "https://bitbucket.org" => Ok(s.into()),
            _ => Err(
                crate::i18n::text("backend-bitbucket-cloud-uses-https-bitbucket-org-e58569").into(),
            ),
        }
    }
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Account {
    pub server: String,
    /// Stable native viewer ID: Azure GUID / Bitbucket UUID.
    pub user: String,
    /// Bitbucket API token's Atlassian account email.
    pub credential_user: String,
    pub secret_ref: String,
    /// Azure uses its installed az login unless a saved PAT is explicitly selected.
    pub pat: bool,
    pub config_dir: String,
}
impl Account {
    pub fn validate(&self, s: Service) -> Result<(), String> {
        if self == &Self::default() {
            return Ok(());
        }
        s.server(&self.server)?;
        if self.user.is_empty()
            || self.user.len() > 256
            || self
                .user
                .chars()
                .any(|c| c.is_control() || c.is_whitespace())
        {
            return Err(crate::i18n::text(
                "backend-choose-the-native-account-s-stable-user-id-fd9c56",
            )
            .into());
        }
        if self.credential_user.chars().any(char::is_control)
            || self.secret_ref.chars().any(char::is_control)
        {
            return Err(crate::i18n::text(
                "backend-account-metadata-cannot-contain-control-characters-dcf623",
            )
            .into());
        }
        if !self.config_dir.is_empty()
            && (!Path::new(&self.config_dir).is_absolute()
                || self.config_dir.chars().any(char::is_control))
        {
            return Err(crate::i18n::text(
                "backend-azure-cli-configuration-folder-must-be-absolute-e7ac9c",
            )
            .into());
        }
        if s == Service::Bitbucket && self.credential_user.is_empty() {
            return Err(crate::i18n::text(
                "backend-choose-the-atlassian-email-used-with-this-api-token-8d225a",
            )
            .into());
        }
        Ok(())
    }
    fn key(&self, s: Service) -> String {
        json!([
            s.name(),
            self.server,
            self.user,
            self.credential_user,
            self.secret_ref
        ])
        .to_string()
    }
    pub fn store(&self, s: Service, token: &str) -> Result<(), String> {
        self.validate(s)?;
        if token.is_empty() || token.chars().any(char::is_control) || self.secret_ref.is_empty() {
            return Err(
                crate::i18n::text("backend-enter-a-token-and-secure-reference-ae8840").into(),
            );
        }
        if !crate::portable::credential_store() {
            return Err(crate::i18n::text("backend-credential-storage-is-disabled-ddef8c").into());
        }
        keyring::Entry::new("gitcraft-native-forge", &self.key(s))
            .map_err(|_| crate::i18n::text("backend-cannot-open-secure-store-a4aa7a"))?
            .set_password(token)
            .map_err(|_| crate::i18n::text("backend-cannot-save-selected-credential-146c35").into())
    }
    pub fn forget(&self, s: Service) -> Result<(), String> {
        if !crate::portable::credential_store() {
            return Err(crate::i18n::text("backend-credential-storage-is-disabled-ddef8c").into());
        }
        keyring::Entry::new("gitcraft-native-forge", &self.key(s))
            .map_err(|_| crate::i18n::text("backend-cannot-open-secure-store-a4aa7a"))?
            .delete_credential()
            .map_err(|_| {
                crate::i18n::text("backend-cannot-forget-selected-credential-834a7e").into()
            })
    }
    fn secret(&self, s: Service) -> Result<String, String> {
        if self.secret_ref.is_empty() || !crate::portable::credential_store() {
            return Err(crate::i18n::text(
                "backend-save-this-profile-s-selected-provider-token-in-connections-first-48842f",
            )
            .into());
        }
        keyring::Entry::new("gitcraft-native-forge", &self.key(s))
            .map_err(|_| crate::i18n::text("backend-cannot-open-secure-store-a4aa7a"))?
            .get_password()
            .map_err(|_| {
                crate::i18n::text(
                    "backend-selected-provider-credential-is-unavailable-on-this-computer-65eed6",
                )
                .into()
            })
    }
}
pub(super) fn plain(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 256
        && !s.starts_with(['-', '.'])
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
}
pub(super) fn segment(s: &str) -> String {
    s.bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) {
                (b as char).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect()
}
pub(super) fn text(v: &Value, p: &str) -> String {
    v.pointer(p)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .into()
}
pub(super) fn required<'a>(v: &'a Value, p: &str) -> Result<&'a str, String> {
    v.pointer(p)
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| {
            crate::i18n::message(
                "backend-provider-response-is-missing-p-efbf32",
                &[("pointer", (p).to_string().into())],
            )
        })
}
#[cfg(test)]
type Recording =
    std::sync::Arc<dyn Fn(&str, &str, Option<&Value>) -> Result<Value, String> + Send + Sync>;
#[derive(Clone)]
pub(in crate::forge) struct Api {
    pub service: Service,
    pub context: connections::Context,
    pub viewer: String,
    pub hosting: Hosting,
    base: String,
    authorization: String,
    agent: ureq::Agent,
    began: Instant,
    expires: Option<u64>,
    #[cfg(test)]
    pub record: Option<Recording>,
}
impl Api {
    pub fn new(service: Service, h: &Hosting) -> Result<Self, String> {
        let c = connections::current();
        if let Some(e) = &c.error {
            return Err(e.clone());
        }
        let a = service.account(&c);
        a.validate(service)?;
        let server = service.server(&a.server)?;
        let expected = match service {
            Service::Azure => format!(
                "https://dev.azure.com/{}",
                h.path.split('/').next().unwrap_or_default()
            ),
            Service::Bitbucket => "https://bitbucket.org".into(),
        };
        if server != expected
            || h.web
                != match service {
                    Service::Azure => "https://dev.azure.com",
                    Service::Bitbucket => "https://bitbucket.org",
                }
        {
            return Err(crate::i18n::text(
                "backend-selected-provider-server-does-not-match-the-displayed-repository-13d818",
            )
            .into());
        }
        use base64::Engine;
        let (authorization, expires) = match service {
            Service::Azure if !a.pat => azure_token(a)?,
            _ => {
                let token = a.secret(service)?;
                let user = if service == Service::Azure {
                    ""
                } else {
                    &a.credential_user
                };
                (
                    format!(
                        "Basic {}",
                        base64::engine::general_purpose::STANDARD.encode(format!("{user}:{token}"))
                    ),
                    None,
                )
            }
        };
        let base = if service == Service::Azure {
            server
        } else {
            "https://api.bitbucket.org/2.0".into()
        };
        let api = Self {
            service,
            context: c.clone(),
            viewer: a.user.clone(),
            hosting: h.clone(),
            authorization,
            agent: super::http::with_proxy(ureq::AgentBuilder::new(), &base)
                .redirects(0)
                .timeout(Duration::from_secs(30))
                .build(),
            base,
            began: Instant::now(),
            expires,
            #[cfg(test)]
            record: None,
        };
        api.verify_viewer()?;
        Ok(api)
    }
    pub fn verify_viewer(&self) -> Result<(), String> {
        let v = self.get(match self.service {
            Service::Azure => "_apis/connectionData?connectOptions=0&api-version=7.1-preview.1",
            Service::Bitbucket => "user",
        })?;
        let id = required(
            &v,
            if self.service == Service::Azure {
                "/authenticatedUser/id"
            } else {
                "/uuid"
            },
        )?;
        if !id.eq_ignore_ascii_case(&self.viewer) {
            return Err(crate::i18n::text(
                "backend-provider-authenticated-a-different-native-account-reload-after-se-b79626",
            )
            .into());
        }
        Ok(())
    }
    pub fn guard_context(&self) -> Result<(), String> {
        if self.context != connections::current() {
            return Err(crate::i18n::text(
                "backend-profile-changed-reload-this-pull-request-before-writing-335973",
            )
            .into());
        }
        self.verify_viewer()
    }
    pub fn get(&self, path: &str) -> Result<Value, String> {
        self.send("GET", path, None)
    }
    pub fn send(&self, method: &str, path: &str, body: Option<&Value>) -> Result<Value, String> {
        let bytes = self.bytes(method, path, body, "application/json")?;
        if bytes.is_empty() {
            Ok(Value::Null)
        } else {
            serde_json::from_slice(&bytes).map_err(|_| {
                crate::i18n::text("backend-provider-returned-invalid-json-2d3db8").into()
            })
        }
    }
    pub fn bytes(
        &self,
        method: &str,
        path: &str,
        body: Option<&Value>,
        accept: &str,
    ) -> Result<Vec<u8>, String> {
        if path.starts_with('/')
            || path.contains("://")
            || path.contains(['#', '\\'])
            || path.chars().any(char::is_control)
            || path
                .split('?')
                .next()
                .unwrap_or_default()
                .split('/')
                .any(|p| p == "." || p == "..")
        {
            return Err(crate::i18n::text("backend-invalid-provider-api-path-5358fa").into());
        }
        let path = if self.service == Service::Azure && !path.contains("api-version=") {
            format!(
                "{path}{}api-version=7.1",
                if path.contains('?') { '&' } else { '?' }
            )
        } else {
            path.into()
        };
        #[cfg(test)]
        if let Some(record) = &self.record {
            return record(method, &path, body).map(|v| {
                if accept == "text/plain" {
                    v.as_str().unwrap_or_default().as_bytes().to_vec()
                } else {
                    serde_json::to_vec(&v).unwrap_or_default()
                }
            });
        }
        if crate::tasks::cancellation().is_cancelled() {
            return Err(crate::i18n::text("backend-provider-request-cancelled-54eb9a").into());
        }
        let remaining = Duration::from_secs(120).saturating_sub(self.began.elapsed());
        if remaining.is_zero() || self.expires.is_some_and(|exp| now() + 15 >= exp) {
            return Err(crate::i18n::text(
                "backend-provider-session-expired-reload-to-acquire-the-selected-account-a-02409a",
            )
            .into());
        }
        let url = format!("{}/{path}", self.base);
        let req = self
            .agent
            .request(method, &url)
            .timeout(remaining.min(Duration::from_secs(30)))
            .set("Authorization", &self.authorization)
            .set("Accept", accept)
            .set("X-TFS-FedAuthRedirect", "Suppress");
        let response = match body {
            Some(v) => req
                .set("Content-Type", "application/json")
                .send_string(&v.to_string()),
            None => req.call(),
        }
        .map_err(|e| match e {
            ureq::Error::Status(s, _) => crate::i18n::message("backend-returned-http-s-check-selected-account-and-permissions-1327cc", &[("name", (self.service.name()).to_string().into()), ("status", (s).to_string().into())]),
            _ => crate::i18n::message("backend-transport-failed-c42359", &[("name", (self.service.name()).to_string().into()), ("if", (if method == "GET" {
                    ""
                } else {
                    crate::i18n::text("backend-the-write-outcome-is-uncertain-refresh-before-trying-again-c04611")
                }).to_string().into())]),
        })?;
        if !(200..300).contains(&response.status()) {
            return Err(crate::i18n::text(
                "backend-provider-redirect-refused-credentials-were-not-forwarded-71c0f0",
            )
            .into());
        }
        let mut bytes = vec![];
        response
            .into_reader()
            .take(8 * 1024 * 1024 + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| crate::i18n::text("backend-cannot-read-provider-response-6e29aa"))?;
        if bytes.len() > 8 * 1024 * 1024 {
            return Err(crate::i18n::text(
                "backend-provider-response-exceeds-8-mib-use-the-forge-page-e7a40f",
            )
            .into());
        }
        if crate::tasks::cancellation().is_cancelled() {
            return Err(crate::i18n::text(
                "backend-provider-request-cancelled-refresh-to-establish-any-write-outcome-70b94f",
            )
            .into());
        }
        Ok(bytes)
    }
    pub fn array(&self, path: &str) -> Result<Vec<Value>, String> {
        self.get(path)?
            .get("value")
            .and_then(Value::as_array)
            .cloned()
            .ok_or_else(|| {
                crate::i18n::text("backend-provider-returned-no-result-list-f70d77").into()
            })
    }
    #[cfg(test)]
    pub fn recording(service: Service, h: Hosting, viewer: &str, record: Recording) -> Self {
        Self {
            service,
            context: connections::current(),
            viewer: viewer.into(),
            hosting: h,
            base: String::new(),
            authorization: String::new(),
            agent: ureq::AgentBuilder::new().redirects(0).build(),
            began: Instant::now(),
            expires: None,
            record: Some(record),
        }
    }
}
fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |t| t.as_secs())
}
fn azure_token(a: &Account) -> Result<(String, Option<u64>), String> {
    let mut command = std::process::Command::new(connections::program("az"));
    azure_command(&mut command, a);
    crate::shell_path::apply(&mut command);
    let out = crate::processes::run(
        command,
        crate::processes::Options {
            background: true,
            timeout: Some(Duration::from_secs(30)),
            output_limit: 65536,
            stderr_limit: Some(65536),
            ..Default::default()
        },
    )?;
    if !out.status.success() {
        return Err(crate::i18n::text(
            "backend-selected-azure-cli-login-unavailable-sign-in-with-az-in-the-selec-0daa14",
        )
        .into());
    }
    let value: Value = serde_json::from_slice(&out.stdout).map_err(|_| {
        crate::i18n::text("backend-azure-cli-returned-invalid-token-metadata-d65a19")
    })?;
    parse_azure_token(&value, now())
}
pub(super) fn azure_command(c: &mut std::process::Command, a: &Account) {
    c.current_dir(std::env::temp_dir()).args([
        "account",
        "get-access-token",
        "--resource",
        "499b84ac-1321-427f-aa17-267ca6975798",
        "--output",
        "json",
    ]);
    for key in [
        "AZURE_DEVOPS_EXT_PAT",
        "AZURE_ACCESS_TOKEN",
        "AZURE_CLIENT_SECRET",
        "AZURE_FEDERATED_TOKEN_FILE",
        "AZURE_CONFIG_DIR",
    ] {
        c.env_remove(key);
    }
    c.env("AZURE_CORE_COLLECT_TELEMETRY", "false")
        .env("AZURE_CORE_ONLY_SHOW_ERRORS", "true");
    if !a.config_dir.is_empty() {
        c.env("AZURE_CONFIG_DIR", &a.config_dir);
    }
}
fn parse_azure_token(v: &Value, now: u64) -> Result<(String, Option<u64>), String> {
    let token = required(v, "/accessToken")?;
    let exp = v
        .get("expires_on")
        .and_then(|v| {
            v.as_u64()
                .or_else(|| v.as_str().and_then(|s| s.parse().ok()))
        })
        .ok_or(crate::i18n::text(
            "backend-azure-cli-must-provide-expires-on-update-az-or-use-an-explicitly-26eb57",
        ))?;
    if exp <= now + 15 || token.len() > 65536 || token.chars().any(char::is_whitespace) {
        return Err(crate::i18n::text(
            "backend-azure-cli-returned-an-expired-or-unusable-token-bdfc27",
        )
        .into());
    }
    Ok((format!("Bearer {token}"), Some(exp)))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn selected_authority_and_secure_reference_are_metadata_only() -> Result<(), String> {
        let a = Account {
            server: "https://dev.azure.com/team".into(),
            user: "viewer-id".into(),
            secret_ref: "profile-2".into(),
            pat: true,
            ..Default::default()
        };
        assert!(a.validate(Service::Azure).is_ok());
        assert!(
            Account {
                server: "https://dev.azure.com/team/foreign".into(),
                ..a.clone()
            }
            .validate(Service::Azure)
            .is_err()
        );
        assert!(
            Service::Bitbucket
                .server("https://bitbucket.org.evil")
                .is_err()
        );
        assert_eq!(
            Service::Azure.server("https://dev.azure.com/TEAM/")?,
            "https://dev.azure.com/team"
        );
        let mut other = a.clone();
        other.user = "another-viewer".into();
        assert_ne!(a.key(Service::Azure), other.key(Service::Azure));
        assert!(
            serde_json::to_value(&a)
                .map_err(|e| e.to_string())?
                .get("token")
                .is_none()
        );
        Ok(())
    }
    #[test]
    fn azure_command_and_expiry_are_recorded_without_process_execution() -> Result<(), String> {
        let a = Account {
            config_dir: "/recorded/azure-profile".into(),
            ..Default::default()
        };
        let mut c = std::process::Command::new("/recording/az");
        azure_command(&mut c, &a);
        assert_eq!(c.get_program(), "/recording/az");
        assert_eq!(
            c.get_args()
                .map(|a| a.to_string_lossy().into_owned())
                .collect::<Vec<_>>(),
            [
                "account",
                "get-access-token",
                "--resource",
                "499b84ac-1321-427f-aa17-267ca6975798",
                "--output",
                "json"
            ]
        );
        assert!(c.get_envs().any(|(k, v)| k == "AZURE_CONFIG_DIR"
            && v == Some(std::ffi::OsStr::new("/recorded/azure-profile"))));
        assert_eq!(
            parse_azure_token(
                &json!({"accessToken":"recorded-only","expires_on":200}),
                100
            )?
            .1,
            Some(200)
        );
        assert!(
            parse_azure_token(
                &json!({"accessToken":"recorded-only","expires_on":100}),
                100
            )
            .is_err()
        );
        assert!(parse_azure_token(&json!({"accessToken":"recorded-only"}), 100).is_err());
        Ok(())
    }
    #[test]
    fn native_viewer_context_and_endpoints_are_bound() -> Result<(), String> {
        let h = Hosting {
            forge: super::super::Forge::GitHub,
            host: "dev.azure.com".into(),
            web: "https://dev.azure.com".into(),
            path: "team/project/_git/repo".into(),
        };
        let api = Api::recording(
            Service::Azure,
            h,
            "viewer-id",
            std::sync::Arc::new(|method, path, _| {
                assert_eq!(method, "GET");
                assert!(path.starts_with("_apis/connectionData"));
                Ok(json!({"authenticatedUser":{"id":"viewer-id"}}))
            }),
        );
        assert!(api.verify_viewer().is_ok());
        assert!(api.get("https://foreign.invalid").is_err());
        assert!(api.get("../foreign").is_err());
        let mut changed = connections::current();
        changed.error = Some("changed".into());
        changed.run(|| assert!(api.guard_context().is_err()));
        Ok(())
    }
}
