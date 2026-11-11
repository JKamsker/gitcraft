//! What a start of gitcraft says to the one already running, and what that one answers: one
//! line of JSON each way.
//!
//! A request is `{"v":1,"token":"…","kind":"…", …}`: the protocol's version, the running
//! instance's token, what is asked, and what that kind of request needs (`"path"` for `open`).
//! The answer is `{"v":1,"status":"ok"}`, `{"v":1,"status":"refused","error":"…"}` or
//! `{"v":1,"status":"closing"}` (the window is closing: the request was not taken).
//!
//! A new kind of request is a new [`Request`] variant with its word in [`Request::kind`] and its
//! fields read in [`decode_request`]; an older instance refuses a kind it doesn't know (`refused`,
//! saying so), so a newer client can tell. Nothing in a request is ever run: a path is only
//! opened as a repository, and only once it is found well formed.

use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use crate::mcp::wire::{self, WriteOp};

/// The protocol spoken.
pub const VERSION: u64 = 1;

/// The longest line read, request or answer, its `\n` included. Anything can connect to the
/// port: what is longer is no gitcraft's, and is dropped before it is held whole.
pub const MAX_LINE: u64 = 64 * 1024;

/// The longest path handed over (Windows' own limit, with room for `ssh://host/`).
const MAX_PATH: usize = 33 * 1024;

/// What a start asks of the running instance.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Request {
    /// Come to the front.
    Focus,
    /// Open (or switch to) the repository at this path, then come to the front: an absolute
    /// folder of this machine, or one elsewhere (`ssh://…`, `\\wsl.localhost\…`).
    Open(PathBuf),
    /// An agent's change, from `gitcraft mcp` (see `crate::mcp`): run in the tab of its
    /// repository, answered once done (see `server`).
    Agent(AgentCall),
    /// Do what the command line `args` asks (`log <file>`, `blame <file>:<line>`, see
    /// `crate::launch`), its relative paths read against `cwd`, the start's folder. Read
    /// again on arrival, as the start read them: only a request `crate::launch` takes is
    /// taken.
    Launch { cwd: PathBuf, args: Vec<String> },
}

/// What an agent asks the window to do: `op` in the repository `repo` (as a tab names it).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentCall {
    pub repo: String,
    /// Who asks, as the agent names itself.
    pub agent: String,
    pub op: WriteOp,
}

impl Request {
    fn kind(&self) -> &'static str {
        match self {
            Request::Focus => "focus",
            Request::Open(_) => "open",
            Request::Agent(_) => "agent",
            Request::Launch { .. } => "launch",
        }
    }
}

/// Why a request was not taken.
#[derive(Debug, PartialEq, Eq)]
pub enum Refusal {
    /// Not JSON, not an object, or without a token: no gitcraft's. Left unanswered.
    Malformed,
    /// The token isn't this instance's. Left unanswered: whoever sent it learns nothing.
    Stranger,
    /// From a gitcraft, but not one this instance can take; answered with this message.
    Unusable(String),
}

/// The answer to a request.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Reply {
    /// Taken: the window does it.
    Ok,
    /// Not taken, and why.
    Refused(String),
    /// The window is closing; the request was not taken.
    Closing,
    /// How an agent's call ended.
    Agent(wire::Reply),
}

/// `request` for the instance whose token is `token`, as the line sent.
pub fn encode_request(token: &str, request: &Request) -> String {
    let mut value = json!({ "v": VERSION, "token": token, "kind": request.kind() });
    if let Some(object) = value.as_object_mut() {
        match request {
            Request::Launch { cwd, args } => {
                object.insert("cwd".to_owned(), cwd.to_string_lossy().into());
                object.insert("args".to_owned(), args.clone().into());
            }
            Request::Focus => {}
            Request::Open(path) => {
                object.insert("path".to_owned(), path.to_string_lossy().into());
            }
            Request::Agent(call) => {
                object.insert("repo".to_owned(), call.repo.clone().into());
                object.insert("agent".to_owned(), call.agent.clone().into());
                object.insert(
                    "op".to_owned(),
                    serde_json::to_value(&call.op).unwrap_or_default(),
                );
            }
        }
    }
    format!("{value}\n")
}

