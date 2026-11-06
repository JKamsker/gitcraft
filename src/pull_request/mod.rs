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

/// The changes a pull request proposes, as its web page shows them: from the merge base.
#[derive(Clone, Debug)]
pub struct PrFiles {
    pub from: String,
    pub to: String,
    pub files: Vec<ChangedFile>,
}

/// Finds the changes of `details`, fetching its commits first if they aren't here yet.
pub fn files(
    repo: &Path,
    details: &PrDetails,
    remotes: &[Remote],
    credentials: Option<&Credentials>,
) -> Result<PrFiles, String> {
    // What the forge says goes to git as refspecs: anything but a commit id or a branch name could
    // make the fetch write a ref (`+x:refs/heads/main`) or read as an option.
    if !is_oid(&details.head_oid) || !is_oid(&details.base_oid) {
        return Err("the forge named commits that aren't commit ids".to_owned());
    }
    if !is_branch_name(&details.base_branch) {
        return Err(format!(
            "the forge named a base branch git can't take: {:?}",
            details.base_branch
        ));
    }
    let has = |oid: &str| {
        git::run(
            Some(repo),
            &["cat-file", "-e", &format!("{oid}^{{commit}}")],
        )
        .is_ok()
    };
    if !has(&details.head_oid) || !has(&details.base_oid) {
        let remote = remote_for(remotes, &details.url)
            .ok_or("no remote of this repository is the one the pull request belongs to")?;
        // No refs and no FETCH_HEAD: nothing here changes but the objects being available.
        let base = format!("refs/heads/{}", details.base_branch);
        let mut args = vec!["fetch", "--no-tags", "--no-write-fetch-head", remote];
        if is_branch_ref(&details.head_ref) {
            args.push(&details.head_ref);
        }
        args.push(&base);
        // The branches may have moved on since the details were read (a force push even leaves
        // the old commits out), so the very commits are asked for too.
        args.extend([details.head_oid.as_str(), details.base_oid.as_str()]);
        git::run_with(Some(repo), &args, credentials)?;
    }
    let from = git::merge_base(repo, &details.base_oid, &details.head_oid)?;
    let files = git::diff_files(repo, &from, Some(&details.head_oid))?;
    Ok(PrFiles {
        from,
        to: details.head_oid.clone(),
        files,
    })
}

/// Whether `oid` is a whole commit id (SHA-1 or SHA-256), nothing else.
fn is_oid(oid: &str) -> bool {
    matches!(oid.len(), 40 | 64) && oid.bytes().all(|b| b.is_ascii_hexdigit())
}

/// Whether `name` is a branch name git takes, and nothing a refspec or an option could be made of:
/// `git check-ref-format`'s rules.
fn is_branch_name(name: &str) -> bool {
    let forbidden = |c: char| c.is_ascii_control() || " ~^:?*[\\".contains(c);
    !name.is_empty()
        && !name.starts_with(['-', '/', '.'])
        && !name.ends_with(['/', '.'])
        && !name.split('/').any(|part| part.ends_with(".lock"))
        && !name.contains("..")
        && !name.contains("//")
        && !name.contains("@{")
        && !name.contains("/.")
        && !name.chars().any(forbidden)
}

/// Whether `r` is a ref the fetch may name, like the `refs/pull/12/head` the forges' modules set.
fn is_branch_ref(r: &str) -> bool {
    r.strip_prefix("refs/").is_some_and(is_branch_name)
}

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
    // The marker nearest the end: a repository may well be called `pull` or `pulls`.
    let repo = ["/-/merge_requests/", "/pulls/", "/pull/"]
        .iter()
        .filter_map(|marker| url.rsplit_once(marker))
        .filter(|(_, number)| number.split(['/', '#', '?']).next().is_some_and(is_number))
        .map(|(repo, _)| repo)
        .max_by_key(|repo| repo.len())?;
    forge::location(repo)
}

fn is_number(text: &str) -> bool {
    !text.is_empty() && text.bytes().all(|b| b.is_ascii_digit())
}

/// The remote of the repository a pull request's URL belongs to. Forges don't mind the case of
/// owner and name, so a clone of `Owner/Repo` may well be from `owner/repo`.
pub(crate) fn remote_for<'a>(remotes: &'a [Remote], url: &str) -> Option<&'a str> {
    let (host, path) = repo_of_url(url)?;
    remotes
        .iter()
        .find(|remote| {
            forge::location(&remote.url)
                .is_some_and(|(h, p)| h == host && p.eq_ignore_ascii_case(&path))
        })
        .map(|remote| remote.name.as_str())
}

