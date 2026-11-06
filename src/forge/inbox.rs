//! The pull requests that wait for you, as one forge sees them in one repository: those whose
//! review is asked of you, and yours, with how their checks went and when others last said
//! something on them. Which of them need you now is worked out against what you have seen
//! ([`needs_you`]), so that a comment you read doesn't keep one in the inbox.

use serde_json::Value;

use crate::pull_request::{at, parse_time};

/// How a pull request's checks stand, all of them together.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Checks {
    /// The forge doesn't say, or there are none.
    #[default]
    Unknown,
    Passing,
    Pending,
    Failing,
}

/// What others said on a pull request, as precisely as the forge tells it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Activity {
    /// When each comment or review by someone else was made (Unix seconds).
    At(Vec<i64>),
    /// Only how many comments it has (GitLab's list), anyone's.
    Count(u64),
}

impl Default for Activity {
    fn default() -> Self {
        Activity::At(Vec::new())
    }
}

/// One pull request of the inbox.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct InboxPr {
    pub number: u64,
    pub title: String,
    pub url: String,
    pub author: String,
    pub draft: bool,
    /// When it last changed (Unix seconds), 0 when not said.
    pub updated: i64,
    /// Your review is asked for.
    pub review_requested: bool,
    /// You opened it.
    pub mine: bool,
    pub checks: Checks,
    pub activity: Activity,
}

/// What was seen of a pull request when it was last opened from the inbox.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct Seen {
    /// When (Unix seconds).
    pub at: i64,
    /// How many comments it had then (for a forge that only counts them).
    pub count: u64,
}

/// Why a pull request is in the inbox.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reason {
    ReviewRequested,
    ChecksFailing,
    /// Comments or reviews by others since it was last opened from the inbox.
    NewComments(u64),
}

impl Reason {
    pub fn label(self) -> String {
        match self {
            Reason::ReviewRequested => "review requested".to_owned(),
            Reason::ChecksFailing => "checks failing".to_owned(),
            Reason::NewComments(1) => "1 new comment".to_owned(),
            Reason::NewComments(n) => format!("{n} new comments"),
        }
    }
}

/// How many comments by others came since `seen` (all of them, never seen).
pub fn new_comments(activity: &Activity, seen: Option<Seen>) -> u64 {
    match (activity, seen) {
        (Activity::At(times), Some(seen)) => times.iter().filter(|t| **t > seen.at).count() as u64,
        (Activity::At(times), None) => times.len() as u64,
        (Activity::Count(n), Some(seen)) => n.saturating_sub(seen.count),
        (Activity::Count(n), None) => *n,
    }
}

/// Why `pr` needs you now: nothing for one of yours that is fine and has nothing new.
pub fn needs_you(pr: &InboxPr, seen: Option<Seen>) -> Vec<Reason> {
    let mut reasons = Vec::new();
    if pr.review_requested && !pr.mine {
        reasons.push(Reason::ReviewRequested);
    }
    if pr.mine {
        if pr.checks == Checks::Failing {
            reasons.push(Reason::ChecksFailing);
        }
        let new = new_comments(&pr.activity, seen);
        if new > 0 {
            reasons.push(Reason::NewComments(new));
        }
    }
    reasons
}

/// What is seen of `pr` now, to remember once it is opened.
pub fn seen_now(pr: &InboxPr, now: i64) -> Seen {
    let count = match &pr.activity {
        Activity::Count(n) => *n,
        Activity::At(times) => times.len() as u64,
    };
    Seen { at: now, count }
}

/// The checks of GitHub's `statusCheckRollup`: check runs (`status`, `conclusion`) and commit
/// statuses (`state`).
pub(super) fn github_checks(rollup: Option<&Value>) -> Checks {
    let Some(items) = rollup.and_then(Value::as_array).filter(|a| !a.is_empty()) else {
        return Checks::Unknown;
    };
    let mut pending = false;
    for item in items {
        let word = [at(item, "/conclusion"), at(item, "/state")]
            .into_iter()
            .find(|w| !w.is_empty())
            .unwrap_or_default()
            .to_uppercase();
        match word.as_str() {
            "FAILURE" | "ERROR" | "TIMED_OUT" | "CANCELLED" | "ACTION_REQUIRED"
            | "STARTUP_FAILURE" => return Checks::Failing,
            "PENDING" | "EXPECTED" => pending = true,
            "" => pending |= !at(item, "/status").eq_ignore_ascii_case("COMPLETED"),
            _ => {}
        }
    }
    if pending {
        Checks::Pending
    } else {
        Checks::Passing
    }
}

