//! A forge's REST API, called by gitcraft itself, for forges without a command-line tool that does
//! all it needs (Forgejo, Gitea). The login is one the user already has for the host, found by
//! [`super::gitea::find_login`]; gitcraft never stores one.

use std::time::Duration;

use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use serde_json::Value;

/// How a request proves who sends it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Login {
    /// An access token (`tea`'s, or one in the environment).
    Token(String),
    /// A name and a password or token, from git's credential helper.
    Basic { user: String, password: String },
}

impl Login {
    fn header(&self) -> String {
        match self {
            Login::Token(token) => format!("token {token}"),
            Login::Basic { user, password } => {
                format!("Basic {}", BASE64.encode(format!("{user}:{password}")))
            }
        }
    }
}

/// Why a request failed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Failure {
    /// The HTTP status, when the server answered.
    pub status: Option<u16>,
    pub message: String,
}

impl From<Failure> for String {
    fn from(failure: Failure) -> String {
        failure.message
    }
}

pub(super) struct Api {
    /// `https://host/api/v1`.
    base: String,
    /// "Forgejo", "Gitea": who errors come from.
    name: &'static str,
    host: String,
    login: Option<Login>,
    agent: ureq::Agent,
}

/// How many items a page holds when the API is asked for pages.
const PAGE: usize = 50;

impl Api {
    /// The API of the forge whose web pages are at `web` (`https://host[:port]`).
    pub fn new(name: &'static str, web: &str, host: &str, login: Option<Login>) -> Api {
        Api {
            base: format!("{}/api/v1", web.trim_end_matches('/')),
            name,
            host: host.to_owned(),
            login,
            agent: ureq::AgentBuilder::new()
                .timeout(Duration::from_secs(20))
                .user_agent(concat!("gitcraft/", env!("CARGO_PKG_VERSION")))
                .build(),
        }
    }

    /// Whether a login goes along with the requests.
    pub fn logged_in(&self) -> bool {
        self.login.is_some()
    }

    pub fn get(&self, endpoint: &str) -> Result<Value, Failure> {
        self.send("GET", endpoint, None)
    }

    /// Every item of a list the API gives page by page, up to `max`.
    pub fn get_all(&self, endpoint: &str, max: usize) -> Result<Vec<Value>, Failure> {
        let joint = if endpoint.contains('?') { '&' } else { '?' };
        let mut all: Vec<Value> = Vec::new();
        for page in 1.. {
            let answer = self.get(&format!("{endpoint}{joint}limit={PAGE}&page={page}"))?;
            let items = match answer {
                Value::Array(items) => items,
                Value::Null => Vec::new(),
                _ => return Err(self.unexpected("a list")),
            };
            // An endpoint that doesn't page gives everything every time: the first page is it.
            let repeats = page > 1 && !items.is_empty() && items.first() == all.first();
            let last = items.len() < PAGE || repeats;
            if !repeats {
                all.extend(items);
            }
            if last || all.len() >= max {
                break;
            }
        }
        all.truncate(max);
        Ok(all)
    }

    /// Sends `payload` (as JSON) with `method` to `endpoint`, and returns the answer; an empty
    /// answer is `null`.
    pub fn send(
        &self,
        method: &str,
        endpoint: &str,
        payload: Option<&Value>,
    ) -> Result<Value, Failure> {
        let url = format!("{}/{}", self.base, endpoint.trim_start_matches('/'));
        let mut request = self
            .agent
            .request(method, &url)
            .set("Accept", "application/json");
        if let Some(login) = &self.login {
            request = request.set("Authorization", &login.header());
        }
        let answer = match payload {
            Some(payload) => request
                .set("Content-Type", "application/json")
                .send_string(&payload.to_string()),
            None => request.call(),
        };
        match answer {
            Ok(response) => {
                let text = response.into_string().map_err(|e| Failure {
                    status: None,
                    message: e.to_string(),
                })?;
                if text.trim().is_empty() {
                    return Ok(Value::Null);
                }
                serde_json::from_str(&text).map_err(|_| self.unexpected("JSON"))
            }
            Err(ureq::Error::Status(status, response)) => {
                let text = response.into_string().unwrap_or_default();
                Err(Failure {
                    status: Some(status),
                    message: self.explain(status, &text),
                })
            }
            Err(ureq::Error::Transport(e)) => Err(Failure {
                status: None,
                message: format!("couldn't reach {}: {e}", self.host),
            }),
        }
    }

    fn unexpected(&self, what: &str) -> Failure {
        Failure {
            status: None,
            message: format!("{} didn't answer with {what}", self.name),
        }
    }

    /// What a refusal means, in a sentence.
    fn explain(&self, status: u16, body: &str) -> String {
        let said = error_message(body);
        let said = if said.is_empty() {
            format!("HTTP {status}")
        } else {
            format!("{said} (HTTP {status})")
        };
        match (status, &self.login) {
            (401 | 403, None) => format!(
                "{}: {said}. gitcraft found no login for {}: add one with `tea login add`, or \
                 let git's credential helper keep one for https://{}",
                self.name, self.host, self.host
            ),
            (401, Some(_)) => format!(
                "{}: {said}. The login gitcraft found for {} was refused",
                self.name, self.host
            ),
            _ => format!("{}: {said}", self.name),
        }
    }
}

/// What an error answer says: `{"message": "..."}`, sometimes with `errors` beside it.
fn error_message(body: &str) -> String {
    let Ok(v) = serde_json::from_str::<Value>(body) else {
        return body.trim().chars().take(200).collect();
    };
    let message = v.get("message").and_then(Value::as_str).unwrap_or("");
    let errors: Vec<&str> = v
        .get("errors")
        .and_then(Value::as_array)
        .map(|e| e.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();
    match (message.is_empty(), errors.is_empty()) {
        (_, true) => message.to_owned(),
        (true, false) => errors.join("; "),
        (false, false) => format!("{message}: {}", errors.join("; ")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_token_and_a_password_go_as_their_own_kind() {
        assert_eq!(Login::Token("t0k".into()).header(), "token t0k");
        let basic = Login::Basic {
            user: "ana".into(),
            password: "pw".into(),
        };
        assert_eq!(basic.header(), "Basic YW5hOnB3");
    }

    #[test]
    fn reads_what_an_error_says() {
        assert_eq!(
            error_message(r#"{"message":"not found","url":"u"}"#),
            "not found"
        );
        assert_eq!(
            error_message(r#"{"message":"invalid","errors":["title is empty"]}"#),
            "invalid: title is empty"
        );
        assert_eq!(
            error_message("<html>Bad gateway</html>"),
            "<html>Bad gateway</html>"
        );
    }

    #[test]
    fn a_refusal_without_a_login_says_how_to_get_one() {
        let api = Api::new("Forgejo", "https://codeberg.org", "codeberg.org", None);
        let said = api.explain(401, r#"{"message":"token is required"}"#);
        assert!(
            said.starts_with("Forgejo: token is required (HTTP 401)"),
            "{said}"
        );
        assert!(said.contains("tea login add"), "{said}");
        let api = Api::new(
            "Forgejo",
            "https://codeberg.org",
            "codeberg.org",
            Some(Login::Token("x".into())),
        );
        assert!(api.explain(401, "").contains("was refused"));
        assert_eq!(api.explain(409, ""), "Forgejo: HTTP 409");
    }
}
