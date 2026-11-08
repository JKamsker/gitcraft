//! One pull request in full, through `gh pr view` and GitHub's GraphQL API.

use std::path::Path;

use serde_json::Value;

use super::{gh, login, threads};
use crate::forge::Forge;
use crate::pr_threads::PrExtras;
use crate::pull_request::{
    Entry, EntryKind, PrCommit, PrDetails, at, check, items, line_entries, parse_time, repo_of_url,
};

const FIELDS: &str = "number,title,body,state,isDraft,author,url,baseRefName,baseRefOid,\
    headRefName,headRefOid,createdAt,updatedAt,additions,deletions,changedFiles,mergeable,\
    mergeStateStatus,reviewDecision,labels,statusCheckRollup,commits,comments,reviews";

/// Reads the pull request numbered `number` of the repository at `repo`.
pub(super) fn details(repo: &Path, number: u64) -> Result<PrDetails, String> {
    let details = read_details(repo, number)?;
    // It's read in two goes. A push in between would put the threads' line numbers on another
    // commit than the one the page takes for the pull request's, so that goes again.
    if read_in_step(&details) {
        return Ok(details);
    }
    let details = read_details(repo, number)?;
    if read_in_step(&details) {
        Ok(details)
    } else {
        Err(crate::i18n::text(
            "backend-the-pull-request-changed-while-it-was-read-load-it-again-fe77a7",
        )
        .to_owned())
    }
}

/// Whether both goes of reading `details` saw the same commit (or the threads weren't read).
fn read_in_step(details: &PrDetails) -> bool {
    details.inline_error.is_some() || details.extras.head_oid == details.head_oid
}

fn read_details(repo: &Path, number: u64) -> Result<PrDetails, String> {
    let view = gh(repo, &["pr", "view", &number.to_string(), "--json", FIELDS])?;
    let mut details = parse_details(&view)?;
    match repo_of_url(&details.url) {
        Some((host, path)) => match threads::fetch(repo, &host, &path, number) {
            Ok(extras) => {
                details.timeline.extend(line_entries(&extras));
                details.extras = extras;
            }
            Err(e) => details.inline_error = Some(e),
        },
        None => {
            details.inline_error =
                Some(crate::i18n::text("backend-unexpected-pull-request-url-c63386").to_owned())
        }
    }
    details.timeline.sort_by_key(|entry| entry.at);
    Ok(details)
}

pub(crate) fn parse_details(json: &str) -> Result<PrDetails, String> {
    let v: Value = serde_json::from_str(json).map_err(|e| {
        crate::i18n::message(
            "backend-unexpected-output-of-gh-e-24588e",
            &[("detail", (e).to_string().into())],
        )
    })?;
    let text = |key: &str| at(&v, key).to_owned();
    let number = |key: &str| v.pointer(key).and_then(Value::as_u64).unwrap_or(0);
    let time = |key: &str| parse_time(at(&v, key)).unwrap_or(0);

    let mut timeline: Vec<Entry> = items(&v, "/comments").iter().map(comment).collect();
    timeline.extend(items(&v, "/reviews").iter().filter_map(review));
    timeline.sort_by_key(|entry| entry.at);
    Ok(PrDetails {
        forge: Some(Forge::GitHub),
        number: number("/number"),
        title: text("/title"),
        body: text("/body"),
        state: text("/state"),
        draft: v
            .pointer("/isDraft")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        author: login(&v, "/author/login"),
        url: text("/url"),
        base_branch: text("/baseRefName"),
        base_oid: text("/baseRefOid"),
        head_branch: text("/headRefName"),
        head_oid: text("/headRefOid"),
        head_ref: format!("refs/pull/{}/head", number("/number")),
        created_at: time("/createdAt"),
        updated_at: time("/updatedAt"),
        additions: number("/additions"),
        deletions: number("/deletions"),
        changed_files: number("/changedFiles"),
        mergeable: text("/mergeable"),
        merge_state: text("/mergeStateStatus"),
        review_decision: text("/reviewDecision"),
        labels: items(&v, "/labels")
            .iter()
            .map(|label| at(label, "/name").to_owned())
            .collect(),
        checks: items(&v, "/statusCheckRollup").iter().map(check).collect(),
        checks_error: None,
        commits: items(&v, "/commits").iter().map(commit).collect(),
        timeline,
        inline_error: None,
        extras: PrExtras::default(),
    })
}

