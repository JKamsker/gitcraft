//! One pull request in full, through the REST API: the pull request, its conversation, its
//! reviews with their comments on lines, its commits and the statuses of its last commit, and
//! what the person logged in may do.
//!
//! These forges have no threads as such: comments on the same line of the same file make one
//! conversation. Their line numbers are the ones of the commit they were written on, so a thread
//! from an older commit than the pull request's last is taken as outdated: the line may not be
//! where it was. The API can't resolve conversations either, so the page offers no button for it.

use serde_json::Value;

use super::{Api, is_draft_title, user};
use crate::forge::{Forge, Hosting};
use crate::pr_threads::{PrExtras, Thread, ThreadComment};
use crate::pull_request::{
    Check, CheckState, DiffSide, Entry, EntryKind, PrCommit, PrDetails, at, items, line_entries,
    parse_time,
};

/// Everything the API was asked for one pull request.
pub(super) struct Answers {
    pub pr: Value,
    pub repo: Value,
    pub viewer: Option<String>,
    pub comments: Vec<Value>,
    /// Each review, with its comments on lines.
    pub reviews: Vec<(Value, Vec<Value>)>,
    pub commits: Vec<Value>,
    /// The statuses of the last commit, or why they couldn't be read.
    pub status: Result<Value, String>,
}

pub(super) fn read(
    api: &Api,
    forge: Forge,
    hosting: &Hosting,
    number: u64,
) -> Result<PrDetails, String> {
    let path = &hosting.path;
    let pr = api.get(&format!("repos/{path}/pulls/{number}"))?;
    let head = at(&pr, "/head/sha").to_owned();
    let mut answers = Answers {
        pr,
        comments: api.get_all(&format!("repos/{path}/issues/{number}/comments"), 1000)?,
        commits: api
            .get_all(
                &format!("repos/{path}/pulls/{number}/commits?verification=false&files=false"),
                250,
            )
            .unwrap_or_default(),
        status: match api.get(&format!("repos/{path}/commits/{head}/status")) {
            // A commit nothing reported on can be one the forge has no statuses for at all.
            Err(failure) if failure.status == Some(404) => Ok(Value::Null),
            status => status.map_err(String::from),
        },
        repo: Value::Null,
        viewer: None,
        reviews: Vec::new(),
    };
    // What the page needs to review and merge. Without it, the pull request still shows.
    let extras = (|| -> Result<(), String> {
        answers.repo = api.get(&format!("repos/{path}"))?;
        if api.logged_in() {
            let me = api.get("user")?;
            answers.viewer = Some(at(&me, "/login").to_owned()).filter(|v| !v.is_empty());
        }
        let reviews = api.get_all(&format!("repos/{path}/pulls/{number}/reviews"), 1000)?;
        for review in reviews {
            let id = review.get("id").and_then(Value::as_u64).unwrap_or(0);
            let has_comments = review
                .get("comments_count")
                .and_then(Value::as_u64)
                .is_none_or(|n| n > 0);
            let comments = if has_comments && id > 0 {
                let endpoint = format!("repos/{path}/pulls/{number}/reviews/{id}/comments");
                match api.get(&endpoint)? {
                    Value::Array(comments) => comments,
                    _ => Vec::new(),
                }
            } else {
                Vec::new()
            };
            answers.reviews.push((review, comments));
        }
        Ok(())
    })();
    let mut details = assemble(&answers, forge, &hosting.web);
    if let Err(e) = extras {
        details.inline_error = Some(e);
        details.extras = PrExtras::default();
        details
            .timeline
            .retain(|e| !matches!(e.kind, EntryKind::Inline { .. }));
    }
    Ok(details)
}

