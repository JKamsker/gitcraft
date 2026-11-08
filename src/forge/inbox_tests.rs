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
    mine.said = vec![T0, T0 + 100];
    assert_eq!(needs_you(&mine, None), vec![Reason::NewComments(2)]);
    let seen = Seen { at: T0 + 50 };
    assert_eq!(needs_you(&mine, Some(seen)), vec![Reason::NewComments(1)]);
    let later = Seen { at: T0 + 100 };
    assert!(
        needs_you(&mine, Some(later)).is_empty(),
        "a comment at that very second was seen"
    );
}

/// Seen goes by the forge's clock (the newest thing said), never back.
#[test]
fn what_is_seen_is_up_to_the_newest_comment() {
    let mut mine = pr(true, false);
    mine.said = vec![T0 + 100, T0];
    assert_eq!(seen_now(&mine, None), Seen { at: T0 + 100 });
    assert!(needs_you(&mine, Some(seen_now(&mine, None))).is_empty());
    // A comment deleted since doesn't make what was seen earlier.
    mine.said = vec![T0];
    assert_eq!(seen_now(&mine, Some(Seen { at: T0 + 100 })).at, T0 + 100);
    // Nothing said yet: whatever comes is new.
    mine.said.clear();
    let first = seen_now(&mine, None);
    mine.said = vec![T0];
    assert_eq!(needs_you(&mine, Some(first)), vec![Reason::NewComments(1)]);
    // A list that didn't read what was said (one of many of yours) still says when it last
    // changed: what came before is in that.
    let unread = InboxPr {
        updated: T0 + 300,
        ..pr(true, false)
    };
    mine.said = vec![T0 + 200, T0 + 400];
    assert_eq!(
        needs_you(&mine, Some(seen_now(&unread, None))),
        vec![Reason::NewComments(1)]
    );
}

#[test]
fn some_errors_arent_worth_asking_again_soon() {
    assert!(gives_up(
        "gh isn't installed: gitcraft uses it for pull requests"
    ));
    assert!(gives_up(
        "glab isn't signed in to git.example.org: `glab auth login --hostname git.example.org`"
    ));
    assert!(gives_up(
        "To get started with GitHub CLI, please run:  gh auth login"
    ));
    let no_login = "no Forgejo login known for https://codeberg.org: add one with `tea login add`";
    assert!(gives_up(no_login) && is_no_login(no_login));
    assert!(!gives_up("HTTP 502: Bad Gateway"));
    assert!(!is_no_login("gh isn't installed"));
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
        list[0].said,
        vec![
            parse_time("2026-11-02T20:20:00Z").unwrap_or(0),
            parse_time("2026-11-02T20:30:00Z").unwrap_or(0),
        ]
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

/// `glab mr list` says nothing of pipelines (only one merge request read alone does), nor
/// whose its notes are.
#[test]
fn reads_gitlab_lists_and_each_of_yours() -> Result<(), String> {
    let json = r#"[{"iid": 4, "title": "MR", "web_url": "https://gitlab.com/g/p/-/merge_requests/4",
        "author": {"username": "me"}, "draft": false, "updated_at": "2026-11-02T20:00:00.000Z",
        "user_notes_count": 3}]"#;
    let list = parse_gitlab(json, false)?;
    assert_eq!(list[0].number, 4);
    assert!(list[0].mine && list[0].said.is_empty());
    assert_eq!(list[0].checks, Checks::Unknown);
    let quiet = r#"[{"iid": 5, "title": "x", "web_url": "u", "author": {"username": "a"}}]"#;
    let list = parse_gitlab(quiet, true)?;
    assert!(list[0].review_requested);
    assert!(parse_gitlab("{", true).is_err());

    let one = r#"{"iid": 4, "head_pipeline": {"id": 7, "status": "failed"}}"#;
    assert_eq!(gitlab_pipeline(one), Checks::Failing);
    assert_eq!(
        gitlab_pipeline(r#"{"head_pipeline": {"status": "running"}}"#),
        Checks::Pending
    );
    assert_eq!(
        gitlab_pipeline(r#"{"head_pipeline": null}"#),
        Checks::Unknown
    );
    assert_eq!(gitlab_pipeline("oops"), Checks::Unknown);

    let notes = r#"[
        {"body": "Why?", "author": {"username": "bo"}, "system": false, "created_at": "2026-11-02T20:30:00.000Z"},
        {"body": "Because.", "author": {"username": "me"}, "system": false, "created_at": "2026-11-02T20:20:00.000Z"},
        {"body": "added 1 commit", "author": {"username": "bo"}, "system": true, "created_at": "2026-11-02T20:10:00.000Z"}
    ]"#;
    assert_eq!(
        gitlab_notes(notes, "me"),
        vec![parse_time("2026-11-02T20:30:00.000Z").unwrap_or(0)]
    );
    assert!(gitlab_notes("{", "me").is_empty());
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

#[test]
fn a_full_list_says_it_was_cut() {
    assert_eq!(cut_note(49, 50, "of yours"), None);
    assert_eq!(
        cut_note(50, 50, "of yours").as_deref(),
        Some("only the 50 latest of yours were read")
    );
}

#[test]
fn backend_fluent_inbox_presentation_preserves_numeric_counts_and_login_identity() {
    let protocol = "no Forgejo login known for forge.example: add one with `tea login add`, or open one of its repositories with a login, for gitcraft to know which pull requests are yours";
    for (locale, one, many, grouped, login) in [
        (
            "en-US",
            "1 new comment",
            "2 new comments",
            "1,234 new comments",
            "No Forgejo login is known for forge.example",
        ),
        (
            "de",
            "1 neuer Kommentar",
            "2 neue Kommentare",
            "1.234 neue Kommentare",
            "Für forge.example ist keine Forgejo-Anmeldung bekannt",
        ),
    ] {
        crate::i18n::set_language(locale);
        assert_eq!(Reason::NewComments(1).label(), one);
        assert_eq!(Reason::NewComments(2).label(), many);
        assert_eq!(Reason::NewComments(1234).label(), grouped);
        assert!(Reason::NewComments(0).label().starts_with('0'));
        assert!(is_no_login(protocol));
        assert!(gives_up(protocol));
        assert!(display_error(protocol).starts_with(login));
        assert_eq!(
            display_error("external provider detail: HTTP 409 /refs/main"),
            "external provider detail: HTTP 409 /refs/main"
        );
        let json = r#"[{"iid":7,"title":"User title","source_branch":"de/untranslated","target_branch":"main","web_url":"https://git.example/mr/7","author":{"username":"user"}}]"#;
        let prs = gitlab_list(json, false).unwrap();
        assert_eq!(prs[0].title, "User title");
        assert_eq!(prs[0].url, "https://git.example/mr/7");
    }
    crate::i18n::set_language("en-US");
}
