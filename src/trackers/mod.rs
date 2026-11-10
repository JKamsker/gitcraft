//! Assigned Jira Cloud / Linear issues. All calls run in the initiating profile's worker.
#[cfg(test)]
mod tests;
pub(crate) mod ui;
use crate::{connections, forge::Issue};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::io::Read;
use std::time::Duration;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Kind {
    Jira,
    Linear,
}
impl Kind {
    pub(crate) const ALL: [Self; 2] = [Self::Jira, Self::Linear];
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Jira => "Jira",
            Self::Linear => "Linear",
        }
    }
    pub(crate) fn account(self, c: &connections::Context) -> &Account {
        match self {
            Self::Jira => &c.choices.jira,
            Self::Linear => &c.choices.linear,
        }
    }
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Account {
    pub origin: String,
    /// Email of the selected account; verified against the server's viewer.
    pub user: String,
    pub secret_ref: String,
    /// Jira customfield_N containing acceptance criteria (optional).
    pub acceptance_field: String,
}
impl Account {
    pub(crate) fn validate(&self, kind: Kind) -> Result<(), String> {
        if self == &Self::default() {
            return Ok(());
        }
        let u = url::Url::parse(&self.origin).map_err(|_| "Choose an HTTPS tracker origin")?;
        if u.scheme() != "https"
            || u.host_str().is_none()
            || !u.username().is_empty()
            || u.password().is_some()
            || u.path() != "/"
            || u.query().is_some()
            || u.fragment().is_some()
            || (kind == Kind::Linear
                && self.origin.trim_end_matches('/') != "https://api.linear.app")
        {
            return Err("Use an HTTPS origin only; Linear uses https://api.linear.app".into());
        }
        if self.user.is_empty() || self.user.chars().any(char::is_control) {
            return Err("Choose the tracker's account email".into());
        }
        if !self.acceptance_field.is_empty()
            && (kind != Kind::Jira
                || !self
                    .acceptance_field
                    .strip_prefix("customfield_")
                    .is_some_and(|v| !v.is_empty() && v.bytes().all(|b| b.is_ascii_digit())))
        {
            return Err("The Jira criteria field must be customfield_N".into());
        }
        Ok(())
    }
    fn store_key(&self) -> String {
        json!([
            self.origin.trim_end_matches('/'),
            self.user,
            self.secret_ref
        ])
        .to_string()
    }
    pub(crate) fn store(&self, kind: Kind, token: &str) -> Result<(), String> {
        self.validate(kind)?;
        if self.secret_ref.is_empty()
            || token.trim().is_empty()
            || token.chars().any(char::is_control)
        {
            return Err("A secure reference and token are required".into());
        }
        if !crate::portable::credential_store() {
            return Err("Credential storage is disabled in portable mode".into());
        }
        keyring::Entry::new("gitcraft-tracker", &self.store_key())
            .map_err(|_| "Cannot open tracker credential store")?
            .set_password(token)
            .map_err(|_| "Cannot save tracker credential".to_owned())
            .map(|_| ())
    }
    fn secret(&self) -> Result<String, String> {
        if self.secret_ref.is_empty() || !crate::portable::credential_store() {
            return Err("Save this tracker's token in Profile connections first".into());
        }
        keyring::Entry::new("gitcraft-tracker", &self.store_key())
            .map_err(|_| "Cannot open tracker credential store")?
            .get_password()
            .map_err(|_| "Selected tracker credential is unavailable on this computer".into())
    }
}
/// Stable source identity; safe metadata persisted beside a branch. Never a token.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Identity {
    pub kind: Kind,
    pub origin: String,
    pub account: String,
    pub viewer: String,
    pub id: String,
    pub key: String,
}
impl Identity {
    pub(crate) fn selected(&self, c: &connections::Context) -> bool {
        let a = self.kind.account(c);
        a.origin.trim_end_matches('/') == self.origin && a.user == self.account
    }
}
struct Client<F> {
    kind: Kind,
    account: Account,
    send: F,
}
fn required<'a>(v: &'a Value, path: &str) -> Result<&'a str, String> {
    v.pointer(path)
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| format!("Tracker response is missing {path}"))
}
fn graph(v: Value) -> Result<Value, String> {
    if v.get("errors")
        .is_some_and(|e| e.as_array().is_none_or(|a| !a.is_empty()))
    {
        return Err("Linear rejected the query; no partial issues were accepted".into());
    }
    v.get("data")
        .cloned()
        .filter(Value::is_object)
        .ok_or_else(|| "Linear response has no data".into())
}
const LINEAR_FIELDS: &str =
    "id identifier title description url labels { nodes { name } } assignee { id }";