/// Whether a link from a pull request may be opened: a click must not run a local file or some
/// other program's address.
pub fn is_web_url(url: &str) -> bool {
    let url = url.trim_start().to_lowercase();
    url.starts_with("https://") || url.starts_with("http://")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_the_repository_of_a_pull_request() {
        assert_eq!(
            repo_of_url("https://github.com/o/r/pull/12"),
            Some(("github.com".to_owned(), "o/r".to_owned()))
        );
        assert_eq!(
            repo_of_url("https://codeberg.org/o/r/pulls/12"),
            Some(("codeberg.org".to_owned(), "o/r".to_owned()))
        );
        assert_eq!(
            repo_of_url("https://gitlab.com/g/sub/r/-/merge_requests/3"),
            Some(("gitlab.com".to_owned(), "g/sub/r".to_owned()))
        );
        assert_eq!(repo_of_url("https://github.com/o/r"), None);
        // A repository may be called what the markers say.
        assert_eq!(
            repo_of_url("https://codeberg.org/o/pull/pulls/3"),
            Some(("codeberg.org".to_owned(), "o/pull".to_owned()))
        );
        assert_eq!(
            repo_of_url("https://github.com/acme/pulls/pull/3"),
            Some(("github.com".to_owned(), "acme/pulls".to_owned()))
        );
        assert_eq!(
            repo_of_url("https://codeberg.org/o/pulls/pulls/3#issuecomment-1"),
            Some(("codeberg.org".to_owned(), "o/pulls".to_owned()))
        );
        assert_eq!(
            repo_of_url("https://github.com/o/pull/pull/12"),
            Some(("github.com".to_owned(), "o/pull".to_owned()))
        );
    }

    fn remote(name: &str, url: &str) -> Remote {
        Remote {
            name: name.to_owned(),
            url: url.to_owned(),
        }
    }

    #[test]
    fn finds_the_remote_of_a_pull_request_whatever_its_url_looks_like() {
        let remotes = [
            remote("fork", "https://github.com/me/r.git"),
            remote("up", "git@github.com:o/r.git"),
        ];
        assert_eq!(
            remote_for(&remotes, "https://github.com/o/r/pull/1"),
            Some("up")
        );
        assert_eq!(remote_for(&remotes, "https://github.com/x/y/pull/1"), None);
        assert_eq!(
            remote_for(&remotes, "https://github.com/O/R/pull/1"),
            Some("up")
        );
    }

    #[test]
    fn only_full_commit_ids_go_to_git() {
        assert!(is_oid(&"a1".repeat(20)));
        assert!(is_oid(&"F".repeat(64)));
        assert!(!is_oid(""));
        assert!(!is_oid("h0"));
        assert!(!is_oid(&format!("--upload-pack={}", "a".repeat(26))));
        assert!(!is_oid(&"g".repeat(40)));
    }

    fn entry(kind: EntryKind) -> Entry {
        Entry {
            kind,
            author: "ana".to_owned(),
            at: 0,
            body: String::new(),
            url: String::new(),
            review_id: None,
        }
    }

    #[test]
    fn a_review_without_a_verdict_still_counts_as_a_review() {
        let reviewed = [entry(EntryKind::Review("reviewed"))];
        assert_eq!(reviews_label("", true, &reviewed), "Reviewed");
        let on_a_line = [entry(EntryKind::Inline {
            path: "a.rs".to_owned(),
            line: Some(1),
            side: DiffSide::Right,
            outdated: false,
            resolved: false,
        })];
        assert_eq!(reviews_label("", true, &on_a_line), "Reviewed");
        let talk = [entry(EntryKind::Comment)];
        assert_eq!(reviews_label("", true, &talk), "No reviews yet");
    }

    #[test]
    fn a_verdict_says_more_than_that_there_was_a_review() {
        let reviewed = [entry(EntryKind::Review("approved"))];
        assert_eq!(reviews_label("APPROVED", true, &reviewed), "Approved");
    }

    #[test]
    fn no_review_is_coming_on_a_finished_pull_request() {
        assert_eq!(reviews_label("", false, &[]), "No reviews");
    }

    #[test]
    fn only_commit_ids_and_branch_names_reach_the_fetch() {
        assert!(is_oid(&"a".repeat(40)) && is_oid(&"0".repeat(64)));
        for bad in [
            "",
            "abc",
            "+refs/pull/1/head:refs/heads/main",
            "--update-head-ok",
        ] {
            assert!(!is_oid(bad), "{bad}");
        }
        for good in ["main", "feat/x", "release-1.2", "a@b"] {
            assert!(is_branch_name(good), "{good}");
        }
        for bad in [
            "",
            "-x",
            "x:refs/heads/main",
            "a..b",
            "a b",
            "x.lock",
            "x.lock/y",
            "/x",
            "x/",
            "a@{1}",
            "a/.b",
        ] {
            assert!(!is_branch_name(bad), "{bad}");
        }
        assert!(is_branch_ref("refs/pull/12/head"));
        assert!(!is_branch_ref("+refs/pull/1/head:refs/heads/main"));
        assert!(!is_branch_ref(""));
    }

    #[test]
    fn a_forge_naming_anything_but_commits_gets_nothing_fetched() {
        let details = PrDetails {
            head_oid: "+refs/pull/1/head:refs/heads/main".to_owned(),
            base_oid: "--update-head-ok".to_owned(),
            base_branch: "main".to_owned(),
            ..PrDetails::default()
        };
        let refused = files(Path::new("."), &details, &[], None);
        assert!(refused.is_err_and(|e| e.contains("aren't commit ids")));
        let details = PrDetails {
            head_oid: "a".repeat(40),
            base_oid: "b".repeat(40),
            base_branch: "x:refs/heads/main".to_owned(),
            ..PrDetails::default()
        };
        assert!(
            files(Path::new("."), &details, &[], None).is_err_and(|e| e.contains("can't take"))
        );
    }

    #[test]
    fn only_web_links_may_be_opened() {
        assert!(is_web_url("https://github.com/o/r"));
        assert!(is_web_url("HTTP://example.com"));
        assert!(!is_web_url("file:///C:/Windows/System32/calc.exe"));
        assert!(!is_web_url("javascript:alert(1)"));
        assert!(!is_web_url("ms-settings:"));
        assert!(!is_web_url(""));
    }
}
