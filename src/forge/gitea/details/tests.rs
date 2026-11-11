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
