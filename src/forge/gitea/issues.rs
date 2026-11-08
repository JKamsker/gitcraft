//! The issues assigned to the person whose login gitcraft found for the server, to start work
//! from, and one of them read again.

use serde_json::Value;

use super::super::http::Api;
use crate::forge::{Hosting, ISSUES_MAX, Issue};
use crate::pull_request::{at, items};

/// The open issues (not pull requests) of the repository assigned to the login's account. The
/// API only knows whose they are from the login, so without one it isn't asked.
pub(super) fn assigned(api: &Api, hosting: &Hosting) -> Result<Vec<Issue>, String> {
    if !api.logged_in() {
        return Err(format!(
            "{} needs a login to tell which issues are yours: add one with `tea login add`, or \
             keep a token for {} in git's credential helper",
            api.name(),
            hosting.web
        ));
    }
    let me = api.get("user")?;
    let login = at(&me, "/login");
    if login.is_empty() {
        return Err(format!("{} didn't say who the login is", api.name()));
    }
    let endpoint = format!(
        "repos/{}/issues?state=open&type=issues&assigned_by={}",
        hosting.path,
        query_value(login)
    );
    // Pull requests left out page by page: they don't take the place of issues in the cap.
    Ok(api
        .get_all_kept(&endpoint, ISSUES_MAX, |item| !is_pull_request(item))?
        .iter()
        .filter_map(parse_issue)
        .collect())
}

/// Issue `number` as it is now.
pub(super) fn one(api: &Api, hosting: &Hosting, number: u64) -> Result<Issue, String> {
    let item = api.get(&format!("repos/{}/issues/{number}", hosting.path))?;
    parse_issue(&item).ok_or_else(|| format!("#{number} is a pull request, not an issue"))
}

/// Older servers ignore `type=issues`: a pull request still says it is one.
fn is_pull_request(item: &Value) -> bool {
    item.get("pull_request").is_some_and(|pr| !pr.is_null())
}

/// `text` as it can go in a URL's query: anything but letters, digits and `-_.~` escaped.
fn query_value(text: &str) -> String {
    text.bytes()
        .map(|b| match b {
            b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                char::from(b).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect()
}

/// One issue of the list; `None` for a pull request.
fn parse_issue(item: &Value) -> Option<Issue> {
    if is_pull_request(item) {
        return None;
    }
    Some(Issue {
        tracker: None,
        acceptance: None,
        number: item.get("number").and_then(Value::as_u64)?,
        title: at(item, "/title").to_owned(),
        body: at(item, "/body").to_owned(),
        url: at(item, "/html_url").to_owned(),
        labels: items(item, "/labels")
            .iter()
            .map(|label| at(label, "/name").to_owned())
            .filter(|name| !name.is_empty())
            .collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::forge::Forge;

    #[test]
    fn reads_an_issue_of_the_list() {
        let item = serde_json::json!({
            "id": 555, "number": 9, "title": "Füße", "body": "* [x] done",
            "html_url": "https://codeberg.org/o/r/issues/9",
            "labels": [{"id": 1, "name": "kind/bug", "color": "ee0701"}],
            "pull_request": null, "state": "open"
        });
        assert_eq!(
            parse_issue(&item),
            Some(Issue {
                tracker: None,
                acceptance: None,
                number: 9,
                title: "Füße".into(),
                body: "* [x] done".into(),
                url: "https://codeberg.org/o/r/issues/9".into(),
                labels: vec!["kind/bug".into()],
            })
        );
    }

    #[test]
    fn a_pull_request_or_an_issue_without_a_number_is_none() {
        let pr = serde_json::json!({"number": 3, "title": "t", "pull_request": {"merged": false}});
        assert_eq!(parse_issue(&pr), None);
        assert_eq!(parse_issue(&serde_json::json!({"title": "t"})), None);
        let bare = parse_issue(&serde_json::json!({"number": 4, "body": null, "labels": null}));
        assert_eq!(
            bare.map(|i| (i.number, i.body, i.labels.len())),
            Some((4, "".into(), 0))
        );
    }

    #[test]
    fn without_a_login_it_says_how_to_add_one() {
        let hosting = Hosting {
            forge: Forge::Forgejo,
            host: "127.0.0.1".into(),
            path: "o/r".into(),
            // Nothing listens there: were it asked, the error would be another.
            web: "http://127.0.0.1:9".into(),
        };
        let api = Api::new("Forgejo", &hosting.web, &hosting.host, None);
        let err = assigned(&api, &hosting).err().unwrap_or_default();
        assert!(
            err.contains("needs a login") && err.contains("tea login add"),
            "{err}"
        );
    }

    #[test]
    fn a_login_goes_in_the_query_escaped() {
        assert_eq!(query_value("ana.b_c-d"), "ana.b_c-d");
        assert_eq!(query_value("a b&c"), "a%20b%26c");
        assert_eq!(query_value("ü"), "%C3%BC");
    }
}
