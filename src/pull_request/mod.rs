//! One pull request in full, whichever forge it's on: its description, build status, commits and
//! comments, and the changes it proposes. The forges' modules (`forge/`) read it; this is what they
//! read it into, and what the page makes of it.

mod html;
mod markdown;
mod parse;
mod tables;

use std::path::Path;

use crate::forge::{self, Forge};
use crate::git::{self, ChangedFile, Credentials, Remote};
use crate::pr_threads::PrExtras;

pub(crate) use markdown::markdown_options;
pub use markdown::{images_to_links_except, safe_blocks, safe_local_markdown, safe_markdown};
pub(crate) use parse::{at, check, items, parse_time};
pub use tables::Block;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CheckState {
    Passed,
    Failed,
    Running,
    /// Skipped, cancelled by a newer run, neutral: neither good nor bad.
    #[default]
    Skipped,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Check {
    pub name: String,
    pub state: CheckState,
    /// The workflow it belongs to or the status's description; may be empty.
    pub detail: String,
    pub url: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PrCommit {
    pub oid: String,
    pub headline: String,
    pub author: String,
}

/// Which version of a line a comment is on.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DiffSide {
    /// As it was: a removed line.
    Left,
    /// As it is now: an added line, or one that didn't change.
    Right,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EntryKind {
    Comment,
    /// What the review said: "approved", "requested changes"…
    Review(&'static str),
    /// On a line of a file.
    Inline {
        path: String,
        line: Option<u64>,
        side: DiffSide,
        /// The line has changed since, so it isn't where the comment says any more.
        outdated: bool,
        resolved: bool,
    },
}

/// One thing said on the pull request.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub kind: EntryKind,
    pub author: String,
    pub at: i64,
    pub body: String,
    pub url: String,
    /// The review this is, if it can be dismissed.
    pub review_id: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PrDetails {
    /// The forge it was read from.
    pub forge: Option<Forge>,
    pub number: u64,
    pub title: String,
    pub body: String,
    pub state: String,
    pub draft: bool,
    pub author: String,
    pub url: String,
    pub base_branch: String,
    pub base_oid: String,
    pub head_branch: String,
    pub head_oid: String,
    /// Where the forge keeps the proposed commits (`refs/pull/12/head`), to fetch them by.
    pub head_ref: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub additions: u64,
    pub deletions: u64,
    pub changed_files: u64,
    pub mergeable: String,
    pub merge_state: String,
    pub review_decision: String,
    pub labels: Vec<String>,
    pub checks: Vec<Check>,
    /// The checks couldn't be read: why. Without it, no checks means none ran.
    pub checks_error: Option<String>,
    pub commits: Vec<PrCommit>,
    /// Oldest first.
    pub timeline: Vec<Entry>,
    /// The comments on lines couldn't be read: why.
    pub inline_error: Option<String>,
    /// Threads on lines and what the person logged in may do.
    pub extras: PrExtras,
}

/// The sent comments of the threads, for the conversation.
pub(crate) fn line_entries(extras: &PrExtras) -> Vec<Entry> {
    extras
        .threads
        .iter()
        .flat_map(|thread| {
            thread
                .comments
                .iter()
                .filter(|comment| !comment.pending)
                .map(move |comment| Entry {
                    kind: EntryKind::Inline {
                        path: thread.path.clone(),
                        line: thread.line,
                        side: thread.side,
                        outdated: thread.outdated,
                        resolved: thread.resolved,
                    },
                    author: comment.author.clone(),
                    at: comment.at,
                    body: comment.body.clone(),
                    url: comment.url.clone(),
                    review_id: None,
                })
        })
        .collect()
}

/// What the page says about the reviews so far.
pub fn reviews_label(decision: &str, open: bool, timeline: &[Entry]) -> &'static str {
    let reviewed = timeline
        .iter()
        .any(|entry| matches!(entry.kind, EntryKind::Review(_) | EntryKind::Inline { .. }));
    match decision {
        "APPROVED" => "Approved",
        "CHANGES_REQUESTED" => "Changes requested",
        "REVIEW_REQUIRED" => "Review required",
        _ if reviewed => "Reviewed",
        _ if open => "No reviews yet",
        _ => "No reviews",
    }
}

mod fetch;
pub use fetch::*;

/// How many checks passed, failed, are running and were skipped.
pub fn tally(checks: &[Check]) -> (usize, usize, usize, usize) {
    let count = |state| checks.iter().filter(|c| c.state == state).count();
    (
        count(CheckState::Passed),
        count(CheckState::Failed),
        count(CheckState::Running),
        count(CheckState::Skipped),
    )
}

/// The host and `owner/repo` of a pull request's URL: `…/pull/12` on GitHub, `…/pulls/12` on
/// Forgejo and Gitea, `…/-/merge_requests/12` on GitLab.
pub(crate) fn repo_of_url(url: &str) -> Option<(String, String)> {
    forge::location(repository_url(url)?)
}

/// The repository URL of a PR, retaining the authority and proxy prefix for identity checks.
pub(crate) fn repository_url(url: &str) -> Option<&str> {
    // The marker nearest the end: a repository may well be called `pull` or `pulls`.
    [
        "/-/merge_requests/",
        "/pulls/",
        "/pull/",
        "/pullrequest/",
        "/pull-requests/",
    ]
    .iter()
    .filter_map(|marker| url.rsplit_once(marker))
    .filter(|(_, number)| number.split(['/', '#', '?']).next().is_some_and(is_number))
    .map(|(repo, _)| repo)
    .max_by_key(|repo| repo.len())
}

fn is_number(text: &str) -> bool {
    !text.is_empty() && text.bytes().all(|b| b.is_ascii_digit())
}

/// The remote of the captured PR repository, including its web instance and forge path rules.
pub(crate) fn remote_for<'a>(
    remotes: &'a [Remote],
    url: &str,
    hosting: &forge::Hosting,
) -> Option<&'a str> {
    if !hosting.matches_pull_request(url) {
        return None;
    }
    remotes
        .iter()
        .find(|remote| hosting.matches_git_remote(&remote.url))
        .map(|remote| remote.name.as_str())
}

/// Whether a link from a pull request may be opened: a click must not run a local file or some
/// other program's address.
pub fn is_web_url(url: &str) -> bool {
    let url = url.trim_start().to_lowercase();
    url.starts_with("https://") || url.starts_with("http://")
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod bitbucket_fetch_tests;