fn commit(v: &Value) -> PrCommit {
    let author = match at(v, "/authors/0/login") {
        "" => at(v, "/authors/0/name"),
        name => name,
    };
    PrCommit {
        oid: at(v, "/oid").to_owned(),
        headline: at(v, "/messageHeadline").to_owned(),
        author: author.to_owned(),
    }
}

fn comment(v: &Value) -> Entry {
    Entry {
        kind: EntryKind::Comment,
        author: login(v, "/author/login"),
        at: parse_time(at(v, "/createdAt")).unwrap_or(0),
        body: at(v, "/body").to_owned(),
        url: at(v, "/url").to_owned(),
        review_id: None,
    }
}

/// A review, if it says anything: one that only holds comments on lines has no text of its own.
fn review(v: &Value) -> Option<Entry> {
    let body = at(v, "/body");
    let verdict = match at(v, "/state") {
        "APPROVED" => "approved",
        "CHANGES_REQUESTED" => crate::i18n::text("backend-requested-changes-7adfe7"),
        "DISMISSED" => crate::i18n::text("backend-had-a-review-dismissed-2ab8b3"),
        "COMMENTED" if !body.is_empty() => "reviewed",
        _ => return None,
    };
    let dismissable = matches!(at(v, "/state"), "APPROVED" | "CHANGES_REQUESTED");
    let id = at(v, "/id");
    Some(Entry {
        kind: EntryKind::Review(verdict),
        author: login(v, "/author/login"),
        at: parse_time(at(v, "/submittedAt")).unwrap_or(0),
        body: body.to_owned(),
        url: at(v, "/url").to_owned(),
        review_id: (dismissable && !id.is_empty()).then(|| id.to_owned()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pull_request::{CheckState, tally};

    const VIEW: &str = r#"{
        "number": 12, "title": "Add search", "body": "Adds **search**.", "state": "OPEN",
        "isDraft": true, "author": {"login": "ana"}, "url": "https://github.com/o/r/pull/12",
        "baseRefName": "master", "baseRefOid": "b0", "headRefName": "feat/search",
        "headRefOid": "h0", "createdAt": "2026-11-02T20:00:00Z", "updatedAt": "2026-11-02T21:00:00Z",
        "additions": 10, "deletions": 2, "changedFiles": 3, "mergeable": "CONFLICTING",
        "mergeStateStatus": "DIRTY", "reviewDecision": "CHANGES_REQUESTED",
        "labels": [{"name": "bug"}, {"name": "ui"}],
        "statusCheckRollup": [
            {"__typename": "CheckRun", "name": "test", "workflowName": "CI", "status": "COMPLETED",
             "conclusion": "SUCCESS", "detailsUrl": "https://ci/1"},
            {"__typename": "CheckRun", "name": "lint", "workflowName": "CI", "status": "COMPLETED",
             "conclusion": "FAILURE", "detailsUrl": "https://ci/2"},
            {"__typename": "CheckRun", "name": "e2e", "workflowName": "CI", "status": "IN_PROGRESS",
             "conclusion": "", "detailsUrl": "https://ci/3"},
            {"__typename": "CheckRun", "name": "docs", "workflowName": "CI", "status": "COMPLETED",
             "conclusion": "SKIPPED", "detailsUrl": "https://ci/4"},
            {"__typename": "CheckRun", "name": "old", "workflowName": "CI", "status": "COMPLETED",
             "conclusion": "TIMED_OUT", "detailsUrl": "https://ci/5"},
            {"__typename": "StatusContext", "context": "ci/legacy", "state": "PENDING",
             "description": "Waiting", "targetUrl": "https://ci/6"},
            {"__typename": "StatusContext", "context": "ci/ok", "state": "SUCCESS",
             "description": "", "targetUrl": "https://ci/7"}
        ],
        "commits": [{"oid": "c1", "messageHeadline": "feat: search",
                     "authors": [{"login": "ana", "name": "Ana"}]}],
        "comments": [{"author": {"login": "bo"}, "body": "Nice", "createdAt": "2026-11-02T20:30:00Z",
                      "url": "https://github.com/o/r/pull/12#c1"}],
        "reviews": [
            {"author": {"login": "cy"}, "body": "Please fix", "state": "CHANGES_REQUESTED",
             "submittedAt": "2026-11-02T20:10:00Z"},
            {"author": {"login": "cy"}, "body": "", "state": "COMMENTED",
             "submittedAt": "2026-11-02T20:11:00Z"},
            {"id": "PRR_di", "author": {"login": "di"}, "body": "", "state": "APPROVED",
             "submittedAt": "2026-11-02T20:50:00Z"}
        ]
    }"#;

    fn parsed() -> PrDetails {
        parse_details(VIEW).unwrap_or_default()
    }

    #[test]
    fn reads_the_pull_request_itself() {
        let pr = parsed();
        assert_eq!(pr.number, 12);
        assert_eq!(pr.title, "Add search");
        assert_eq!(pr.body, "Adds **search**.");
        assert!(pr.draft);
        assert_eq!(pr.author, "ana");
        assert_eq!(
            (pr.base_branch.as_str(), pr.base_oid.as_str()),
            ("master", "b0")
        );
        assert_eq!(
            (pr.head_branch.as_str(), pr.head_oid.as_str()),
            ("feat/search", "h0")
        );
        assert_eq!((pr.additions, pr.deletions, pr.changed_files), (10, 2, 3));
        assert_eq!(pr.mergeable, "CONFLICTING");
        assert_eq!(pr.review_decision, "CHANGES_REQUESTED");
        assert_eq!(pr.labels, ["bug", "ui"]);
        assert_eq!(pr.commits[0].headline, "feat: search");
        assert_eq!(pr.commits[0].author, "ana");
    }

    #[test]
    fn output_that_is_not_json_is_an_error() {
        assert!(parse_details("oops").is_err());
    }

    #[test]
    fn understands_both_kinds_of_check() {
        let pr = parsed();
        let states: Vec<(&str, CheckState)> = pr
            .checks
            .iter()
            .map(|c| (c.name.as_str(), c.state))
            .collect();
        assert_eq!(
            states,
            [
                ("test", CheckState::Passed),
                ("lint", CheckState::Failed),
                ("e2e", CheckState::Running),
                ("docs", CheckState::Skipped),
                ("old", CheckState::Failed),
                ("ci/legacy", CheckState::Running),
                ("ci/ok", CheckState::Passed),
            ]
        );
        assert_eq!(pr.checks[0].url, "https://ci/1");
        assert_eq!(pr.checks[0].detail, "CI");
        assert_eq!(pr.checks[5].detail, "Waiting");
    }

    #[test]
    fn counts_the_checks_by_outcome() {
        assert_eq!(tally(&parsed().checks), (2, 2, 2, 1));
    }

    #[test]
    fn the_timeline_has_comments_and_reviews_that_say_something() {
        let pr = parsed();
        let said: Vec<(&str, &EntryKind)> = pr
            .timeline
            .iter()
            .map(|e| (e.author.as_str(), &e.kind))
            .collect();
        assert_eq!(
            said,
            [
                ("cy", &EntryKind::Review("requested changes")),
                ("bo", &EntryKind::Comment),
                ("di", &EntryKind::Review("approved")),
            ]
        );
    }

    #[test]
    fn approvals_and_requests_for_changes_can_be_dismissed() {
        let pr = parsed();
        let approval = pr.timeline.iter().find(|e| e.author == "di");
        assert_eq!(
            approval.and_then(|e| e.review_id.as_deref()),
            Some("PRR_di")
        );
        // Without an id there's nothing to refer to.
        let request = pr.timeline.iter().find(|e| e.author == "cy");
        assert_eq!(request.and_then(|e| e.review_id.as_deref()), None);
    }

    #[test]
    fn threads_read_on_another_commit_are_not_shown_on_this_one() {
        let mut details = PrDetails {
            head_oid: "h1".to_owned(),
            ..PrDetails::default()
        };
        details.extras.head_oid = "h1".to_owned();
        assert!(read_in_step(&details));
        details.extras.head_oid = "h0".to_owned();
        assert!(!read_in_step(&details));
        // Without threads there are no line numbers to put on the wrong commit.
        details.inline_error = Some("offline".to_owned());
        assert!(read_in_step(&details));
    }
}
