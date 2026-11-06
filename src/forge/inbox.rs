//! The pull requests that wait for you, as one forge sees them (in one repository, or on a
//! whole host): those whose review is asked of you, and yours, with how their checks went and
//! when others last said something on them. Which of them need you now is worked out against
//! what you have seen ([`needs_you`]), so that a comment you read doesn't keep one in the inbox.

use std::time::Duration;

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

/// One pull request of the inbox.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct InboxPr {
    pub number: u64,
    pub title: String,
    pub url: String,
    pub author: String,
    /// `owner/repo` of the repository it is in, when the forge says (a list of a whole host).
    pub project: String,
    pub draft: bool,
    /// When it last changed (Unix seconds), 0 when not said.
    pub updated: i64,
    /// Your review is asked for.
    pub review_requested: bool,
    /// You opened it.
    pub mine: bool,
    pub checks: Checks,
    /// When others commented on it or reviewed it (Unix seconds, by the forge's clock); read
    /// for yours only.
    pub said: Vec<i64>,
}

/// What one forge answered: the pull requests, and what was left unread (a list cut at its
/// length, yours past the first few not read in full).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Answer {
    pub prs: Vec<InboxPr>,
    pub cut: Vec<String>,
}

/// How long one run of `gh` or `glab` for the inbox may take before it is stopped: nobody
/// waits on it, and one that hangs would only be asked again on top of it.
pub const TOOL_FOR: Duration = Duration::from_secs(45);

/// How many of your pull requests get their checks and comments read, each with requests of
/// its own.
pub const MAX_MINE: usize = 20;

/// What was seen of a pull request: what others said up to then counts as read.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct Seen {
    /// Up to when (Unix seconds, by the forge's clock).
    pub at: i64,
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
pub fn new_comments(said: &[i64], seen: Option<Seen>) -> u64 {
    match seen {
        Some(seen) => said.iter().filter(|t| **t > seen.at).count() as u64,
        None => said.len() as u64,
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
        let new = new_comments(&pr.said, seen);
        if new > 0 {
            reasons.push(Reason::NewComments(new));
        }
    }
    reasons
}

/// What is seen of `pr` now (opened from the inbox, or met for the first time): up to the
/// newest thing said on it, or its last change if later (whatever was said then is in it), by
/// the forge's clock, which the computer's may be off from; never less than was seen `before`.
pub fn seen_now(pr: &InboxPr, before: Option<Seen>) -> Seen {
    let newest = pr.said.iter().copied().max().unwrap_or(0).max(pr.updated);
    Seen {
        at: newest.max(before.map_or(0, |s| s.at)),
    }
}

/// Whether asking again soon would only get the same error: the tool isn't installed, or
/// there is no login to ask with. Those are left alone until ↻ asks.
pub fn gives_up(err: &str) -> bool {
    let e = err.to_lowercase();
    e.contains("isn't installed")
        || e.contains("isn't signed in")
        || e.contains("auth login")
        || is_no_login(err)
}

/// Whether `err` says gitcraft knows no login for a Forgejo or Gitea host: fine for
/// repositories only read from, so it's said quietly.
pub fn is_no_login(err: &str) -> bool {
    err.starts_with("no ") && err.contains(" login known for ")
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
            let mut said = by_others("comments", "/createdAt");
            said.extend(by_others("reviews", "/submittedAt"));
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
                said,
                author,
                ..InboxPr::default()
            }
        })
        .collect())
}

/// `glab mr list --output json`: those whose review is asked of you (`requested`), or yours.
/// The list says nothing of pipelines, nor who wrote the notes it counts: those are read per
/// merge request ([`gitlab_pipeline`], [`gitlab_notes`]).
pub(super) fn parse_gitlab(json: &str, requested: bool) -> Result<Vec<InboxPr>, String> {
    let value: Value =
        serde_json::from_str(json).map_err(|e| format!("unexpected glab output: {e}"))?;
    Ok(value
        .as_array()
        .map_or(&[][..], Vec::as_slice)
        .iter()
        .map(|item| InboxPr {
            number: item.get("iid").and_then(Value::as_u64).unwrap_or(0),
            title: at(item, "/title").to_owned(),
            url: at(item, "/web_url").to_owned(),
            author: at(item, "/author/username").to_owned(),
            draft: item.get("draft").and_then(Value::as_bool).unwrap_or(false),
            updated: parse_time(at(item, "/updated_at")).unwrap_or(0),
            review_requested: requested,
            mine: !requested,
            ..InboxPr::default()
        })
        .collect())
}

/// The checks of one GitLab merge request (`projects/:id/merge_requests/:iid`): its head
/// pipeline's.
pub(super) fn gitlab_pipeline(json: &str) -> Checks {
    let Ok(value) = serde_json::from_str::<Value>(json) else {
        return Checks::Unknown;
    };
    match at(&value, "/head_pipeline/status") {
        "failed" => Checks::Failing,
        "success" => Checks::Passing,
        "running" | "pending" | "created" | "preparing" | "waiting_for_resource" | "scheduled" => {
            Checks::Pending
        }
        _ => Checks::Unknown,
    }
}

/// When others wrote the notes of a merge request (`…/notes`): GitLab's own notes ("added 1
/// commit") and the author's aren't news to the author.
pub(super) fn gitlab_notes(json: &str, author: &str) -> Vec<i64> {
    let Ok(value) = serde_json::from_str::<Value>(json) else {
        return Vec::new();
    };
    value
        .as_array()
        .map_or(&[][..], Vec::as_slice)
        .iter()
        .filter(|n| !n.get("system").and_then(Value::as_bool).unwrap_or(false))
        .filter(|n| at(n, "/author/username") != author)
        .filter_map(|n| parse_time(at(n, "/created_at")))
        .collect()
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

/// "only the 50 latest asking your review were read", when a list came back as long as it
/// was allowed to be.
pub(super) fn cut_note(len: usize, max: usize, what: &str) -> Option<String> {
    (len >= max).then(|| format!("only the {max} latest {what} were read"))
}

#[cfg(test)]
#[path = "inbox_tests.rs"]
mod tests;
