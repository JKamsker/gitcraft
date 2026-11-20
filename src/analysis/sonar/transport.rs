//! Live HTTP for the Sonar client: the shared ureq stack with gitcraft's proxy selection and the
//! system certificate store (custom CAs), bounded time and size, no redirects. The token is
//! held only by the request closure; failures are typed and never carry server text.

use std::io::Read;
use std::time::Duration;

use base64::Engine;

use super::super::{Failure, Provider};
use super::Account;
use super::client::{Auth, Client};

/// Per request; a quality-gate refresh makes at most eight.
pub(super) const TIMEOUT: Duration = Duration::from_secs(15);
/// The largest answer read; the lists used are far smaller.
const MAX_BODY: u64 = 4 * 1024 * 1024;

pub(in crate::analysis) type Sender = Box<dyn FnMut(&str, Auth) -> Result<String, Failure> + Send>;

/// A client for the connection of the profile the calling worker runs for.
pub(crate) fn live() -> Result<impl Provider, Failure> {
    let context = crate::connections::current();
    if context.error.is_some() {
        return Err(Failure::InvalidConnection);
    }
    for_account(&context.choices.sonar)
}

/// A client for `account` as edited (the connection check in settings).
pub(in crate::analysis) fn for_account(account: &Account) -> Result<Client<Sender>, Failure> {
    if !account.configured() {
        return Err(Failure::NotConfigured);
    }
    account.validate().map_err(|_| Failure::InvalidConnection)?;
    let token = account.secret()?;
    Ok(Client::new(
        account.clone(),
        sender(account.base().to_owned(), token, TIMEOUT),
    ))
}

pub(super) fn sender(base: String, token: String, timeout: Duration) -> Sender {
    let agent = crate::forge::http::with_proxy(ureq::AgentBuilder::new(), &base)
        // A redirect could replay the token to another origin: refused instead.
        .redirects(0)
        .timeout(timeout)
        .user_agent(concat!("gitcraft/", env!("CARGO_PKG_VERSION")))
        .build();
    let basic = format!(
        "Basic {}",
        base64::engine::general_purpose::STANDARD.encode(format!("{token}:"))
    );
    let bearer = format!("Bearer {token}");
    drop(token);
    let cancel = crate::tasks::cancellation();
    Box::new(move |path: &str, auth: Auth| {
        if cancel.is_cancelled() {
            return Err(Failure::Cancelled);
        }
        if !path.starts_with("api/") || path.contains("://") || path.chars().any(char::is_control) {
            return Err(Failure::Malformed);
        }
        let mut request = agent
            .get(&format!("{base}/{path}"))
            .set("Accept", "application/json, text/plain");
        match auth {
            Auth::None => {}
            Auth::Basic => request = request.set("Authorization", &basic),
            Auth::Bearer => request = request.set("Authorization", &bearer),
        }
        let response = request.call().map_err(|e| failure(&e))?;
        if !(200..300).contains(&response.status()) {
            return Err(if (300..400).contains(&response.status()) {
                Failure::Redirected
            } else {
                Failure::Http(response.status())
            });
        }
        let mut bytes = Vec::new();
        response
            .into_reader()
            .take(MAX_BODY + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| io_failure(&e))?;
        if cancel.is_cancelled() {
            return Err(Failure::Cancelled);
        }
        if bytes.len() as u64 > MAX_BODY {
            return Err(Failure::Malformed);
        }
        String::from_utf8(bytes).map_err(|_| Failure::Malformed)
    })
}

fn failure(error: &ureq::Error) -> Failure {
    match error {
        ureq::Error::Status(401, _) => Failure::Unauthorized,
        ureq::Error::Status(403, _) => Failure::Forbidden,
        ureq::Error::Status(429, response) => Failure::RateLimited(
            response
                .header("retry-after")
                .and_then(|s| s.trim().parse().ok()),
        ),
        ureq::Error::Status(status, _) => Failure::Http(*status),
        ureq::Error::Transport(transport) => {
            use std::error::Error;
            match transport
                .source()
                .and_then(|s| s.downcast_ref::<std::io::Error>())
            {
                Some(io) => io_failure(io),
                None => Failure::Unreachable,
            }
        }
    }
}

fn io_failure(error: &std::io::Error) -> Failure {
    match error.kind() {
        std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock => Failure::Timeout,
        _ => Failure::Unreachable,
    }
}
