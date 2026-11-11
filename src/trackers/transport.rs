//! Bind live issue-tracker requests to the selected profile and transport.
use super::*;

pub(super) fn live(kind: Kind) -> Result<Client<SendRequest>, String> {
    let context = connections::current();
    if let Some(e) = context.error {
        return Err(e);
    }
    let account = kind.account(&context).clone();
    account.validate(kind)?;
    let token = account.secret()?;
    use base64::Engine;
    let auth = match kind {
        Kind::Jira => format!(
            "Basic {}",
            base64::engine::general_purpose::STANDARD.encode(format!("{}:{token}", account.user))
        ),
        Kind::Linear => token,
    };
    let origin = account.origin.trim_end_matches('/').to_owned();
    let agent = crate::forge::http::with_proxy(ureq::AgentBuilder::new(), &origin)
        .redirects(0)
        .timeout(Duration::from_secs(30))
        .build();
    let cancel = crate::tasks::cancellation();
    Ok(Client {
        kind,
        account,
        send: Box::new(move |method: &str, path: &str, body: Option<&Value>| {
            if cancel.is_cancelled() {
                return Err("Tracker request cancelled".into());
            }
            if !path.starts_with('/') || path.starts_with("//") || path.contains(['\r', '\n']) {
                return Err("Invalid tracker API path".into());
            }
            let req = agent
                .request(method, &format!("{origin}{path}"))
                .set("Authorization", &auth)
                .set("Accept", "application/json");
            let response = match body {
                Some(v) => req
                    .set("Content-Type", "application/json")
                    .send_string(&v.to_string()),
                None => req.call(),
            }
            .map_err(|e| match e {
                ureq::Error::Status(s, _) => format!(
                    "{} returned HTTP {s}; check the selected account, permissions and rate limits",
                    kind.name()
                ),
                _ => format!(
                    "{} request failed; check connectivity and the selected origin",
                    kind.name()
                ),
            })?;
            if !(200..300).contains(&response.status()) {
                return Err(format!(
                    "{} returned HTTP {}; redirects are refused",
                    kind.name(),
                    response.status()
                ));
            }
            let mut bytes = Vec::new();
            response
                .into_reader()
                .take(8 * 1024 * 1024 + 1)
                .read_to_end(&mut bytes)
                .map_err(|_| "Cannot read tracker response")?;
            if cancel.is_cancelled() {
                return Err("Tracker request cancelled".into());
            }
            if bytes.len() > 8 * 1024 * 1024 {
                return Err("Tracker response exceeds 8 MiB".into());
            }
            serde_json::from_slice(&bytes).map_err(|_| "Tracker returned invalid JSON".into())
        }),
    })
}

pub(crate) fn assigned(kind: Kind) -> Result<Vec<Issue>, String> {
    live(kind)?.listed()
}

pub(crate) fn one(identity: &Identity) -> Result<Issue, String> {
    live(identity.kind)?.one(identity)
}