/// A Forgejo / Gitea combined status's `state`.
pub(super) fn status_checks(state: &str) -> Checks {
    match state {
        "failure" | "error" => Checks::Failing,
        "pending" => Checks::Pending,
        "success" => Checks::Passing,
        _ => Checks::Unknown,
    }
}

/// `gh pr list --json number,title,url,author,isDraft,updatedAt[,statusCheckRollup,comments,
/// reviews]`: those asked of you (`requested`), or yours.
pub(super) fn parse_github(json: &str, requested: bool) -> Result<Vec<InboxPr>, String> {
    let value: Value =
        serde_json::from_str(json).map_err(|e| format!("unexpected gh output: {e}"))?;
    Ok(value
        .as_array()
        .map_or(&[][..], Vec::as_slice)
        .iter()
        .map(|item| {
            let author = at(item, "/author/login").to_owned();
            let by_others = |list: &str, time: &str| -> Vec<i64> {
                item.get(list)
                    .and_then(Value::as_array)
                    .map_or(&[][..], Vec::as_slice)
                    .iter()
                    .filter(|c| at(c, "/author/login") != author)
                    .filter_map(|c| parse_time(at(c, time)))
                    .collect()
            };
            let mut times = by_others("comments", "/createdAt");
            times.extend(by_others("reviews", "/submittedAt"));
            InboxPr {
                number: item.get("number").and_then(Value::as_u64).unwrap_or(0),
                title: at(item, "/title").to_owned(),
                url: at(item, "/url").to_owned(),
                draft: item
                    .get("isDraft")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
                updated: parse_time(at(item, "/updatedAt")).unwrap_or(0),
                review_requested: requested,
                mine: !requested,
                checks: github_checks(item.get("statusCheckRollup")),
                activity: Activity::At(times),
                author,
            }
        })
        .collect())
}

/// `glab mr list --output json`: those whose review is asked of you (`requested`), or yours.
pub(super) fn parse_gitlab(json: &str, requested: bool) -> Result<Vec<InboxPr>, String> {
    let value: Value =
        serde_json::from_str(json).map_err(|e| format!("unexpected glab output: {e}"))?;
    Ok(value
        .as_array()
        .map_or(&[][..], Vec::as_slice)
        .iter()
        .map(|item| {
            let pipeline = at(item, "/head_pipeline/status");
            InboxPr {
                number: item.get("iid").and_then(Value::as_u64).unwrap_or(0),
                title: at(item, "/title").to_owned(),
                url: at(item, "/web_url").to_owned(),
                author: at(item, "/author/username").to_owned(),
                draft: item.get("draft").and_then(Value::as_bool).unwrap_or(false),
                updated: parse_time(at(item, "/updated_at")).unwrap_or(0),
                review_requested: requested,
                mine: !requested,
                checks: match pipeline {
                    "failed" => Checks::Failing,
                    "success" => Checks::Passing,
                    "running" | "pending" | "created" => Checks::Pending,
                    _ => Checks::Unknown,
                },
                activity: Activity::Count(
                    item.get("user_notes_count")
                        .and_then(Value::as_u64)
                        .unwrap_or(0),
                ),
            }
        })
        .collect())
}

/// The pull requests of `lists` (the forge's two lists), each once: one both asked of you and
/// yours is yours.
pub(super) fn merged(lists: Vec<Vec<InboxPr>>) -> Vec<InboxPr> {
    let mut all: Vec<InboxPr> = Vec::new();
    for pr in lists.into_iter().flatten() {
        match all
            .iter_mut()
            .find(|p| p.url == pr.url && p.number == pr.number)
        {
            Some(known) => {
                known.review_requested |= pr.review_requested;
                if pr.mine {
                    let requested = known.review_requested;
                    *known = pr;
                    known.review_requested = requested;
                }
            }
            None => all.push(pr),
        }
    }
    all
}

#[cfg(test)]
#[path = "inbox_tests.rs"]
mod tests;