/// The page's pull request, from what the API answered.
pub(super) fn assemble(answers: &Answers, forge: Forge, web: &str) -> PrDetails {
    let pr = &answers.pr;
    let text = |pointer: &str| at(pr, pointer).to_owned();
    let number = |pointer: &str| pr.pointer(pointer).and_then(Value::as_u64).unwrap_or(0);
    let flag = |v: &Value, pointer: &str| v.pointer(pointer).and_then(Value::as_bool);
    let time = |pointer: &str| parse_time(at(pr, pointer)).unwrap_or(0);
    let open = at(pr, "/state") == "open";
    let state = if flag(pr, "/merged") == Some(true) {
        "MERGED"
    } else if open {
        "OPEN"
    } else {
        "CLOSED"
    };
    let head_oid = text("/head/sha");
    let title = text("/title");
    let draft = flag(pr, "/draft") == Some(true) || is_draft_title(&title);

    // A site admin is shown everyone's pending reviews: only the viewer's are the viewer's to
    // add to, send or discard, and the others' are nobody else's to read.
    let viewer = answers.viewer.as_deref();
    let reviews: Vec<(Value, Vec<Value>)> = answers
        .reviews
        .iter()
        .filter(|(review, _)| at(review, "/state") != "PENDING" || is_by(review, viewer))
        .cloned()
        .collect();
    let threads = threads(&reviews, &head_oid, at(pr, "/merge_base"), viewer.is_some());
    let pending = reviews
        .iter()
        .map(|(review, _)| review)
        .find(|review| at(review, "/state") == "PENDING");
    let repo = &answers.repo;
    let permission = if flag(repo, "/permissions/admin") == Some(true) {
        "ADMIN"
    } else if flag(repo, "/permissions/push") == Some(true) {
        "WRITE"
    } else if flag(repo, "/permissions/pull") == Some(true) {
        "READ"
    } else {
        ""
    };
    let head_repo = at(pr, "/head/repo/full_name");
    let extras = PrExtras {
        pr_id: pr
            .get("id")
            .and_then(Value::as_u64)
            .filter(|_| !repo.is_null())
            .map_or(String::new(), |id| id.to_string()),
        viewer: answers.viewer.clone(),
        permission: permission.to_owned(),
        merge_allowed: flag(repo, "/allow_merge_commits").unwrap_or(false),
        squash_allowed: flag(repo, "/allow_squash_merge").unwrap_or(false),
        rebase_allowed: flag(repo, "/allow_rebase").unwrap_or(false),
        // The forge deletes a branch on merging only when asked; gitcraft asks itself, after
        // checking that nothing was pushed to it since.
        delete_branch_on_merge: false,
        head_repo: (!head_repo.is_empty()).then(|| head_repo.to_owned()),
        pending_review: pending.and_then(id_of),
        head_oid: head_oid.clone(),
        can_bypass_rules: false,
        threads,
        ..Default::default()
    };

    let mut timeline: Vec<Entry> = answers
        .comments
        .iter()
        .map(|c| Entry {
            kind: EntryKind::Comment,
            author: user(c, "/user/login"),
            at: parse_time(at(c, "/created_at")).unwrap_or(0),
            body: absolute_links(at(c, "/body"), web),
            url: at(c, "/html_url").to_owned(),
            review_id: None,
        })
        .collect();
    timeline.extend(
        answers
            .reviews
            .iter()
            .filter_map(|(review, _)| review_entry(review, web)),
    );
    timeline.extend(line_entries(&extras));
    timeline.sort_by_key(|entry| entry.at);

    PrDetails {
        forge: Some(forge),
        number: number("/number"),
        draft,
        title,
        body: absolute_links(at(pr, "/body"), web),
        state: state.to_owned(),
        author: user(pr, "/user/login"),
        url: text("/html_url"),
        base_branch: text("/base/ref"),
        base_oid: text("/base/sha"),
        head_branch: text("/head/ref"),
        head_ref: format!("refs/pull/{}/head", number("/number")),
        head_oid,
        created_at: time("/created_at"),
        updated_at: time("/updated_at"),
        additions: number("/additions"),
        deletions: number("/deletions"),
        changed_files: number("/changed_files"),
        // A draft can't be merged whether or not it has conflicts, and these forges say so the
        // same way: by not calling it mergeable.
        mergeable: match (open, flag(pr, "/mergeable")) {
            (true, Some(true)) => "MERGEABLE",
            (true, Some(false)) if !draft => "CONFLICTING",
            _ => "",
        }
        .to_owned(),
        merge_state: String::new(),
        review_decision: review_decision(&reviews).to_owned(),
        labels: items(pr, "/labels")
            .iter()
            .map(|label| at(label, "/name").to_owned())
            .collect(),
        checks: answers.status.as_ref().map_or(Vec::new(), |status| {
            items(status, "/statuses")
                .iter()
                .map(|status| check(status, web))
                .collect()
        }),
        checks_error: answers.status.as_ref().err().cloned(),
        commits: commits(&answers.commits, &text("/head/sha")),
        timeline,
        inline_error: None,
        extras,
    }
}

