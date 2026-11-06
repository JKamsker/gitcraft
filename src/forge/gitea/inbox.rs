//! Forgejo's and Gitea's inbox: the open pull requests of a repository whose review is asked
//! of the person logged in, and theirs with their checks and what others said on them.

use serde_json::Value;

use super::{Api, Hosting};
use crate::forge::inbox::{Activity, Checks, InboxPr, status_checks};
use crate::pull_request::{at, parse_time};

/// How many of your pull requests get their checks and comments read: two requests each.
const MAX_MINE: usize = 20;

pub(super) fn inbox(api: &Api, hosting: &Hosting) -> Result<Vec<InboxPr>, String> {
    if !api.logged_in() {
        return Err(format!(
            "no {} login for {}: log in with `tea login add` (or let git's credential helper keep \
             one) so that gitcraft knows which pull requests are yours",
            api.name(),
            hosting.web
        ));
    }
    let me = api.get("user")?;
    let login = at(&me, "/login").to_owned();
    if login.is_empty() {
        return Err(format!("{} didn't say who is logged in", api.name()));
    }
    let endpoint = format!("repos/{}/pulls?state=open&sort=recentupdate", hosting.path);
    let items = api.get_all(&endpoint, 100)?;
    let mut list: Vec<InboxPr> = items.iter().filter_map(|i| read(i, &login)).collect();
    for pr in list.iter_mut().filter(|p| p.mine).take(MAX_MINE) {
        let item = items
            .iter()
            .find(|i| i.get("number").and_then(Value::as_u64) == Some(pr.number));
        let sha = item.map(|i| at(i, "/head/sha")).unwrap_or_default();
        pr.checks = checks(api, hosting, sha);
        pr.activity = Activity::At(said_by_others(api, hosting, pr.number, &login));
    }
    Ok(list)
}

/// One pull request of the list, if it is yours or asks your review.
fn read(item: &Value, login: &str) -> Option<InboxPr> {
    let author = at(item, "/user/login").to_owned();
    let requested = item
        .get("requested_reviewers")
        .and_then(Value::as_array)
        .is_some_and(|r| r.iter().any(|u| at(u, "/login") == login));
    let mine = author == login;
    (mine || requested).then(|| InboxPr {
        number: item.get("number").and_then(Value::as_u64).unwrap_or(0),
        title: at(item, "/title").to_owned(),
        url: at(item, "/html_url").to_owned(),
        draft: item.get("draft").and_then(Value::as_bool).unwrap_or(false)
            || super::is_draft_title(at(item, "/title")),
        updated: parse_time(at(item, "/updated_at")).unwrap_or(0),
        review_requested: requested,
        mine,
        checks: Checks::Unknown,
        activity: Activity::default(),
        author,
    })
}

/// The combined status of `sha`; unknown when it can't be read or has none.
fn checks(api: &Api, hosting: &Hosting, sha: &str) -> Checks {
    if sha.is_empty() {
        return Checks::Unknown;
    }
    let Ok(status) = api.get(&format!("repos/{}/commits/{sha}/status", hosting.path)) else {
        return Checks::Unknown;
    };
    let count = status
        .get("total_count")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let has_any = count > 0
        || status
            .get("statuses")
            .and_then(Value::as_array)
            .is_some_and(|s| !s.is_empty());
    if has_any {
        status_checks(at(&status, "/state"))
    } else {
        Checks::Unknown
    }
}

/// When others commented on the pull request or reviewed it; what can't be read counts as
/// nothing said.
fn said_by_others(api: &Api, hosting: &Hosting, number: u64, login: &str) -> Vec<i64> {
    let path = &hosting.path;
    let mut times = Vec::new();
    let sources = [
        (
            format!("repos/{path}/issues/{number}/comments"),
            "/created_at",
        ),
        (
            format!("repos/{path}/pulls/{number}/reviews"),
            "/submitted_at",
        ),
    ];
    for (endpoint, time) in sources {
        let Ok(items) = api.get_all(&endpoint, 200) else {
            continue;
        };
        times.extend(
            items
                .iter()
                .filter(|c| at(c, "/user/login") != login)
                // A review only asked for, or not sent yet, says nothing.
                .filter(|c| !matches!(at(c, "/state"), "REQUEST_REVIEW" | "PENDING"))
                .filter_map(|c| parse_time(at(c, time))),
        );
    }
    times.sort_unstable();
    times
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_yours_and_those_asking_your_review() {
        let item = |author: &str, reviewers: &[&str]| {
            serde_json::json!({
                "number": 9, "title": "WIP: ünïcode", "html_url": "http://h/o/r/pulls/9",
                "user": {"login": author},
                "requested_reviewers": reviewers.iter().map(|r| serde_json::json!({"login": r})).collect::<Vec<_>>(),
                "updated_at": "2026-11-02T20:00:00+01:00"
            })
        };
        let mine = read(&item("me", &[]), "me");
        assert!(
            mine.as_ref()
                .is_some_and(|p| p.mine && !p.review_requested && p.draft)
        );
        let asked = read(&item("ana", &["bo", "me"]), "me");
        assert!(asked.is_some_and(|p| p.review_requested && !p.mine && p.number == 9));
        assert!(read(&item("ana", &["bo"]), "me").is_none());
        let no_reviewers =
            serde_json::json!({"number": 1, "user": {"login": "x"}, "requested_reviewers": null});
        assert!(read(&no_reviewers, "me").is_none());
    }
}
