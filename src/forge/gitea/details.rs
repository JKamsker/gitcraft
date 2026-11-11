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
mod tests;
