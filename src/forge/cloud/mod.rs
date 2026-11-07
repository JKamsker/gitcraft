//! Captured, explicit authentication and bounded REST for the two cloud provider families.
#[cfg(test)]
mod tests;
mod transport;
use super::http::Login;
use crate::connections::{self, CloudAccount};
use serde_json::Value;
pub(super) use transport::Api;

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
    /// Only hosted service URLs are supported; an imported URL cannot choose a credential sink.
    pub fn server(self, value: &str) -> Result<String, String> {
        let value = value.trim_end_matches('/');
        match self {
            Self::Azure => {
                let org = value
                    .strip_prefix("https://dev.azure.com/")
                    .ok_or("Choose https://dev.azure.com/ORGANIZATION")?;
                if !plain(org) {
                    return Err("Choose one plain Azure organization name".into());
                }
                Ok(format!(
                    "https://dev.azure.com/{}",
                    org.to_ascii_lowercase()
                ))
            }
            Self::Bitbucket if value == "https://bitbucket.org" => Ok(value.into()),
            Self::Bitbucket => Err("Bitbucket Cloud accounts use https://bitbucket.org".into()),
        }
    }
    pub fn account(self, context: &connections::Context) -> &CloudAccount {
        match self {
            Self::Azure => &context.choices.azure,
            Self::Bitbucket => &context.choices.bitbucket,
        }
    }
}
pub(super) fn plain(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 256
        && !value.starts_with(['-', '.'])
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
}
pub(super) fn segment(value: &str) -> String {
    value
        .bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) {
                (b as char).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect()
}
fn azure_login(account: &CloudAccount) -> Result<Login, String> {
    let mut cmd = std::process::Command::new(connections::program("az"));
    crate::shell_path::apply(&mut cmd);
    cmd.current_dir(std::env::temp_dir()).args([
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
    ] {
        cmd.env_remove(key);
    }
    cmd.env("AZURE_CORE_COLLECT_TELEMETRY", "false")
        .env("AZURE_CORE_ONLY_SHOW_ERRORS", "true");
    if !account.config_dir.is_empty() {
        cmd.env("AZURE_CONFIG_DIR", &account.config_dir);
    }
    let out = crate::bounded_process::run_input(
        cmd,
        None,
        &|| true,
        std::time::Duration::from_secs(30),
        65536,
        65536,
    )
    .map_err(
        |_| "Could not read the selected Azure CLI account; sign in locally and check it again",
    )?;
    if !out.status.success() {
        return Err(
            "The selected Azure CLI account is unavailable; sign in locally and check it again"
                .into(),
        );
    }
    let value: Value = serde_json::from_slice(&out.stdout)
        .map_err(|_| "Azure CLI did not return an access token")?;
    let token = value
        .get("accessToken")
        .and_then(Value::as_str)
        .filter(|v| !v.is_empty() && v.len() <= 65536 && !v.chars().any(char::is_whitespace))
        .ok_or("Azure CLI did not return a usable access token")?;
    Ok(Login::Bearer(token.into()))
}
fn bitbucket_login(account: &CloudAccount) -> Result<Login, String> {
    let user = if account.credential_user.is_empty() {
        &account.user
    } else {
        &account.credential_user
    };
    let url = format!("https://{}@bitbucket.org", segment(user));
    let credentials=crate::git::provider_credentials(&url).map_err(|_|"No credential for the selected Bitbucket account was found in this machine's Git credential store")?;
    if credentials.username != *user {
        return Err("The credential store returned another Bitbucket username; choose the expected credential-store username explicitly".into());
    }
    Ok(Login::Basic {
        user: credentials.username,
        password: credentials.password,
    })
}
/// Fresh import checks verify server-reported ownership before enabling a pending profile.
pub(crate) fn check_connection(service: Service, account: &CloudAccount) -> Result<String, String> {
    account.validate(service)?;
    if !account.selected() {
        return Err("Choose an expected account name or ID first".into());
    }
    let mut context = connections::current();
    context.error = None;
    let api = context.run(|| Api::authenticated(service, account.clone(), None))?;
    Ok(format!("{} as {}", account.server, api.viewer))
}

/// An explicit local check records the immutable public owner for later exports and requests.
pub(crate) fn verified_account(
    service: Service,
    account: &CloudAccount,
) -> Result<CloudAccount, String> {
    account.validate(service)?;
    if !account.selected() {
        return Err("Choose the expected account first".into());
    }
    let mut context = connections::current();
    context.error = None;
    let api = context.run(|| Api::authenticated(service, account.clone(), None))?;
    let mut verified = account.clone();
    verified.subject = api.viewer_id;
    Ok(verified)
}