/// A numeric id as the page keeps ids.
fn id_of(v: &Value) -> Option<String> {
    v.get("id").and_then(Value::as_u64).map(|id| id.to_string())
}

/// A review, if it says anything: one that only holds comments on lines has no text of its own.
fn review_entry(v: &Value, web: &str) -> Option<Entry> {
    let body = at(v, "/body");
    let dismissed = v.get("dismissed").and_then(Value::as_bool) == Some(true);
    let state = at(v, "/state");
    let verdict = match state {
        "APPROVED" | "REQUEST_CHANGES" if dismissed => {
            crate::i18n::text("backend-had-a-review-dismissed-2ab8b3")
        }
        "APPROVED" => "approved",
        "REQUEST_CHANGES" => crate::i18n::text("backend-requested-changes-7adfe7"),
        "COMMENT" if !body.trim().is_empty() => "reviewed",
        _ => return None,
    };
    let dismissable = !dismissed && matches!(state, "APPROVED" | "REQUEST_CHANGES");
    Some(Entry {
        kind: EntryKind::Review(verdict),
        author: user(v, "/user/login"),
        at: parse_time(at(v, "/submitted_at")).unwrap_or(0),
        body: absolute_links(body, web),
        url: at(v, "/html_url").to_owned(),
        review_id: id_of(v).filter(|_| dismissable),
    })
}

/// What the reviews come to, in GitHub's words: each reviewer's last verdict counts.
fn review_decision(reviews: &[(Value, Vec<Value>)]) -> &'static str {
    let mut last: Vec<(String, i64, &str)> = Vec::new();
    for (review, _) in reviews {
        let state = at(review, "/state");
        // Only official reviews count: by someone who may approve, and not asked to look again.
        if !matches!(state, "APPROVED" | "REQUEST_CHANGES")
            || review.get("dismissed").and_then(Value::as_bool) == Some(true)
            || review.get("official").and_then(Value::as_bool) == Some(false)
        {
            continue;
        }
        let who = at(review, "/user/login").to_owned();
        let when = parse_time(at(review, "/submitted_at")).unwrap_or(0);
        match last.iter_mut().find(|(name, _, _)| *name == who) {
            Some(seen) if seen.1 <= when => *seen = (who, when, state),
            Some(_) => {}
            None => last.push((who, when, state)),
        }
    }
    if last.iter().any(|(_, _, state)| *state == "REQUEST_CHANGES") {
        "CHANGES_REQUESTED"
    } else if !last.is_empty() {
        "APPROVED"
    } else {
        ""
    }
}

/// Whether `review` is `viewer`'s.
fn is_by(review: &Value, viewer: Option<&str>) -> bool {
    viewer.is_some_and(|viewer| at(review, "/user/login").eq_ignore_ascii_case(viewer))
}

