//! Bounded, explicit commit→PR→line-comment evidence, shared by history cards.
//! No full PR/CI page is polled. Unmapped comments are counted, never relabelled as evidence.
use super::{Forge, Hosting};
use crate::git::investigations::{Reader, dossier::Seed, oid};
use serde_json::Value;
use std::{
    collections::BTreeMap,
    path::Path,
    time::{Duration, Instant},
};
#[derive(Clone, Debug)]
pub(crate) struct Query {
    pub at: Seed,
    pub file_oid: String,
    pub comments: bool,
}
#[derive(Clone, Debug)]
pub(crate) struct Comment {
    pub author: String,
    pub body: String,
    pub url: String,
    pub proof: String,
}
#[derive(Clone, Debug)]
pub(crate) struct Pull {
    pub number: u64,
    pub title: String,
    pub url: String,
    pub state: String,
    pub association: String,
    pub issues: Vec<(String, String)>,
    pub comments: Vec<Comment>,
    pub checked: usize,
    pub unmapped: usize,
    pub limited: bool,
    pub notes: Vec<String>,
}
#[derive(Clone, Debug, Default)]
pub(crate) struct Evidence {
    pub pulls: Vec<Pull>,
    pub notes: Vec<String>,
    pub limited: bool,
}
fn text(v: &Value, key: &str) -> String {
    v.pointer(key)
        .and_then(Value::as_str)
        .unwrap_or("")
        .chars()
        .take(4096)
        .collect()
}
fn number(v: &Value, key: &str) -> u64 {
    v.pointer(key).and_then(Value::as_u64).unwrap_or(0)
}
fn component(value: &str) -> String {
    let mut s = String::new();
    for b in value.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            s.push(char::from(b));
        } else {
            s.push_str(&format!("%{b:02X}"));
        }
    }
    s
}
fn project(h: &Hosting) -> String {
    h.path
        .split('/')
        .map(component)
        .collect::<Vec<_>>()
        .join("/")
}
fn safe_url(url: &str) -> bool {
    let Some((scheme, rest)) = url.split_once("://") else {
        return false;
    };
    matches!(scheme, "https" | "http")
        && !rest.split('/').next().unwrap_or("").is_empty()
        && !rest.split('/').next().unwrap_or("").contains('@')
        && !url.chars().any(char::is_control)
}
fn url(v: &Value, key: &str) -> String {
    let value = text(v, key);
    if safe_url(&value) {
        value
    } else {
        String::new()
    }
}
fn issue_links(h: &Hosting, body: &str) -> Vec<(String, String)> {
    let words: Vec<_> = body.split_whitespace().take(2048).collect();
    let mut out = Vec::new();
    for pair in words.windows(2) {
        let verb = pair[0]
            .trim_matches(|c: char| !c.is_ascii_alphabetic())
            .to_ascii_lowercase();
        if !matches!(
            verb.as_str(),
            "fix"
                | "fixes"
                | "fixed"
                | "close"
                | "closes"
                | "closed"
                | "resolve"
                | "resolves"
                | "resolved"
        ) {
            continue;
        }
        let issue = pair[1].trim_matches(|c: char| matches!(c, ',' | '.' | ';' | ')' | ']'));
        let Some(n) = issue
            .strip_prefix('#')
            .and_then(|s| s.parse::<u64>().ok())
            .filter(|n| *n > 0)
        else {
            continue;
        };
        let segment = if h.forge == Forge::GitLab {
            "-/issues"
        } else {
            "issues"
        };
        let link = format!(
            "{}/{}/{segment}/{n}",
            h.web.trim_end_matches('/'),
            project(h)
        );
        if safe_url(&link) && !out.iter().any(|(id, _)| id == &format!("#{n}")) {
            out.push((format!("#{n}"), link));
        }
        if out.len() == 16 {
            break;
        }
    }
    out
}
struct Client<'a> {
    hosting: &'a Hosting,
    api: Option<super::http::Api>,
    began: Instant,
    requests: usize,
    active: &'a dyn Fn() -> bool,
}
impl Client<'_> {
    fn get(&mut self, path: &str) -> Result<Value, String> {
        if self.requests >= 48 || self.began.elapsed() > Duration::from_secs(90) || !(self.active)()
        {
            return Err("History evidence request budget reached or cancelled".into());
        }
        self.requests += 1;
        match &self.api {
            Some(api) => api.history_get(path, self.active),
            None => super::cli::history_api(self.hosting, path, self.active),
        }
    }
}
pub(crate) fn lookup(
    repo: &Path,
    hosting: &Hosting,
    query: &Query,
    active: &dyn Fn() -> bool,
) -> Result<Evidence, String> {
    check_provider(hosting.forge)?;
    if !oid(&query.at.commit)
        || !oid(&query.file_oid)
        || query.at.line == 0
        || query.at.line > 20000
        || query.at.path.len() > 4096
    {
        return Err(crate::i18n::text("backend-invalid-pinned-history-card-1b9013").into());
    }
    if !safe_url(&hosting.web)
        || hosting
            .path
            .split('/')
            .any(|s| s.is_empty() || matches!(s, "." | ".."))
    {
        return Err(crate::i18n::text("backend-invalid-captured-forge-repository-098e0a").into());
    }
    let authority = super::location(&format!("{}/_history_guard_", hosting.web))
        .map(|(host, _)| host)
        .ok_or(crate::i18n::text("backend-invalid-forge-authority-9d9b21"))?;
    if !authority.eq_ignore_ascii_case(&hosting.host) {
        return Err(
            crate::i18n::text("backend-captured-forge-account-and-api-host-differ-39ae5b").into(),
        );
    }
    let reader = Reader::new(repo, active);
    let config = reader.run(
        &["config", "--get-regexp", r"^remote\..*\.url$"],
        vec![],
        &[0, 1],
    )?;
    let config = std::str::from_utf8(&config)
        .map_err(|_| crate::i18n::text("backend-repository-remote-is-not-utf-8-db2d43"))?;
    let same = config
        .lines()
        .filter_map(|line| line.split_once(' '))
        .any(|(_, url)| {
            super::location(url).is_some_and(|(host, path)| {
                host.eq_ignore_ascii_case(&hosting.host) && path == hosting.path
            })
        });
    if !same {
        return Err(crate::i18n::text(
            "backend-repository-forge-remote-changed-refresh-before-requesting-evidenc-cd608a",
        )
        .into());
    }
    let api = matches!(hosting.forge, Forge::Gitea | Forge::Forgejo)
        .then(|| super::gitea::history_api(hosting));
    let mut client = Client {
        hosting,
        api,
        began: Instant::now(),
        requests: 0,
        active,
    };
    let mut proved = BTreeMap::<(String, String), bool>::new();
    let mut proof_limited = false;
    let mut prove = |revision: &str, path: &str| {
        if path != query.at.path || !oid(revision) {
            return false;
        }
        if revision == query.at.commit {
            return true;
        }
        let key = (revision.to_owned(), path.to_owned());
        if let Some(result) = proved.get(&key) {
            return *result;
        }
        if proved.len() >= 64 {
            proof_limited = true;
            return false;
        }
        let proof = (|| {
            let out = reader
                .run(
                    &[
                        "--literal-pathspecs",
                        "ls-tree",
                        "-z",
                        "--full-tree",
                        revision,
                        "--",
                        path,
                    ],
                    vec![],
                    &[0],
                )
                .ok()?;
            let records: Vec<_> = out.split(|b| *b == 0).filter(|s| !s.is_empty()).collect();
            let [record] = records.as_slice() else {
                return None;
            };
            let split = record.iter().position(|b| *b == b'\t')?;
            if &record[split + 1..] != path.as_bytes() {
                return None;
            }
            let header = std::str::from_utf8(&record[..split]).ok()?;
            let fields: Vec<_> = header.split_whitespace().collect();
            let [mode, kind, id] = fields.as_slice() else {
                return None;
            };
            Some(matches!(*mode, "100644" | "100755") && *kind == "blob" && *id == query.file_oid)
        })()
        .unwrap_or(false);
        proved.insert(key, proof);
        proof
    };
    let mut result = lookup_with(hosting, query, &mut |path| client.get(path), &mut prove)?;
    if proof_limited {
        result.limited = true;
        result.notes.push(
            crate::i18n::text(
                "backend-64-reviewed-revision-proofs-reached-remaining-comments-were-not-l-e73501",
            )
            .into(),
        );
    }
    Ok(result)
}
fn pages(
    endpoint: &str,
    get: &mut dyn FnMut(&str) -> Result<Value, String>,
    visit: &mut dyn FnMut(&Value),
) -> Result<bool, String> {
    let mut previous = String::new();
    for page in 1..=5 {
        let rows = list(get(&format!("{endpoint}&page={page}"))?)?;
        if rows.is_empty() {
            return Ok(false);
        }
        let marker = rows.first().map(Value::to_string).unwrap_or_default();
        if page > 1 && marker == previous {
            return Ok(true);
        }
        previous = marker;
        for row in rows.iter().take(100) {
            visit(row);
        }
        if rows.len() > 100 {
            return Ok(true);
        }
    }
    Ok(true)
}
#[cfg(test)]
mod tests;

mod comments;
use comments::*;

mod association;
use association::*;
