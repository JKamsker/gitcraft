//! Reading each forge's lists, and which pull requests need you.

use super::*;

const T0: i64 = 1_793_000_000;

fn pr(mine: bool, requested: bool) -> InboxPr {
    InboxPr {
        number: 1,
        url: "https://f/o/r/pull/1".into(),
        mine,
        review_requested: requested,
        ..InboxPr::default()
    }
}

#[test]
fn a_review_asked_of_you_is_in_the_inbox() {
    assert_eq!(
        needs_you(&pr(false, true), None),
        vec![Reason::ReviewRequested]
    );
    // Not one of yours that asks you to review it (a team you're in).
    assert!(needs_you(&pr(true, true), None).is_empty());
    assert!(needs_you(&pr(false, false), None).is_empty());
}

#[test]
fn yours_only_when_checks_fail_or_others_said_something_new() {
    let mut mine = pr(true, false);
    assert!(needs_you(&mine, None).is_empty(), "fine and quiet");
    mine.checks = Checks::Pending;
    assert!(needs_you(&mine, None).is_empty());
    mine.checks = Checks::Failing;
    assert_eq!(needs_you(&mine, None), vec![Reason::ChecksFailing]);
    mine.checks = Checks::Passing;
    mine.activity = Activity::At(vec![T0, T0 + 100]);
    assert_eq!(needs_you(&mine, None), vec![Reason::NewComments(2)]);
    let seen = Seen {
        at: T0 + 50,
        count: 0,
    };
    assert_eq!(needs_you(&mine, Some(seen)), vec![Reason::NewComments(1)]);
    let later = Seen {
        at: T0 + 100,
        count: 0,
    };
    assert!(
        needs_you(&mine, Some(later)).is_empty(),
        "a comment at that very second was seen"
    );
}

#[test]
fn a_forge_that_only_counts_comments_compares_counts() {
    let mut mine = pr(true, false);
    mine.activity = Activity::Count(5);
    assert_eq!(needs_you(&mine, None), vec![Reason::NewComments(5)]);
    let seen = seen_now(&mine, T0);
    assert_eq!(seen, Seen { at: T0, count: 5 });
    assert!(needs_you(&mine, Some(seen)).is_empty());
    // A comment deleted since: never fewer than none.
    mine.activity = Activity::Count(3);
    assert!(needs_you(&mine, Some(seen)).is_empty());
    mine.activity = Activity::Count(7);
    assert_eq!(needs_you(&mine, Some(seen)), vec![Reason::NewComments(2)]);
}

#[test]
fn reads_github_lists() -> Result<(), String> {
    let requested = r#"[{"number": 12, "title": "Fix ünïcode ✓", "url": "https://github.com/o/r/pull/12",
        "author": {"login": "ana"}, "isDraft": false, "updatedAt": "2026-11-02T20:00:00Z"}]"#;
    let list = parse_github(requested, true)?;
    assert_eq!(list.len(), 1);
    assert!(list[0].review_requested && !list[0].mine);
    assert_eq!(list[0].title, "Fix ünïcode ✓");
    assert_eq!(list[0].checks, Checks::Unknown);

    let mine = r#"[{"number": 3, "title": "Mine", "url": "https://github.com/o/r/pull/3",
        "author": {"login": "me"}, "isDraft": true, "updatedAt": "2026-11-02T21:00:00Z",
        "statusCheckRollup": [
            {"__typename": "CheckRun", "status": "COMPLETED", "conclusion": "SUCCESS"},
            {"__typename": "CheckRun", "status": "COMPLETED", "conclusion": "FAILURE"}],
        "comments": [
            {"author": {"login": "me"}, "createdAt": "2026-11-02T20:10:00Z"},
            {"author": {"login": "bo"}, "createdAt": "2026-11-02T20:20:00Z"}],
        "reviews": [{"author": {"login": "cy"}, "submittedAt": "2026-11-02T20:30:00Z", "state": "COMMENTED"}]
    }]"#;
    let list = parse_github(mine, false)?;
    assert!(list[0].mine && list[0].draft);
    assert_eq!(list[0].checks, Checks::Failing);
    assert_eq!(
        list[0].activity,
        Activity::At(vec![
            parse_time("2026-11-02T20:20:00Z").unwrap_or(0),
            parse_time("2026-11-02T20:30:00Z").unwrap_or(0),
        ])
    );
    assert!(parse_github("not json", false).is_err());
    assert_eq!(parse_github("[]", false), Ok(Vec::new()));
    Ok(())
}

#[test]
fn githubs_checks_together() {
    let rollup = |json: &str| github_checks(serde_json::from_str::<Value>(json).ok().as_ref());
    assert_eq!(rollup("[]"), Checks::Unknown);
    assert_eq!(rollup("null"), Checks::Unknown);
    assert_eq!(
        rollup(r#"[{"status": "COMPLETED", "conclusion": "SUCCESS"}, {"state": "SUCCESS"}]"#),
        Checks::Passing
    );
    assert_eq!(
        rollup(r#"[{"status": "IN_PROGRESS", "conclusion": ""}, {"state": "SUCCESS"}]"#),
        Checks::Pending
    );
    assert_eq!(
        rollup(r#"[{"status": "IN_PROGRESS"}, {"state": "ERROR"}]"#),
        Checks::Failing
    );
    assert_eq!(rollup(r#"[{"state": "PENDING"}]"#), Checks::Pending);
    assert_eq!(
        rollup(r#"[{"status": "COMPLETED", "conclusion": "TIMED_OUT"}]"#),
        Checks::Failing
    );
}

#[test]
fn reads_gitlab_lists() -> Result<(), String> {
    let json = r#"[{"iid": 4, "title": "MR", "web_url": "https://gitlab.com/g/p/-/merge_requests/4",
        "author": {"username": "me"}, "draft": false, "updated_at": "2026-11-02T20:00:00.000Z",
        "user_notes_count": 3, "head_pipeline": {"status": "failed"}}]"#;
    let list = parse_gitlab(json, false)?;
    assert_eq!(list[0].number, 4);
    assert_eq!(list[0].activity, Activity::Count(3));
    assert_eq!(list[0].checks, Checks::Failing);
    let quiet = r#"[{"iid": 5, "title": "x", "web_url": "u", "author": {"username": "a"}}]"#;
    let list = parse_gitlab(quiet, true)?;
    assert_eq!(list[0].checks, Checks::Unknown);
    assert_eq!(list[0].activity, Activity::Count(0));
    assert!(parse_gitlab("{", true).is_err());
    Ok(())
}

#[test]
fn a_pull_request_in_both_lists_is_yours_once() {
    let url = "https://f/o/r/pull/1";
    let asked = InboxPr {
        url: url.into(),
        number: 1,
        review_requested: true,
        ..InboxPr::default()
    };
    let mine = InboxPr {
        url: url.into(),
        number: 1,
        mine: true,
        checks: Checks::Failing,
        ..InboxPr::default()
    };
    let other = InboxPr {
        url: "https://f/o/r/pull/2".into(),
        number: 2,
        review_requested: true,
        ..InboxPr::default()
    };
    let all = merged(vec![vec![asked, other], vec![mine]]);
    assert_eq!(all.len(), 2);
    assert!(all[0].mine && all[0].review_requested);
    assert_eq!(all[0].checks, Checks::Failing);
    assert_eq!(Reason::NewComments(1).label(), "1 new comment");
    assert_eq!(Reason::NewComments(4).label(), "4 new comments");
    assert_eq!(status_checks("error"), Checks::Failing);
    assert_eq!(status_checks(""), Checks::Unknown);
}