/// The comments on lines, one thread per line of a file and side. A thread is current while its
/// first comment's review was made on `head`, the pull request's last commit: its line numbers are
/// that commit's. A comment's own commit doesn't tell, as Forgejo files those on removed and
/// unchanged lines under the merge base; it stands in only when a review names no commit, and then
/// the merge base (`base`) counts too.
fn threads(
    reviews: &[(Value, Vec<Value>)],
    head: &str,
    base: &str,
    logged_in: bool,
) -> Vec<Thread> {
    let mut threads: Vec<Thread> = Vec::new();
    // Which commit each thread's first comment was written on.
    let mut first_commit: Vec<(String, i64)> = Vec::new();
    for (review, comments) in reviews {
        let pending = at(review, "/state") == "PENDING";
        for comment in comments {
            let number = |key: &str| comment.get(key).and_then(Value::as_u64).unwrap_or(0);
            let (side, line) = match (number("position"), number("original_position")) {
                (0, old) if old > 0 => (DiffSide::Left, old),
                (new, _) => (DiffSide::Right, new),
            };
            let path = at(comment, "/path");
            let id = thread_id(side, line, path);
            let at_time = parse_time(at(comment, "/created_at")).unwrap_or(0);
            let entry = ThreadComment {
                id: id_of(comment).unwrap_or_default(),
                author: user(comment, "/user/login"),
                at: at_time,
                body: at(comment, "/body").to_owned(),
                url: at(comment, "/html_url").to_owned(),
                pending,
            };
            let resolved = comment.get("resolver").is_some_and(|r| !r.is_null());
            let commit = match at(review, "/commit_id") {
                "" => match at(comment, "/commit_id") {
                    "" => String::new(),
                    on if on == base => head.to_owned(),
                    on => on.to_owned(),
                },
                on => on.to_owned(),
            };
            match threads.iter().position(|t| t.id == id) {
                Some(i) => {
                    threads[i].comments.push(entry);
                    threads[i].resolved |= resolved;
                    if at_time < first_commit[i].1 {
                        first_commit[i] = (commit, at_time);
                    }
                }
                None => {
                    threads.push(Thread {
                        id,
                        path: path.to_owned(),
                        line: (line > 0).then_some(line),
                        start_line: None,
                        side,
                        outdated: false,
                        resolved,
                        can_resolve: false,
                        can_unresolve: false,
                        can_reply: logged_in,
                        comments: vec![entry],
                        starts_left: false,
                        more_comments: false,
                    });
                    first_commit.push((commit, at_time));
                }
            }
        }
    }
    for (thread, (commit, _)) in threads.iter_mut().zip(&first_commit) {
        thread.outdated = !commit.is_empty() && commit != head;
        thread.comments.sort_by_key(|c| c.at);
    }
    threads
}

/// A thread's id holds where it is, since replying to it means commenting on that line again.
pub(super) fn thread_id(side: DiffSide, line: u64, path: &str) -> String {
    let side = match side {
        DiffSide::Left => 'L',
        DiffSide::Right => 'R',
    };
    format!("{side}{line}:{path}")
}

/// The side, line and file of a thread's id.
pub(super) fn parse_thread_id(id: &str) -> Option<(DiffSide, u64, &str)> {
    let side = match id.chars().next()? {
        'L' => DiffSide::Left,
        'R' => DiffSide::Right,
        _ => return None,
    };
    let (line, path) = id.get(1..)?.split_once(':')?;
    Some((side, line.parse().ok()?, path))
}

fn check(v: &Value, web: &str) -> Check {
    let state = match at(v, "/status") {
        "" => at(v, "/state"),
        status => status,
    };
    Check {
        name: at(v, "/context").to_owned(),
        state: match state {
            "success" => CheckState::Passed,
            "failure" | "error" => CheckState::Failed,
            "pending" => CheckState::Running,
            _ => CheckState::Skipped,
        },
        detail: at(v, "/description").to_owned(),
        url: absolute_url(at(v, "/target_url"), web),
    }
}

