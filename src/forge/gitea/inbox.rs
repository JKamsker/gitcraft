//! Forgejo's and Gitea's inbox, for a whole host at once: its search finds the open pull
//! requests whose review is asked of the person logged in, and theirs, in every repository
//! there (a fork's upstream too, where a pull request from a fork is), with one request each
//! rather than one per repository. Theirs get their checks and what others said on them read.

use serde_json::Value;

use super::{Api, Hosting};
use crate::forge::inbox::{self, Answer, Checks, InboxPr, MAX_MINE, status_checks};
use crate::pull_request::{at, parse_time};

/// How many of each list are read.
const MAX_LISTED: usize = 100;

pub(super) fn inbox(api: &Api, hosting: &Hosting) -> Result<Answer, String> {
    if !api.logged_in() {
        return Err(format!(
            "no {} login known for {}: add one with `tea login add`, or open one of its \
             repositories with a login, for gitcraft to know which pull requests are yours",
            api.name(),
            hosting.web
        ));
    }
    let me = api.get("user")?;
    let login = at(&me, "/login").to_owned();
    if login.is_empty() {
        return Err(crate::i18n::message(
            "backend-didn-t-say-who-is-logged-in-d78e69",
            &[("name", (api.name()).to_string().into())],
        ));
    }
    let search = |filter: &str| {
        let endpoint = format!("repos/issues/search?type=pulls&state=open&{filter}=true");
        api.get_all(&endpoint, MAX_LISTED)
    };
    let asked = search("review_requested")?;
    let created = search("created")?;
    let mut cut: Vec<String> = [
        inbox::cut_note(
            asked.len(),
            MAX_LISTED,
            crate::i18n::text("backend-asking-your-review-3c3907"),
        ),
        inbox::cut_note(
            created.len(),
            MAX_LISTED,
            crate::i18n::text("backend-of-yours-2a649f"),
        ),
    ]
    .into_iter()
    .flatten()
    .collect();
    let mut prs = inbox::merged(vec![
        asked.iter().map(|i| read(i, true)).collect(),
        created.iter().map(|i| read(i, false)).collect(),
    ]);
    prs.sort_by_key(|p| std::cmp::Reverse(p.updated));
    let mine = prs.iter().filter(|p| p.mine).count();
    if mine > MAX_MINE {
        cut.push(crate::i18n::message(
            "backend-the-checks-and-comments-of-only-the-max-mine-latest-of-your-mine-cbdf30",
            &[("max_mine", MAX_MINE.into()), ("mine", mine.into())],
        ));
    }
    for pr in prs.iter_mut().filter(|p| p.mine).take(MAX_MINE) {
        // The search says nothing of the head commit: the pull request itself does.
        let Ok(full) = api.get(&format!("repos/{}/pulls/{}", pr.project, pr.number)) else {
            continue;
        };
        pr.checks = checks(api, &pr.project, at(&full, "/head/sha"));
        pr.said = said_by_others(api, &pr.project, pr.number, &login);
    }
    Ok(Answer { prs, cut })
}

/// One pull request of a search (an issue that is a pull request): asking your review
/// (`requested`), or yours.
fn read(item: &Value, requested: bool) -> InboxPr {
    let title = at(item, "/title");
    InboxPr {
        number: item.get("number").and_then(Value::as_u64).unwrap_or(0),
        title: title.to_owned(),
        url: at(item, "/html_url").to_owned(),
        author: at(item, "/user/login").to_owned(),
        project: at(item, "/repository/full_name").to_owned(),
        draft: item
            .pointer("/pull_request/draft")
            .and_then(Value::as_bool)
            .unwrap_or(false)
            || super::is_draft_title(title),
        updated: parse_time(at(item, "/updated_at")).unwrap_or(0),
        review_requested: requested,
        mine: !requested,
        ..InboxPr::default()
    }
}

/// The combined status of `sha` in `project`; unknown when it can't be read or has none.
fn checks(api: &Api, project: &str, sha: &str) -> Checks {
    if sha.is_empty() || project.is_empty() {
        return Checks::Unknown;
    }
    let Ok(status) = api.get(&format!("repos/{project}/commits/{sha}/status")) else {
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
fn said_by_others(api: &Api, project: &str, number: u64, login: &str) -> Vec<i64> {
    let mut times = Vec::new();
    let sources = [
        (
            format!("repos/{project}/issues/{number}/comments"),
            "/created_at",
        ),
        (
            format!("repos/{project}/pulls/{number}/reviews"),
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
    fn reads_a_search_of_the_host() {
        let item = serde_json::json!({
            "number": 9, "title": "WIP: ünïcode", "html_url": "http://h/up/r/pulls/9",
            "user": {"login": "me"}, "updated_at": "2026-11-02T20:00:00+01:00",
            "pull_request": {"draft": false}, "repository": {"full_name": "up/r"}
        });
        let mine = read(&item, false);
        assert!(mine.mine && !mine.review_requested && mine.draft);
        assert_eq!((mine.project.as_str(), mine.number), ("up/r", 9));
        assert!(mine.updated > 0);
        let asked = read(&item, true);
        assert!(asked.review_requested && !asked.mine);
        let bare = read(&serde_json::json!({"number": 1, "user": null}), true);
        assert!(bare.project.is_empty() && !bare.draft);
    }
}
