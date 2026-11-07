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
    /// `https://host[:port]`, as the login was found for.
    web: String,
    login: Option<Login>,
    /// Told the server's `web` when it refused the login, so that it isn't sent again.
    on_refused: Option<fn(&str)>,
    refused_key: Option<String>,
    agent: ureq::Agent,
}

/// How many items a page holds when the API is asked for pages.
const PAGE: usize = 50;

impl Api {
    /// The API of the forge whose web pages are at `web` (`https://host[:port]`).
    #[cfg(test)]
    pub(super) fn fixture(base: &str, login: Login) -> Self {
        let mut api = Self::new("fixture", base, "127.0.0.1", Some(login));
        api.agent = ureq::AgentBuilder::new()
            .timeout(Duration::from_secs(3))
            .redirects(0)
            .build();
        api
    }
    pub fn new(name: &'static str, web: &str, host: &str, login: Option<Login>) -> Api {
        Api {
            base: format!("{}/api/v1", web.trim_end_matches('/')),
            name,
            host: host.to_owned(),
            web: web.trim_end_matches('/').to_owned(),
            login,
            on_refused: None,
            refused_key: None,
            agent: with_proxy(ureq::AgentBuilder::new(), web)
                // A large repository on a busy server can take its time over a page of the list.
                .timeout(Duration::from_secs(30))
                // A redirect turns a POST into a GET and drops the login: what was sent would read
                // as done. It's said instead (see `send`).
                .redirects(0)
                .user_agent(concat!("gitcraft/", env!("CARGO_PKG_VERSION")))
                .build(),
        }
    }