/// The commits, oldest first. The API may give them newest first.
fn commits(list: &[Value], head_oid: &str) -> Vec<PrCommit> {
    let mut commits: Vec<PrCommit> = list
        .iter()
        .map(|c| {
            let author = match at(c, "/author/login") {
                "" => at(c, "/commit/author/name"),
                login => login,
            };
            PrCommit {
                oid: at(c, "/sha").to_owned(),
                headline: at(c, "/commit/message")
                    .lines()
                    .next()
                    .unwrap_or("")
                    .to_owned(),
                author: author.to_owned(),
            }
        })
        .collect();
    if commits.len() > 1 && commits.first().is_some_and(|c| c.oid == head_oid) {
        commits.reverse();
    }
    commits
}

/// A link relative to the forge (`/owner/repo/actions/runs/1`) made whole.
fn absolute_url(url: &str, web: &str) -> String {
    match url.strip_prefix('/') {
        Some(path) if !path.starts_with('/') => format!("{web}/{path}"),
        _ => url.to_owned(),
    }
}

/// Text with the forge's own relative links and images (uploads are `/attachments/…`) made
/// whole, so they show and open like any other.
fn absolute_links(text: &str, web: &str) -> String {
    text.replace("](/attachments/", &format!("]({web}/attachments/"))
        .replace("src=\"/attachments/", &format!("src=\"{web}/attachments/"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const WEB: &str = "https://codeberg.org";

    fn answers() -> Answers {
        Answers {
            pr: json!({
                "id": 901, "number": 12, "title": "WIP: Add search",
                "body": "Adds search.\n\n![shot](/attachments/abc)",
                "state": "open", "merged": false, "mergeable": true, "draft": false,
                "user": {"login": "ana"}, "html_url": "https://codeberg.org/o/r/pulls/12",
                "base": {"ref": "main", "sha": "b0"},
                "head": {"ref": "feat/search", "sha": "h1",
                         "repo": {"full_name": "ana/r"}},
                "created_at": "2026-11-02T21:00:00+01:00", "updated_at": "2026-11-02T21:00:00Z",
                "additions": 10, "deletions": 2, "changed_files": 3,
                "labels": [{"name": "feature"}]
            }),
            repo: json!({
                "allow_merge_commits": true, "allow_squash_merge": true, "allow_rebase": false,
                "permissions": {"admin": false, "push": true, "pull": true}
            }),
            viewer: Some("bo".to_owned()),
            comments: vec![json!({
                "id": 5, "user": {"login": "bo"}, "body": "Nice",
                "created_at": "2026-11-02T20:30:00Z", "html_url": "https://codeberg.org/o/r/pulls/12#issuecomment-5"
            })],
            reviews: vec![
                (
                    json!({"id": 1, "state": "REQUEST_CHANGES", "body": "Please fix",
                           "user": {"login": "cy"}, "submitted_at": "2026-11-02T20:10:00Z",
                           "comments_count": 2}),
                    vec![
                        json!({"id": 11, "path": "src/a.rs", "position": 9, "original_position": 0,
                               "commit_id": "h1", "body": "Here", "user": {"login": "cy"},
                               "created_at": "2026-11-02T20:10:00Z", "resolver": null}),
                        json!({"id": 12, "path": "src/b.rs", "position": 0, "original_position": 3,
                               "commit_id": "h0", "body": "Old", "user": {"login": "cy"},
                               "created_at": "2026-11-02T20:10:00Z", "resolver": {"login": "ana"}}),
                    ],
                ),
                (
                    json!({"id": 2, "state": "APPROVED", "body": "", "user": {"login": "cy"},
                           "submitted_at": "2026-11-02T20:40:00Z", "comments_count": 0}),
                    vec![],
                ),
                (
                    json!({"id": 3, "state": "PENDING", "body": "", "user": {"login": "bo"},
                           "comments_count": 1}),
                    vec![json!({"id": 13, "path": "src/a.rs", "position": 9,
                                "commit_id": "h1", "body": "Mine", "user": {"login": "bo"},
                                "created_at": "2026-11-02T20:50:00Z"})],
                ),
                (
                    json!({"id": 4, "state": "APPROVED", "dismissed": true, "body": "",
                           "user": {"login": "di"}, "submitted_at": "2026-11-02T20:20:00Z"}),
                    vec![],
                ),
            ],
            commits: vec![
                json!({"sha": "h1", "commit": {"message": "feat: more\n\nbody", "author": {"name": "Ana"}},
                       "author": {"login": "ana"}}),
                json!({"sha": "h0", "commit": {"message": "feat: search", "author": {"name": "Ana B"}},
                       "author": null}),
            ],
            status: Ok(json!({"statuses": [
                {"context": "ci / test", "status": "success", "description": "Passed",
                 "target_url": "/o/r/actions/runs/1/jobs/0"},
                {"context": "ci / lint", "status": "failure", "target_url": "https://ci/2"},
                {"context": "ci / e2e", "status": "pending"},
                {"context": "ci / docs", "status": "warning"}
            ]})),
        }
    }

    fn parsed() -> PrDetails {
        assemble(&answers(), Forge::Forgejo, WEB)
    }

    #[test]
    fn reads_the_pull_request_itself() {
        let pr = parsed();
        assert_eq!(pr.forge, Some(Forge::Forgejo));
        assert_eq!((pr.number, pr.state.as_str()), (12, "OPEN"));
        assert!(pr.draft, "a WIP title is a draft");
        assert_eq!(pr.author, "ana");
        assert_eq!(
            (pr.base_branch.as_str(), pr.base_oid.as_str()),
            ("main", "b0")
        );
        assert_eq!(
            (pr.head_branch.as_str(), pr.head_oid.as_str()),
            ("feat/search", "h1")
        );
        assert_eq!(pr.head_ref, "refs/pull/12/head");
        assert_eq!(pr.created_at, pr.updated_at - 3_600);
        assert_eq!((pr.additions, pr.deletions, pr.changed_files), (10, 2, 3));
        assert_eq!(pr.mergeable, "MERGEABLE");
        assert_eq!(pr.labels, ["feature"]);
    }

    #[test]
    fn uploads_show_from_the_forge() {
        assert!(
            parsed()
                .body
                .contains("![shot](https://codeberg.org/attachments/abc)")
        );
    }

    #[test]
    fn merged_and_conflicting_say_so() {
        let mut a = answers();
        a.pr["mergeable"] = json!(false);
        // A draft isn't mergeable either way: that says nothing of conflicts.
        assert_eq!(assemble(&a, Forge::Forgejo, WEB).mergeable, "");
        a.pr["title"] = json!("Add search");
        assert_eq!(assemble(&a, Forge::Forgejo, WEB).mergeable, "CONFLICTING");
        a.pr["state"] = json!("closed");
        a.pr["merged"] = json!(true);
        let pr = assemble(&a, Forge::Forgejo, WEB);
        assert_eq!((pr.state.as_str(), pr.mergeable.as_str()), ("MERGED", ""));
    }

    #[test]
    fn reads_the_statuses_as_checks() {
        let pr = parsed();
        let states: Vec<CheckState> = pr.checks.iter().map(|c| c.state).collect();
        assert_eq!(
            states,
            [
                CheckState::Passed,
                CheckState::Failed,
                CheckState::Running,
                CheckState::Skipped
            ]
        );
        assert_eq!(
            pr.checks[0].url,
            "https://codeberg.org/o/r/actions/runs/1/jobs/0"
        );
        assert_eq!(pr.checks[0].detail, "Passed");
    }

    #[test]
    fn statuses_that_couldnt_be_read_are_no_empty_list() {
        let mut a = answers();
        a.status = Err("Forgejo: HTTP 500".to_owned());
        let pr = assemble(&a, Forge::Forgejo, WEB);
        assert!(pr.checks.is_empty());
        assert_eq!(pr.checks_error.as_deref(), Some("Forgejo: HTTP 500"));
        assert_eq!(parsed().checks_error, None);
    }

    #[test]
    fn commits_come_oldest_first() {
        let pr = parsed();
        let commits: Vec<(&str, &str, &str)> = pr
            .commits
            .iter()
            .map(|c| (c.oid.as_str(), c.headline.as_str(), c.author.as_str()))
            .collect();
        assert_eq!(
            commits,
            [("h0", "feat: search", "Ana B"), ("h1", "feat: more", "ana")]
        );
    }

    #[test]
    fn each_reviewers_last_verdict_counts() {
        // cy asked for changes, then approved; di's approval was dismissed.
        assert_eq!(parsed().review_decision, "APPROVED");
        let mut a = answers();
        a.reviews.swap(0, 1);
        a.reviews[1].0["submitted_at"] = json!("2026-11-02T21:00:00Z");
        assert_eq!(review_decision(&a.reviews), "CHANGES_REQUESTED");
        assert_eq!(review_decision(&[]), "");
    }

    #[test]
    fn the_timeline_has_comments_reviews_and_comments_on_lines() {
        let pr = parsed();
        let said: Vec<(&str, &EntryKind)> = pr
            .timeline
            .iter()
            .filter(|e| !matches!(e.kind, EntryKind::Inline { .. }))
            .map(|e| (e.author.as_str(), &e.kind))
            .collect();
        assert_eq!(
            said,
            [
                ("cy", &EntryKind::Review("requested changes")),
                ("di", &EntryKind::Review("had a review dismissed")),
                ("bo", &EntryKind::Comment),
                ("cy", &EntryKind::Review("approved")),
            ]
        );
        let inline = pr
            .timeline
            .iter()
            .filter(|e| matches!(e.kind, EntryKind::Inline { .. }))
            .count();
        assert_eq!(inline, 2, "the pending comment isn't shown to the others");
        let approval = pr
            .timeline
            .iter()
            .find(|e| e.kind == EntryKind::Review("approved"));
        assert_eq!(approval.and_then(|e| e.review_id.as_deref()), Some("2"));
        let dismissed = pr
            .timeline
            .iter()
            .find(|e| e.kind == EntryKind::Review("had a review dismissed"));
        assert_eq!(dismissed.and_then(|e| e.review_id.as_deref()), None);
    }

    #[test]
    fn reads_who_may_do_what() {
        let extras = parsed().extras;
        assert_eq!(extras.pr_id, "901");
        assert_eq!(extras.viewer.as_deref(), Some("bo"));
        assert!(extras.can_write());
        assert!(extras.merge_allowed && extras.squash_allowed && !extras.rebase_allowed);
        assert!(!extras.delete_branch_on_merge);
        assert_eq!(extras.head_repo.as_deref(), Some("ana/r"));
        assert_eq!(extras.pending_review.as_deref(), Some("3"));
        assert_eq!(extras.head_oid, "h1");
    }

    #[test]
    fn without_the_repository_nothing_can_be_done_from_the_page() {
        let mut a = answers();
        a.repo = Value::Null;
        let extras = assemble(&a, Forge::Forgejo, WEB).extras;
        assert!(extras.pr_id.is_empty());
        assert!(!extras.can_write());
    }

    #[test]
    fn comments_on_the_same_line_make_one_thread() {
        let threads = parsed().extras.threads;
        assert_eq!(threads.len(), 2);
        let on_a = &threads[0];
        assert_eq!(
            (on_a.path.as_str(), on_a.line, on_a.side),
            ("src/a.rs", Some(9), DiffSide::Right)
        );
        let said: Vec<(&str, bool)> = on_a
            .comments
            .iter()
            .map(|c| (c.body.as_str(), c.pending))
            .collect();
        assert_eq!(said, [("Here", false), ("Mine", true)]);
        assert!(on_a.can_reply && !on_a.can_resolve && !on_a.outdated && !on_a.resolved);
        let extras = parsed().extras;
        let pending: Vec<&str> = extras
            .pending_comments()
            .map(|(_, c)| c.id.as_str())
            .collect();
        assert_eq!(pending, ["13"]);
    }

    #[test]
    fn a_thread_from_an_older_commit_is_outdated() {
        let threads = parsed().extras.threads;
        let on_b = &threads[1];
        assert_eq!((on_b.side, on_b.line), (DiffSide::Left, Some(3)));
        assert!(on_b.outdated && on_b.resolved);
    }

    #[test]
    fn someone_elses_draft_is_not_the_viewers() {
        let mut a = answers();
        // An admin, shown bo's pending review.
        a.viewer = Some("admin".to_owned());
        let pr = assemble(&a, Forge::Forgejo, WEB);
        assert_eq!(pr.extras.pending_review, None);
        assert!(
            pr.extras
                .threads
                .iter()
                .flat_map(|t| &t.comments)
                .all(|c| c.body != "Mine")
        );
        // Whose it is goes by name, whatever the case.
        a.viewer = Some("BO".to_owned());
        let pr = assemble(&a, Forge::Forgejo, WEB);
        assert_eq!(pr.extras.pending_review.as_deref(), Some("3"));
    }

    #[test]
    fn only_official_reviews_decide() {
        let mut a = answers();
        a.reviews[1].0["official"] = json!(false);
        // cy's approval doesn't count, so their request for changes stands.
        assert_eq!(review_decision(&a.reviews), "CHANGES_REQUESTED");
    }

    #[test]
    fn a_thread_is_as_current_as_the_review_it_came_with() {
        let mut a = answers();
        // Forgejo files the comment under the merge base; the review says which head it was on.
        a.reviews[0].0["commit_id"] = json!("h1");
        a.reviews[0].1[1]["commit_id"] = json!("b0");
        let threads = assemble(&a, Forge::Forgejo, WEB).extras.threads;
        assert!(threads.iter().all(|t| !t.outdated), "{threads:?}");
        // Pushed to since: the line numbers are another commit's.
        a.reviews[0].0["commit_id"] = json!("h0");
        let threads = assemble(&a, Forge::Forgejo, WEB).extras.threads;
        assert!(threads[0].outdated && threads[1].outdated, "{threads:?}");
        // No commit on the review: the comment's own, the merge base standing for the head.
        a.reviews[0].0["commit_id"] = json!("");
        a.pr["merge_base"] = json!("b0");
        let threads = assemble(&a, Forge::Forgejo, WEB).extras.threads;
        assert!(!threads[0].outdated && !threads[1].outdated, "{threads:?}");
    }

    #[test]
    fn nobody_logged_in_can_reply() {
        let mut a = answers();
        a.viewer = None;
        let pr = assemble(&a, Forge::Forgejo, WEB);
        assert!(pr.extras.threads.iter().all(|t| !t.can_reply));
    }

    #[test]
    fn a_threads_id_says_where_it_is() {
        let id = thread_id(DiffSide::Left, 12, "src/a b:c.rs");
        assert_eq!(id, "L12:src/a b:c.rs");
        assert_eq!(
            parse_thread_id(&id),
            Some((DiffSide::Left, 12, "src/a b:c.rs"))
        );
        assert_eq!(parse_thread_id("R3:x"), Some((DiffSide::Right, 3, "x")));
        assert_eq!(parse_thread_id("PRRT_1"), None);
        assert_eq!(parse_thread_id("Rx:y"), None);
    }

    #[test]
    fn relative_links_are_made_whole() {
        assert_eq!(
            absolute_url("/o/r/actions", WEB),
            "https://codeberg.org/o/r/actions"
        );
        assert_eq!(absolute_url("https://ci/1", WEB), "https://ci/1");
        assert_eq!(absolute_url("//evil.example/x", WEB), "//evil.example/x");
        assert_eq!(absolute_url("", WEB), "");
    }
}
