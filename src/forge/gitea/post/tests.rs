use super::*;

#[test]
fn review13_gitea_targets_reject_another_web_instance() {
    for forge in [crate::forge::Forge::Gitea, crate::forge::Forge::Forgejo] {
        for prefix in ["", "/reviews"] {
            let h = Hosting {
                forge,
                host: "forge.example".into(),
                path: "Team/Repo".into(),
                web: format!("https://forge.example:4000{prefix}"),
            };
            for (base, path, expected) in [
                ("https://forge.example:3000", "Team/Repo", false),
                ("https://forge.example:4000", "Team/Repo", prefix.is_empty()),
                (
                    "https://forge.example:4000/reviews",
                    "team/repo",
                    !prefix.is_empty(),
                ),
                ("https://forge.example:4000/REVIEWS", "Team/Repo", false),
            ] {
                let details = PrDetails {
                    number: 7,
                    url: format!("{base}/{path}/pulls/7"),
                    ..Default::default()
                };
                let api = Api::new("Gitea", &h.web, &h.host, None);
                let target = GiteaTarget::of(api, "Gitea", &h, &details);
                assert_eq!(target.is_some(), expected, "{h:?} {}", details.url);
                if let Some(target) = target {
                    assert!(
                        target.path.eq_ignore_ascii_case(&h.path),
                        "proxy prefix must stay in API base only"
                    );
                }
            }
        }
    }
}

#[test]
fn provider_boundary_stale_details_cannot_retarget_gitea_or_forgejo() {
    for forge in [crate::forge::Forge::Gitea, crate::forge::Forge::Forgejo] {
        let h = Hosting {
            forge,
            host: "forge.test".into(),
            path: "fork/project".into(),
            web: "https://forge.test".into(),
        };
        for (url, expected) in [
            ("https://forge.test/fork/project/pulls/7", true),
            ("https://forge.test/upstream/project/pulls/7", false),
            ("https://other.test/fork/project/pulls/7", false),
        ] {
            let details = PrDetails {
                number: 7,
                url: url.into(),
                ..Default::default()
            };
            let api = Api::new("Gitea", &h.web, &h.host, None);
            assert_eq!(
                GiteaTarget::of(api, "Gitea", &h, &details).is_some(),
                expected
            );
        }
    }
}

/// Only "still testing it" is waited out; any other refusal of a merge is said at once.
#[test]
fn a_merge_is_tried_again_only_while_the_server_tests_it() {
    let failure = |status, message: &str| Failure {
        status,
        message: message.to_owned(),
    };
    assert!(still_checking(&failure(
        Some(405),
        "Gitea: Please try again later (HTTP 405)"
    )));
    assert!(!still_checking(&failure(Some(405), "Not allowed to merge")));
    assert!(!still_checking(&failure(
        Some(409),
        "Please try again later"
    )));
    assert!(!still_checking(&failure(None, "try again later")));
}

fn line(path: &str, line: u64, side: DiffSide, body: &str) -> LineComment {
    LineComment {
        path: path.to_owned(),
        line,
        side,
        body: body.to_owned(),
    }
}

#[test]
fn a_review_names_its_verdict_and_commit() {
    for (verdict, name) in [
        (Verdict::Comment, "COMMENT"),
        (Verdict::Approve, "APPROVED"),
        (Verdict::RequestChanges, "REQUEST_CHANGES"),
    ] {
        let payload = review_payload("abc", event(verdict), "Text", &[]);
        assert_eq!(payload["event"], name);
        assert_eq!(payload["commit_id"], "abc");
        assert_eq!(payload["body"], "Text");
        assert!(payload.get("comments").is_none());
    }
}

#[test]
fn comments_on_lines_go_on_their_side() {
    let comments = [
        line("src/a.rs", 12, DiffSide::Right, "New line"),
        line("src/b.rs", 3, DiffSide::Left, "Removed line"),
    ];
    let payload = review_payload("abc", "PENDING", "", &comments);
    assert_eq!(
        payload["comments"],
        json!([
            {"path": "src/a.rs", "new_position": 12, "body": "New line"},
            {"path": "src/b.rs", "old_position": 3, "body": "Removed line"},
        ])
    );
}

#[test]
fn a_merge_names_its_method_and_commit() {
    for (method, name) in [
        (MergeMethod::Merge, "merge"),
        (MergeMethod::Squash, "squash"),
        (MergeMethod::Rebase, "rebase"),
    ] {
        let payload = merge_payload("abc", method);
        assert_eq!(payload["Do"], name);
        assert_eq!(payload["head_commit_id"], "abc");
        assert_eq!(payload["delete_branch_after_merge"], false);
    }
}

#[test]
fn knows_a_refusal_for_want_of_words() {
    assert!(needs_words(
        "Forgejo: review event PENDING requires a body (HTTP 422)"
    ));
    assert!(needs_words(
        "Forgejo: review event COMMENT requires a body or a comment (HTTP 422)"
    ));
    assert!(!needs_words("Forgejo: not found (HTTP 404)"));
}

#[test]
fn reads_which_commit_a_branch_is_on() {
    let branch = json!({"name": "b", "commit": {"id": "abc123"}});
    assert_eq!(branch_tip(&branch), Some("abc123"));
    assert_eq!(branch_tip(&json!({"message": "not found"})), None);
}

fn target(pending: Option<&str>) -> GiteaTarget {
    GiteaTarget {
        api: Api::new("Forgejo", "https://127.0.0.1:9", "127.0.0.1", None),
        name: "Forgejo",
        path: "o/r".to_owned(),
        number: 1,
        head_oid: "h".to_owned(),
        head_branch: "b".to_owned(),
        head_repo: None,
        pr_id: String::new(),
        pending_review: pending.map(str::to_owned),
    }
}

#[test]
fn nothing_goes_out_without_knowing_the_pull_request() {
    let error = target(None).add_pending(&line("a", 1, DiffSide::Right, "x"));
    assert!(error.is_err_and(|e| e.contains("Load it again")));
}

#[test]
fn nothing_folds_a_pending_review_in_unknowingly() {
    let t = target(None);
    assert!(t.comment_now(&line("a", 1, DiffSide::Right, "x")).is_err());
    assert!(t.review(Verdict::Comment, "x").is_err());
    assert!(t.reply("R1:a", "x").is_err());
}

#[test]
fn conversations_cant_be_resolved_through_the_api() {
    assert!(target(None).set_resolved("R1:a", true).is_err());
}

#[test]
fn a_thread_from_elsewhere_cant_be_answered() {
    let error = target(None).reply("PRRT_1", "x");
    assert!(error.is_err_and(|e| e.contains("can't be answered")));
}

#[test]
fn nothing_is_pending_to_discard() {
    assert!(target(None).discard_pending().is_err());
    assert!(target(None).remove_pending("1", false).is_err());
}