impl<F: FnMut(&str, &str, Option<&Value>) -> Result<Value, String>> Client<F> {
    fn request(&mut self, method: &str, path: &str, body: Option<&Value>) -> Result<Value, String> {
        (self.send)(method, path, body)
    }
    fn gql(&mut self, query: String, variables: Value) -> Result<Value, String> {
        graph(self.request(
            "POST",
            "/graphql",
            Some(&json!({"query":query,"variables":variables})),
        )?)
    }
    fn viewer(&mut self) -> Result<String, String> {
        let (id, email) = match self.kind {
            Kind::Jira => {
                let v = self.request("GET", "/rest/api/3/myself", None)?;
                (
                    required(&v, "/accountId")?.to_owned(),
                    required(&v, "/emailAddress")?.to_owned(),
                )
            }
            Kind::Linear => {
                let v = self.gql("query { viewer { id email } }".into(), json!({}))?;
                (
                    required(&v, "/viewer/id")?.to_owned(),
                    required(&v, "/viewer/email")?.to_owned(),
                )
            }
        };
        if !email.eq_ignore_ascii_case(&self.account.user) {
            return Err("Tracker authenticated a different account; select its exact email before loading issues".into());
        }
        Ok(id)
    }
    fn fields(&self) -> Vec<String> {
        let mut f = vec![
            "summary".into(),
            "description".into(),
            "labels".into(),
            "assignee".into(),
        ];
        if !self.account.acceptance_field.is_empty() {
            f.push(self.account.acceptance_field.clone());
        }
        f
    }
    fn listed(&mut self) -> Result<Vec<Issue>, String> {
        let viewer = self.viewer()?;
        let mut result = vec![];
        let mut cursor = None::<String>;
        let mut seen = std::collections::HashSet::new();
        loop {
            let (rows, next) = match self.kind {
                Kind::Jira => {
                    let mut q = json!({"jql":"assignee = currentUser() AND resolution = Unresolved ORDER BY updated DESC", "fields":self.fields(),"maxResults":50});
                    if let Some(c) = &cursor {
                        q["nextPageToken"] = json!(c);
                    }
                    let v = self.request("POST", "/rest/api/3/search/jql", Some(&q))?;
                    let rows = v
                        .get("issues")
                        .and_then(Value::as_array)
                        .ok_or("Jira response has no issue list")?
                        .clone();
                    let next = if v.get("isLast").and_then(Value::as_bool) == Some(true) {
                        None
                    } else {
                        Some(required(&v, "/nextPageToken")?.to_owned())
                    };
                    (rows, next)
                }
                Kind::Linear => {
                    let q = format!(
                        "query Assigned($cursor: String) {{ viewer {{ assignedIssues(first: 50, after: $cursor, filter: {{ state: {{ type: {{ nin: [\"completed\",\"canceled\"] }} }} }}) {{ nodes {{ {LINEAR_FIELDS} }} pageInfo {{ hasNextPage endCursor }} }} }} }}"
                    );
                    let v = self.gql(q, json!({"cursor":cursor}))?;
                    let rows = v
                        .pointer("/viewer/assignedIssues/nodes")
                        .and_then(Value::as_array)
                        .ok_or("Linear response has no issue list")?
                        .clone();
                    let next = if v
                        .pointer("/viewer/assignedIssues/pageInfo/hasNextPage")
                        .and_then(Value::as_bool)
                        .ok_or("Linear response has no pagination state")?
                    {
                        Some(required(&v, "/viewer/assignedIssues/pageInfo/endCursor")?.to_owned())
                    } else {
                        None
                    };
                    (rows, next)
                }
            };
            for row in rows {
                let assigned = match self.kind {
                    Kind::Jira => row.pointer("/fields/assignee/accountId"),
                    Kind::Linear => row.pointer("/assignee/id"),
                }
                .and_then(Value::as_str);
                if assigned != Some(&viewer) {
                    return Err(
                        "Tracker returned an issue assigned to another account; refresh the source"
                            .into(),
                    );
                }
                let issue = self.parse(&row, &viewer)?;
                if result.iter().any(|i: &Issue| {
                    i.tracker.as_ref().map(|t| &t.id) == issue.tracker.as_ref().map(|t| &t.id)
                }) {
                    continue;
                }
                result.push(issue);
                if result.len() == crate::forge::ISSUES_MAX {
                    return Ok(result);
                }
            }
            match next {
                None => return Ok(result),
                Some(c) => {
                    if !seen.insert(c.clone()) || seen.len() > 100 {
                        return Err("Tracker repeated a page cursor".into());
                    }
                    cursor = Some(c);
                }
            }
        }
    }
    fn one(&mut self, identity: &Identity) -> Result<Issue, String> {
        if self.kind != identity.kind
            || self.account.origin.trim_end_matches('/') != identity.origin
            || self.account.user != identity.account
        {
            return Err(
                "Select the linked issue's tracker origin and account before refreshing it".into(),
            );
        }
        let viewer = self.viewer()?;
        if viewer != identity.viewer {
            return Err("The selected tracker account changed identity".into());
        }
        let value = match self.kind {
            Kind::Jira => self.request(
                "GET",
                &format!(
                    "/rest/api/3/issue/{}?fields={}",
                    encoded(&identity.id),
                    self.fields().join(",")
                ),
                None,
            )?,
            Kind::Linear => self
                .gql(
                    format!("query Issue($id: String!) {{ issue(id: $id) {{ {LINEAR_FIELDS} }} }}"),
                    json!({"id":identity.id}),
                )?
                .get("issue")
                .cloned()
                .ok_or("Linear issue is unavailable")?,
        };
        let issue = self.parse(&value, &viewer)?;
        if issue.tracker.as_ref().is_none_or(|i| i.id != identity.id) {
            return Err("Tracker returned a different issue".into());
        }
        Ok(issue)
    }
    fn parse(&self, v: &Value, viewer: &str) -> Result<Issue, String> {
        let id = required(v, "/id")?.to_owned();
        let mut acceptance = None;
        let (key, title, body, url, labels) = match self.kind {
            Kind::Jira => {
                let key = required(v, "/key")?.to_owned();
                if !key.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-') {
                    return Err("Jira returned an invalid issue key".into());
                }
                let body = adf(v.pointer("/fields/description").unwrap_or(&Value::Null))?;
                if !self.account.acceptance_field.is_empty() {
                    let value=v.pointer(&format!("/fields/{}",self.account.acceptance_field)).ok_or("Jira did not return the configured criteria field; verify the field and account permissions")?;
                    acceptance = Some(adf(value)?);
                }
                (
                    key.clone(),
                    required(v, "/fields/summary")?.to_owned(),
                    body,
                    format!("{}/browse/{key}", self.account.origin.trim_end_matches('/')),
                    v.pointer("/fields/labels")
                        .and_then(Value::as_array)
                        .map(|a| {
                            a.iter()
                                .filter_map(Value::as_str)
                                .map(str::to_owned)
                                .collect()
                        })
                        .unwrap_or_default(),
                )
            }
            Kind::Linear => {
                let url = required(v, "/url")?.to_owned();
                let u = url::Url::parse(&url).map_err(|_| "Invalid Linear issue URL")?;
                if u.scheme() != "https"
                    || u.host_str() != Some("linear.app")
                    || !u.username().is_empty()
                    || u.password().is_some()
                {
                    return Err("Linear returned an unexpected issue URL".into());
                }
                (
                    required(v, "/identifier")?.to_owned(),
                    required(v, "/title")?.to_owned(),
                    v.get("description")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_owned(),
                    url,
                    v.pointer("/labels/nodes")
                        .and_then(Value::as_array)
                        .map(|a| {
                            a.iter()
                                .filter_map(|l| l.get("name").and_then(Value::as_str))
                                .map(str::to_owned)
                                .collect()
                        })
                        .unwrap_or_default(),
                )
            }
        };
        Ok(Issue {
            number: 0,
            acceptance,
            title,
            body,
            url,
            labels,
            tracker: Some(Identity {
                kind: self.kind,
                origin: self.account.origin.trim_end_matches('/').into(),
                account: self.account.user.clone(),
                viewer: viewer.into(),
                id,
                key,
            }),
        })
    }
}
fn encoded(s: &str) -> String {
    url::form_urlencoded::byte_serialize(s.as_bytes()).collect()
}
/// Jira's document tree, preserving readable text and task/table content. No silent truncation.
fn adf(v: &Value) -> Result<String, String> {
    fn walk(v: &Value, depth: usize, out: &mut String) -> Result<(), String> {
        if depth > 64 || out.len() > 4 * 1024 * 1024 {
            return Err("Jira document exceeds the supported reading bound".into());
        }
        if v.is_null() {
            return Ok(());
        }
        if let Some(s) = v.as_str() {
            out.push_str(s);
            return Ok(());
        }
        let kind = v
            .get("type")
            .and_then(Value::as_str)
            .ok_or("Unsupported Jira document value")?;
        match kind {
            "text" => out.push_str(v.get("text").and_then(Value::as_str).unwrap_or_default()),
            "hardBreak" => out.push('\n'),
            "rule" => out.push_str("\n---\n"),
            "mention" | "emoji" => out.push_str(
                v.pointer("/attrs/text")
                    .or_else(|| v.pointer("/attrs/shortName"))
                    .and_then(Value::as_str)
                    .unwrap_or("[mention]"),
            ),
            "heading" => out.push_str(&format!(
                "\n{} ",
                "#".repeat(
                    v.pointer("/attrs/level")
                        .and_then(Value::as_u64)
                        .unwrap_or(2)
                        .min(6) as usize
                )
            )),
            "listItem" => out.push_str("\n- "),
            "taskItem" => out.push_str(
                if v.pointer("/attrs/state").and_then(Value::as_str) == Some("DONE") {
                    "\n- [x] "
                } else {
                    "\n- [ ] "
                },
            ),
            "codeBlock" => out.push_str("\n```\n"),
            "tableCell" | "tableHeader" => out.push_str(" | "),
            "inlineCard" | "blockCard" => out.push_str(
                v.pointer("/attrs/url")
                    .and_then(Value::as_str)
                    .unwrap_or("[card]"),
            ),
            "doc" | "paragraph" | "bulletList" | "orderedList" | "taskList" | "table"
            | "tableRow" | "blockquote" | "panel" => {}
            other => out.push_str(&format!("[Unsupported Jira node: {other}] ")),
        }
        if let Some(children) = v.get("content").and_then(Value::as_array) {
            for c in children {
                walk(c, depth + 1, out)?;
            }
        }
        match kind {
            "paragraph" | "heading" | "tableRow" => out.push('\n'),
            "codeBlock" => out.push_str("\n```\n"),
            _ => {}
        }
        if kind == "text"
            && let Some(marks) = v.get("marks").and_then(Value::as_array)
        {
            for m in marks {
                if m.get("type").and_then(Value::as_str) == Some("link")
                    && let Some(href) = m.pointer("/attrs/href").and_then(Value::as_str)
                {
                    out.push_str(&format!(" ({href})"));
                }
            }
        }
        Ok(())
    }
    let mut out = String::new();
    walk(v, 0, &mut out)?;
    Ok(out.trim().to_owned())
}
type SendRequest = Box<dyn FnMut(&str, &str, Option<&Value>) -> Result<Value, String>>;
mod transport;
pub(crate) use transport::*;