/// The request in `line`, for the instance whose token is `token`.
pub fn decode_request(line: &str, token: &str) -> Result<Request, Refusal> {
    let value: Value = serde_json::from_str(line.trim_end()).map_err(|_| Refusal::Malformed)?;
    let object = value.as_object().ok_or(Refusal::Malformed)?;
    let sent = object
        .get("token")
        .and_then(Value::as_str)
        .ok_or(Refusal::Malformed)?;
    // Only a start of this user's knows the token: checked before anything else is told.
    if !crate::remote::askpass::same(sent.as_bytes(), token.as_bytes()) {
        return Err(Refusal::Stranger);
    }
    match object.get("v").and_then(Value::as_u64) {
        Some(VERSION) => {}
        Some(v) => {
            return Err(Refusal::Unusable(format!(
                "this gitcraft speaks version {VERSION}, not {v}"
            )));
        }
        None => return Err(Refusal::Unusable("no version".to_owned())),
    }
    match object.get("kind").and_then(Value::as_str) {
        Some("focus") => Ok(Request::Focus),
        Some("open") => {
            let path = object.get("path").and_then(Value::as_str).unwrap_or("");
            valid_path(path)
                .map(Request::Open)
                .map_err(Refusal::Unusable)
        }
        Some("agent") => decode_agent(object).map(Request::Agent),
        Some("launch") => {
            let cwd = object.get("cwd").and_then(Value::as_str).unwrap_or("");
            let cwd = valid_path(cwd).map_err(Refusal::Unusable)?;
            let args = object
                .get("args")
                .and_then(Value::as_array)
                .ok_or_else(|| Refusal::Unusable("no command line".to_owned()))?
                .iter()
                .map(|a| a.as_str().map(str::to_owned))
                .collect::<Option<Vec<String>>>()
                .ok_or_else(|| Refusal::Unusable("the command line isn't text".to_owned()))?;
            valid_args(&args).map_err(Refusal::Unusable)?;
            Ok(Request::Launch { cwd, args })
        }
        Some(kind) => Err(Refusal::Unusable(format!(
            "this gitcraft can't do \"{kind}\""
        ))),
        None => Err(Refusal::Unusable("no kind".to_owned())),
    }
}

/// An agent's call: the repository (named as a tab names it), who asks, and what.
fn decode_agent(object: &serde_json::Map<String, Value>) -> Result<AgentCall, Refusal> {
    let text = |key: &str| object.get(key).and_then(Value::as_str).unwrap_or("");
    let repo = text("repo");
    if repo.is_empty() || repo.len() > MAX_PATH || repo.chars().any(char::is_control) {
        return Err(Refusal::Unusable("no repository, or not one".to_owned()));
    }
    let agent: String = text("agent")
        .chars()
        .filter(|c| !c.is_control())
        .take(64)
        .collect();
    let op = object
        .get("op")
        .cloned()
        .and_then(|op| serde_json::from_value::<WriteOp>(op).ok())
        .ok_or_else(|| Refusal::Unusable("not an operation this gitcraft knows".to_owned()))?;
    Ok(AgentCall {
        repo: repo.to_owned(),
        agent: if agent.trim().is_empty() {
            "An agent".to_owned()
        } else {
            agent
        },
        op,
    })
}

/// `path` as a folder to open, when it is one: not empty, not too long, no control characters
/// (a line break, a NUL), and absolute or on another machine. A relative one would be read
/// against the running instance's folder, not the start's that sent it.
pub fn valid_path(path: &str) -> Result<PathBuf, String> {
    if path.is_empty() {
        return Err("no path".to_owned());
    }
    if path.len() > MAX_PATH {
        return Err("the path is too long".to_owned());
    }
    if path.chars().any(char::is_control) {
        return Err("the path has control characters".to_owned());
    }
    let parsed = Path::new(path);
    if !parsed.is_absolute() && !crate::remote::is_remote(parsed) {
        return Err(format!("{path} is not an absolute path"));
    }
    Ok(parsed.to_owned())
}

/// The most arguments a command line handed over has (`blame -- <file>` is three).
const MAX_ARGS: usize = 4;

/// `args` as a command line to carry out: few, short, without control characters, and one
/// that `crate::launch` reads as something to do.
pub fn valid_args(args: &[String]) -> Result<(), String> {
    if args.is_empty() || args.len() > MAX_ARGS {
        return Err("not a command line gitcraft takes".to_owned());
    }
    if args
        .iter()
        .any(|a| a.len() > MAX_PATH || a.chars().any(char::is_control))
    {
        return Err("the command line has control characters or is too long".to_owned());
    }
    match crate::launch::parse(args.iter().map(std::ffi::OsString::from)) {
        crate::launch::Parsed::Run(Some(_)) => Ok(()),
        crate::launch::Parsed::Fail(why) => Err(why),
        _ => Err("not a command line gitcraft takes".to_owned()),
    }
}

