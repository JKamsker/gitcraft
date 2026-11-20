//! SonarQube Server (self-hosted) and SonarQube Cloud: the profile's connection, its token in
//! the OS credential store, and the read-only Web API client.
//!
//! API references (verified against the servers' own `api/webservices/list`):
//! - Server: <https://docs.sonarsource.com/sonarqube-server/latest/extension-guide/web-api/>
//! - Cloud: <https://docs.sonarsource.com/sonarqube-cloud/advanced-setup/web-api/>

mod client;
mod parse;
mod transport;
pub(crate) mod ui;

#[cfg(test)]
mod account_tests;
#[cfg(test)]
mod client_tests;
#[cfg(test)]
mod parse_tests;
#[cfg(test)]
mod transport_tests;
#[cfg(test)]
mod ui_tests;

pub(in crate::analysis) use transport::for_account;
pub(crate) use transport::live;

use serde::{Deserialize, Serialize};
use serde_json::json;

use super::Failure;

/// The credential-store service; distinct from forge and tracker secrets.
const SERVICE: &str = "gitcraft-analysis";

/// SonarQube Cloud's regional origins (Web API v1 hosts, per the Cloud Web API page).
pub(in crate::analysis) const CLOUD_ORIGINS: [&str; 3] = [
    "https://sonarcloud.io",
    "https://sonarqube.us",
    "https://sonarqube.au",
];

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub(in crate::analysis) enum Edition {
    /// A self-hosted SonarQube Server (or Community Build).
    #[default]
    Server,
    /// SonarQube Cloud (formerly SonarCloud).
    Cloud,
}

/// Profile metadata only. The token lives in the credential store under [`Account::store_key`];
/// exported profile settings carry the reference, never the secret.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Account {
    pub(in crate::analysis) edition: Edition,
    /// Server: its base URL including any context path (`https://host/sonarqube`).
    /// Cloud: one of [`CLOUD_ORIGINS`].
    pub origin: String,
    /// SonarQube Cloud organization key; empty for a Server.
    pub organization: String,
    pub secret_ref: String,
}

impl Account {
    /// A Server connection, for tests outside this module.
    #[cfg(test)]
    pub(crate) fn server_for_tests(origin: &str, secret_ref: &str) -> Self {
        Self {
            origin: origin.into(),
            secret_ref: secret_ref.into(),
            ..Self::default()
        }
    }

    /// Whether the profile has a connection at all; an empty one is valid and inert.
    pub(crate) fn configured(&self) -> bool {
        !self.origin.trim().is_empty()
    }

    /// The base URL requests go to, without a trailing slash.
    pub(in crate::analysis) fn base(&self) -> &str {
        self.origin.trim().trim_end_matches('/')
    }

    /// Which connection a repository binding was made for (no secret involved).
    pub(crate) fn identity(&self) -> String {
        json!([self.edition, self.base(), self.organization.trim()]).to_string()
    }

    /// Checks the connection; the error is a Fluent message id.
    pub(crate) fn validate(&self) -> Result<(), &'static str> {
        if !self.configured() {
            // Inert: no request is ever made without an origin.
            return Ok(());
        }
        if [&self.origin, &self.organization, &self.secret_ref]
            .iter()
            .any(|v| v.chars().any(char::is_control))
        {
            return Err("analysis-connection-control-characters");
        }
        if !origin_is_safe(self.base()) {
            return Err("analysis-connection-origin");
        }
        match self.edition {
            Edition::Server if !self.organization.is_empty() => {
                Err("analysis-connection-server-organization")
            }
            Edition::Server => Ok(()),
            Edition::Cloud if !CLOUD_ORIGINS.contains(&self.base()) => {
                Err("analysis-connection-cloud-origin")
            }
            Edition::Cloud if !organization_key(&self.organization) => {
                Err("analysis-connection-organization")
            }
            Edition::Cloud => Ok(()),
        }
    }

    fn store_key(&self) -> String {
        json!([self.base(), self.organization.trim(), self.secret_ref]).to_string()
    }

    fn entry(&self) -> Result<keyring::Entry, Failure> {
        if self.secret_ref.is_empty() || !crate::portable::credential_store() {
            return Err(Failure::MissingToken);
        }
        keyring::Entry::new(SERVICE, &self.store_key()).map_err(|_| Failure::MissingToken)
    }

    /// Saves `token` for this connection. The error is a Fluent message id, so no error can
    /// contain the token.
    pub(in crate::analysis) fn store(&self, token: &str) -> Result<(), &'static str> {
        self.validate()?;
        if !self.configured() || self.secret_ref.is_empty() || !token_is_plausible(token) {
            return Err("analysis-token-required");
        }
        if !crate::portable::credential_store() {
            return Err("analysis-token-portable");
        }
        self.entry()
            .and_then(|e| {
                e.set_password(token.trim())
                    .map_err(|_| Failure::MissingToken)
            })
            .map_err(|_| "analysis-token-store-failed")
    }

    pub(in crate::analysis) fn forget(&self) -> Result<(), &'static str> {
        self.entry()
            .and_then(|e| e.delete_credential().map_err(|_| Failure::MissingToken))
            .map_err(|_| "analysis-token-forget-failed")
    }

    fn secret(&self) -> Result<String, Failure> {
        self.entry()?
            .get_password()
            .map_err(|_| Failure::MissingToken)
            .and_then(|t| {
                if token_is_plausible(&t) {
                    Ok(t)
                } else {
                    Err(Failure::MissingToken)
                }
            })
    }
}

/// HTTPS, or plain HTTP to this machine only (a local SonarQube on `http://localhost:9000`):
/// a token must never cross a network unencrypted. No credentials, query or fragment.
fn origin_is_safe(origin: &str) -> bool {
    let Ok(url) = url::Url::parse(origin) else {
        return false;
    };
    let loopback = match url.host() {
        Some(url::Host::Domain(d)) => d.eq_ignore_ascii_case("localhost"),
        Some(url::Host::Ipv4(ip)) => ip.is_loopback(),
        Some(url::Host::Ipv6(ip)) => ip.is_loopback(),
        None => return false,
    };
    (url.scheme() == "https" || (url.scheme() == "http" && loopback))
        && url.username().is_empty()
        && url.password().is_none()
        && url.query().is_none()
        && url.fragment().is_none()
}

fn organization_key(key: &str) -> bool {
    !key.is_empty()
        && key.len() <= 255
        && key
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_'))
}

fn token_is_plausible(token: &str) -> bool {
    let token = token.trim();
    !token.is_empty()
        && token.len() <= 512
        && !token.chars().any(|c| c.is_control() || c.is_whitespace())
}

/// A SonarQube project key: letters, digits, `-`, `_`, `.` and `:`, with at least one
/// non-digit, at most 400 characters (the server's own rule).
pub(crate) fn project_key(key: &str) -> bool {
    !key.is_empty()
        && key.len() <= 400
        && key
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b':'))
        && !key.bytes().all(|b| b.is_ascii_digit())
}