    pub fn on_refused_key(mut self, forget: fn(&str), key: &str) -> Api {
        self.on_refused = Some(forget);
        self.refused_key = Some(key.to_owned());
        self
    }
    pub fn name(&self) -> &'static str {
        self.name
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
        self.get_all_kept(endpoint, max, |_| true)
    }

    /// [`Api::get_all`], of the items `keep` keeps: the others don't count toward `max`.
    pub fn get_all_kept(
        &self,
        endpoint: &str,
        max: usize,
        keep: impl Fn(&Value) -> bool,
    ) -> Result<Vec<Value>, Failure> {
        let joint = if endpoint.contains('?') { '&' } else { '?' };
        let mut all: Vec<Value> = Vec::new();
        let mut first: Option<Value> = None;
        for page in 1.. {
            let answer = self.get(&format!("{endpoint}{joint}limit={PAGE}&page={page}"))?;
            let items = match answer {
                Value::Array(items) => items,
                Value::Null => Vec::new(),
                _ => return Err(self.unexpected("a list")),
            };
            // An endpoint that doesn't page gives everything every time: the first page is it.
            // A short page isn't the last one: the server's own cap on a page may be below ours.
            let repeats = page > 1 && !items.is_empty() && items.first() == first.as_ref();
            let last = items.is_empty() || repeats;
            if page == 1 {
                first = items.first().cloned();
            }
            if !repeats {
                all.extend(items.into_iter().filter(|item| keep(item)));
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
        self.take(answer)
    }

    /// Sends the file `name` (`bytes`) as `multipart/form-data`, the field named `field`, to
    /// `endpoint` (a release's assets).
    pub fn upload(
        &self,
        endpoint: &str,
        field: &str,
        name: &str,
        bytes: &[u8],
    ) -> Result<Value, Failure> {
        let url = format!("{}/{}", self.base, endpoint.trim_start_matches('/'));
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos());
        let boundary = format!("gitcraft-{stamp:x}-{:x}", bytes.len());
        // The name as the header can carry it: no quote, no line break.
        let safe: String = name
            .chars()
            .map(|c| if c == '"' || c.is_control() { '_' } else { c })
            .collect();
        let mut body = format!(
            "--{boundary}\r\nContent-Disposition: form-data; name=\"{field}\"; \
             filename=\"{safe}\"\r\nContent-Type: application/octet-stream\r\n\r\n"
        )
        .into_bytes();
        body.extend_from_slice(bytes);
        body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
        // A big file takes its time: half a minute, and more for each 50 KB.
        let time = std::time::Duration::from_secs(30 + bytes.len() as u64 / 51_200);
        let mut request = self
            .agent
            .request("POST", &url)
            .timeout(time)
            .set("Accept", "application/json")
            .set(
                "Content-Type",
                &format!("multipart/form-data; boundary={boundary}"),
            );
        if let Some(login) = &self.login {
            request = request.set("Authorization", &login.header());
        }
        self.take(request.send_bytes(&body))
    }

    /// What an answer of the server says: its JSON, or why it failed.
    fn take(&self, answer: Result<ureq::Response, ureq::Error>) -> Result<Value, Failure> {
        match answer {
            Ok(response) if (300..400).contains(&response.status()) => {
                let to = response
                    .header("location")
                    .unwrap_or("elsewhere")
                    .to_owned();
                Err(Failure {
                    status: Some(response.status()),
                    message: format!(
                        "{} sent gitcraft to {to}: set the remote to the server's own address",
                        self.host
                    ),
                })
            }
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
                if status == 401
                    && self.login.is_some()
                    && let Some(forget) = self.on_refused
                {
                    forget(self.refused_key.as_deref().unwrap_or(&self.web));
                }
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
            // A private repository is "not found" to whoever isn't known.
            (404, None) => format!(
                "{}: {said}. If the repository is private, gitcraft found no login for {}: add one \
                 with `tea login add`, or let git's credential helper keep one for https://{}",
                self.name, self.host, self.host
            ),
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

/// `builder` going through the proxy the environment sets for `web`, as git and curl pick it.
pub(crate) fn with_proxy(builder: ureq::AgentBuilder, web: &str) -> ureq::AgentBuilder {
    let proxy = proxy_for(web, |name| {
        std::env::var(name).ok().filter(|v| !v.is_empty())
    });
    match proxy.and_then(|url| ureq::Proxy::new(url).ok()) {
        Some(proxy) => builder.proxy(proxy),
        None => builder,
    }
}

/// The proxy for `web` (`https://host[:port]`): the scheme's own variable, else `ALL_PROXY`, unless
/// `NO_PROXY` names the host (`*`, a domain and its subdomains, an IPv4 range like `10.0.0.0/8`).
fn proxy_for(web: &str, var: impl Fn(&str) -> Option<String>) -> Option<String> {
    let (scheme, rest) = web.split_once("://")?;
    let authority = rest.split('/').next().unwrap_or(rest);
    let host = match authority.find(']') {
        Some(end) if authority.starts_with('[') => &authority[1..end],
        _ => authority.split(':').next().unwrap_or(authority),
    }
    .to_lowercase();
    let first = |names: &[&str]| names.iter().find_map(|name| var(name));
    let excluded = first(&["no_proxy", "NO_PROXY"]).unwrap_or_default();
    if excluded
        .split(',')
        .map(|entry| entry.trim().to_lowercase())
        .any(|entry| bypasses(&entry, &host))
    {
        return None;
    }
    match scheme {
        "https" => first(&["https_proxy", "HTTPS_PROXY", "all_proxy", "ALL_PROXY"]),
        _ => first(&["http_proxy", "HTTP_PROXY", "all_proxy", "ALL_PROXY"]),
    }
}

/// Whether one `NO_PROXY` entry covers `host`.
fn bypasses(entry: &str, host: &str) -> bool {
    if entry == "*" {
        return true;
    }
    if let Some((net, bits)) = entry.split_once('/') {
        let (Ok(net), Ok(bits), Ok(ip)) = (
            net.parse::<std::net::Ipv4Addr>(),
            bits.parse::<u32>(),
            host.parse::<std::net::Ipv4Addr>(),
        ) else {
            return false;
        };
        let mask = u32::MAX.checked_shl(32 - bits.min(32)).unwrap_or(0);
        return u32::from(net) & mask == u32::from(ip) & mask;
    }
    let entry = entry.trim_start_matches("*.").trim_start_matches('.');
    let entry = match entry.find(']') {
        Some(end) if entry.starts_with('[') => &entry[1..end],
        _ if entry.matches(':').count() == 1 => entry.split(':').next().unwrap_or(entry),
        _ => entry,
    };
    !entry.is_empty() && (host == entry || host.ends_with(&format!(".{entry}")))
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
    fn the_proxy_is_the_schemes_unless_no_proxy_names_the_host() {
        let env = |pairs: &'static [(&'static str, &'static str)]| {
            move |name: &str| {
                pairs
                    .iter()
                    .find(|(k, _)| *k == name)
                    .map(|(_, v)| (*v).to_owned())
            }
        };
        let vars = env(&[
            ("HTTPS_PROXY", "http://proxy:8080"),
            ("HTTP_PROXY", "http://plain:3128"),
            (
                "NO_PROXY",
                "localhost, .corp.example,git.lan:3000,10.0.0.0/8,::1",
            ),
        ]);
        let proxy = |web: &str| proxy_for(web, vars);
        assert_eq!(
            proxy("https://codeberg.org").as_deref(),
            Some("http://proxy:8080")
        );
        assert_eq!(
            proxy("http://codeberg.org").as_deref(),
            Some("http://plain:3128")
        );
        for direct in [
            "http://localhost:3000",
            "https://git.corp.example",
            "https://corp.example",
            "http://git.lan:3000",
            "http://10.1.2.3:3000",
            "http://[::1]:3000",
        ] {
            assert_eq!(proxy(direct), None, "{direct}");
        }
        assert_eq!(
            proxy("https://notcorp.example").as_deref(),
            Some("http://proxy:8080")
        );
        assert_eq!(
            proxy("http://11.1.2.3").as_deref(),
            Some("http://plain:3128")
        );
        let everything = env(&[("HTTPS_PROXY", "http://p"), ("no_proxy", "*")]);
        assert_eq!(proxy_for("https://codeberg.org", everything), None);
        let all = env(&[("ALL_PROXY", "http://all")]);
        assert_eq!(
            proxy_for("https://x.org", all).as_deref(),
            Some("http://all")
        );
        assert_eq!(proxy_for("https://x.org", env(&[])), None);
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