/// `reply` as the line sent.
pub fn encode_reply(reply: &Reply) -> String {
    let value = match reply {
        Reply::Ok => json!({ "v": VERSION, "status": "ok" }),
        Reply::Refused(why) => json!({ "v": VERSION, "status": "refused", "error": why }),
        Reply::Closing => json!({ "v": VERSION, "status": "closing" }),
        Reply::Agent(reply) => json!({
            "v": VERSION,
            "status": "agent",
            "outcome": reply.outcome,
            "message": reply.message,
        }),
    };
    format!("{value}\n")
}

/// The reply in `line`; `None` for one that isn't a gitcraft's.
pub fn decode_reply(line: &str) -> Option<Reply> {
    let value: Value = serde_json::from_str(line.trim_end()).ok()?;
    match value.get("status")?.as_str()? {
        "ok" => Some(Reply::Ok),
        "closing" => Some(Reply::Closing),
        "agent" => Some(Reply::Agent(wire::Reply::new(
            serde_json::from_value(value.get("outcome")?.clone()).ok()?,
            value
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or_default(),
        ))),
        "refused" => Some(Reply::Refused(
            value
                .get("error")
                .and_then(Value::as_str)
                .unwrap_or("no reason given")
                .to_owned(),
        )),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TOKEN: &str = "0123456789abcdef0123456789abcdef";

    fn absolute() -> PathBuf {
        if cfg!(windows) {
            PathBuf::from(r"C:\code\app")
        } else {
            PathBuf::from("/home/me/app")
        }
    }

    #[test]
    fn requests_go_there_and_back() {
        for request in [
            Request::Focus,
            Request::Open(absolute()),
            Request::Open(PathBuf::from("ssh://build/home/me/app")),
            Request::Launch {
                cwd: absolute(),
                args: vec!["blame".into(), "src/my file ü.rs:40".into()],
            },
        ] {
            let line = encode_request(TOKEN, &request);
            assert!(line.ends_with('\n') && line.matches('\n').count() == 1);
            assert_eq!(decode_request(&line, TOKEN), Ok(request));
        }
    }

    #[test]
    fn a_line_break_in_a_path_stays_inside_the_line() {
        let line = encode_request(TOKEN, &Request::Open(PathBuf::from("/a\nb")));
        assert_eq!(line.matches('\n').count(), 1);
        assert!(matches!(
            decode_request(&line, TOKEN),
            Err(Refusal::Unusable(_))
        ));
    }

    #[test]
    fn an_agents_call_goes_there_and_back() {
        let call = Request::Agent(AgentCall {
            repo: "/home/me/app".into(),
            agent: "Claude Code".into(),
            op: WriteOp::DeleteBranch {
                name: "old".into(),
                force: true,
            },
        });
        let line = encode_request(TOKEN, &call);
        assert_eq!(line.matches('\n').count(), 1);
        assert_eq!(decode_request(&line, TOKEN), Ok(call));
        // Without its token it is a stranger's, like any other.
        assert_eq!(
            decode_request(&line, &"f".repeat(32)),
            Err(Refusal::Stranger)
        );
    }

    #[test]
    fn an_agents_call_is_checked_before_it_is_taken() {
        let refused = |rest: &str| {
            let line = format!(r#"{{"v":1,"token":"{TOKEN}","kind":"agent",{rest}}}"#);
            matches!(decode_request(&line, TOKEN), Err(Refusal::Unusable(_)))
        };
        assert!(refused(r#""repo":"/r","agent":"a","op":{"op":"rm_rf"}"#));
        assert!(refused(
            r#""repo":"","agent":"a","op":{"op":"push","force":false}"#
        ));
        assert!(refused(
            r#""repo":"/r\n","agent":"a","op":{"op":"push","force":false}"#
        ));
        let named = format!(
            r#"{{"v":1,"token":"{TOKEN}","kind":"agent","repo":"/r","agent":"\u0007","op":{{"op":"push","force":false}}}}"#
        );
        assert!(matches!(
            decode_request(&named, TOKEN),
            Ok(Request::Agent(AgentCall { agent, .. })) if agent == "An agent"
        ));
    }

    #[test]
    fn a_command_line_is_taken_only_as_gitcraft_reads_one() {
        let launch = |cwd: &str, args: &[&str]| {
            let line = encode_request(
                TOKEN,
                &Request::Launch {
                    cwd: PathBuf::from(cwd),
                    args: args.iter().map(|a| (*a).to_owned()).collect(),
                },
            );
            decode_request(&line, TOKEN)
        };
        let cwd = absolute();
        let cwd = cwd.to_string_lossy();
        assert!(launch(&cwd, &["log", "a.rs"]).is_ok());
        // Refused: a revision git would take for an option, a relative folder, no command,
        // too many arguments, a line break.
        for (cwd, args) in [
            (&*cwd, &["show", "--output=/tmp/x"][..]),
            ("relative/dir", &["log", "a.rs"]),
            (&*cwd, &[]),
            (&*cwd, &["log", "a", "b", "c", "d"]),
            (&*cwd, &["log", "a\nb"]),
            (&*cwd, &["--frobnicate"]),
        ] {
            assert!(
                matches!(launch(cwd, args), Err(Refusal::Unusable(_))),
                "{cwd} {args:?}"
            );
        }
    }

    #[test]
    fn replies_go_there_and_back() {
        for reply in [
            Reply::Ok,
            Reply::Closing,
            Reply::Refused("why".to_owned()),
            Reply::Agent(wire::Reply::new(wire::Outcome::Denied, "no")),
        ] {
            assert_eq!(decode_reply(&encode_reply(&reply)), Some(reply));
        }
        assert_eq!(decode_reply("HTTP/1.1 200 OK"), None);
        assert_eq!(decode_reply(r#"{"status":"maybe"}"#), None);
    }

    #[test]
    fn a_wrong_or_missing_token_is_a_stranger_or_nothing() {
        let line = encode_request("ffffffffffffffffffffffffffffffff", &Request::Focus);
        assert_eq!(decode_request(&line, TOKEN), Err(Refusal::Stranger));
        assert_eq!(
            decode_request(r#"{"v":1,"kind":"focus"}"#, TOKEN),
            Err(Refusal::Malformed)
        );
        assert_eq!(
            decode_request(r#"{"v":1,"token":7,"kind":"focus"}"#, TOKEN),
            Err(Refusal::Malformed)
        );
    }

    /// What a web page can make a browser send to 127.0.0.1, and other garbage.
    #[test]
    fn what_no_gitcraft_sends_is_malformed() {
        for line in [
            "POST / HTTP/1.1",
            "GET /?token=x HTTP/1.1",
            "",
            "[1,2]",
            "\"focus\"",
            "{",
        ] {
            assert_eq!(
                decode_request(line, TOKEN),
                Err(Refusal::Malformed),
                "{line}"
            );
        }
    }

    #[test]
    fn another_version_or_kind_is_refused_and_says_why() {
        let other = format!(r#"{{"v":2,"token":"{TOKEN}","kind":"focus"}}"#);
        assert!(
            matches!(decode_request(&other, TOKEN), Err(Refusal::Unusable(w)) if w.contains("version"))
        );
        let none = format!(r#"{{"token":"{TOKEN}","kind":"focus"}}"#);
        assert!(matches!(
            decode_request(&none, TOKEN),
            Err(Refusal::Unusable(_))
        ));
        let unknown = format!(r#"{{"v":1,"token":"{TOKEN}","kind":"run","cmd":"rm -rf /"}}"#);
        assert!(
            matches!(decode_request(&unknown, TOKEN), Err(Refusal::Unusable(w)) if w.contains("\"run\""))
        );
    }

    #[test]
    fn only_well_formed_paths_are_opened() {
        assert!(valid_path("").is_err());
        assert!(valid_path("relative/folder").is_err());
        assert!(valid_path(".").is_err());
        assert!(valid_path("--upload-pack=touch x").is_err());
        assert!(valid_path("ssh://-oProxyCommand=x/repo").is_err());
        assert!(valid_path("/a\0b").is_err());
        assert!(valid_path(&format!("/{}", "a".repeat(MAX_PATH))).is_err());
        assert_eq!(
            valid_path(&absolute().to_string_lossy()),
            Ok(absolute()),
            "an absolute folder"
        );
        assert!(valid_path("ssh://me@build:2222/~/repo").is_ok());
        let open = format!(r#"{{"v":1,"token":"{TOKEN}","kind":"open"}}"#);
        assert!(matches!(
            decode_request(&open, TOKEN),
            Err(Refusal::Unusable(_))
        ));
    }
}
