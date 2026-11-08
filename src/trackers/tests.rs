use super::*;
#[test]
fn jira_cursor_account_adf_and_custom_criteria_are_preserved() {
    let a = Account {
        origin: "https://company.atlassian.net".into(),
        user: "me@example.org".into(),
        secret_ref: "profile-2".into(),
        acceptance_field: "customfield_10001".into(),
    };
    let mut calls = vec![];
    let mut c = Client {
        kind: Kind::Jira,
        account: a.clone(),
        send: |method: &str, path: &str, body: Option<&Value>| {
            calls.push((method.to_owned(), path.to_owned(), body.cloned()));
            if path.ends_with("myself") {
                return Ok(json!({"accountId":"viewer-1","emailAddress":"me@example.org"}));
            }
            let second = body.is_some_and(|b| b.get("nextPageToken") == Some(&json!("next")));
            if second {
                Ok(json!({"issues":[],"isLast":true}))
            } else {
                Ok(
                    json!({"issues":[{"id":"908","key":"APP-42","fields":{"summary":"Fix login","assignee":{"accountId":"viewer-1"},"labels":["bug"],"description":{"type":"doc","content":[{"type":"paragraph","content":[{"type":"text","text":"Details"}]}]},"customfield_10001":{"type":"doc","content":[{"type":"taskList","content":[{"type":"taskItem","attrs":{"state":"TODO"},"content":[{"type":"text","text":"no crash"}]}]}]}}}],"nextPageToken":"next","isLast":false}),
                )
            }
        },
    };
    let list = c.listed().unwrap();
    drop(c);
    assert_eq!(calls.len(), 3);
    assert!(calls[1].1.ends_with("search/jql"));
    assert_eq!(calls[2].2.as_ref().unwrap()["nextPageToken"], "next");
    let issue = &list[0];
    assert_eq!(issue.key(), "APP-42");
    assert_eq!(issue.branch_name(), "app-42-fix-login");
    assert_eq!(issue.acceptance.as_deref(), Some("- [ ] no crash"));
    assert_eq!(issue.url, "https://company.atlassian.net/browse/APP-42");
    let link = crate::repo::issue_work::link::IssueLink::of(issue);
    assert_eq!(
        link.pull_request_text().1,
        "Relates to APP-42: https://company.atlassian.net/browse/APP-42"
    );
    assert!(!serde_json::to_string(&a).unwrap().contains("token"));
}
#[test]
fn linear_stable_ids_pagination_and_graphql_errors() {
    let a = Account {
        origin: "https://api.linear.app".into(),
        user: "me@example.org".into(),
        ..Default::default()
    };
    let mut n = 0;
    let mut c = Client {
        kind: Kind::Linear,
        account: a,
        send: |_: &str, path: &str, b: Option<&Value>| {
            assert_eq!(path, "/graphql");
            n += 1;
            let q = b.unwrap()["query"].as_str().unwrap();
            if q.contains("viewer { id email }") {
                return Ok(json!({"data":{"viewer":{"id":"uuid-v","email":"me@example.org"}}}));
            }
            assert!(q.contains("viewer { assignedIssues"));
            let page = n == 2;
            Ok(
                json!({"data":{"viewer":{"assignedIssues":{"nodes":if page{vec![json!({"id":"uuid-i","identifier":"ENG-9","title":"Keyboard","url":"https://linear.app/team/issue/ENG-9/keyboard","description":"## Acceptance criteria\n- [ ] works","assignee":{"id":"uuid-v"},"labels":{"nodes":[]}})]}else{vec![]},"pageInfo":{"hasNextPage":page,"endCursor":"cursor-2"}}}}}),
            )
        },
    };
    let list = c.listed().unwrap();
    assert_eq!(list[0].tracker.as_ref().unwrap().id, "uuid-i");
    assert_eq!(list[0].key(), "ENG-9");
    assert!(
        graph(json!({"data":{"issues":[]},"errors":[{"message":"secret"}]}))
            .unwrap_err()
            .contains("no partial")
    );
}
#[test]
fn wrong_viewer_foreign_origin_and_refresh_identity_are_refused() {
    let a = Account {
        origin: "https://api.linear.app".into(),
        user: "me@example.org".into(),
        ..Default::default()
    };
    let mut c = Client {
        kind: Kind::Linear,
        account: a.clone(),
        send: |_: &str, _: &str, _: Option<&Value>| {
            Ok(json!({"data":{"viewer":{"id":"other","email":"other@example.org"}}}))
        },
    };
    assert!(c.listed().unwrap_err().contains("different account"));
    assert!(
        Account {
            origin: "https://api.linear.app.evil".into(),
            ..a.clone()
        }
        .validate(Kind::Linear)
        .is_err()
    );
    assert!(
        Account {
            origin: "http://api.linear.app".into(),
            ..a.clone()
        }
        .validate(Kind::Linear)
        .is_err()
    );
    assert!(
        Account {
            origin: "https://user:pw@api.linear.app".into(),
            ..a
        }
        .validate(Kind::Linear)
        .is_err()
    );
    let identity = Identity {
        kind: Kind::Linear,
        origin: "https://api.linear.app".into(),
        account: "someone@example.org".into(),
        viewer: "old".into(),
        id: "uuid".into(),
        key: "ENG-9".into(),
    };
    assert!(c.one(&identity).unwrap_err().contains("origin and account"));
}
#[test]
fn unknown_adf_is_visible_and_depth_is_not_silently_truncated() {
    assert_eq!(
        adf(&json!({"type":"newNode","content":[{"type":"text","text":"kept"}]})).unwrap(),
        "[Unsupported Jira node: newNode] kept"
    );
    let mut v = json!({"type":"text","text":"tail"});
    for _ in 0..66 {
        v = json!({"type":"paragraph","content":[v]});
    }
    assert!(adf(&v).is_err());
}

#[test]
fn linked_jira_and_linear_refresh_exact_stable_ids() {
    for kind in Kind::ALL {
        let origin = if kind == Kind::Jira {
            "https://site.atlassian.net"
        } else {
            "https://api.linear.app"
        };
        let identity = Identity {
            kind,
            origin: origin.into(),
            account: "me@example.org".into(),
            viewer: "viewer-id".into(),
            id: "stable-id".into(),
            key: "APP-7".into(),
        };
        let mut c = Client {
            kind,
            account: Account {
                origin: origin.into(),
                user: identity.account.clone(),
                ..Default::default()
            },
            send: |_: &str, path: &str, body: Option<&Value>| {
                let issue = json!({"id":"stable-id","key":"APP-7","identifier":"APP-7","title":"Updated","description":"- [ ] updated","url":"https://linear.app/team/issue/APP-7/title","fields":{"summary":"Updated","description":"- [ ] updated","labels":[]}});
                if kind == Kind::Jira {
                    if path.ends_with("myself") {
                        Ok(json!({"accountId":"viewer-id","emailAddress":"me@example.org"}))
                    } else {
                        assert!(path.starts_with("/rest/api/3/issue/stable-id?fields="));
                        Ok(issue)
                    }
                } else if body.unwrap()["query"]
                    .as_str()
                    .unwrap()
                    .contains("viewer { id email }")
                {
                    Ok(json!({"data":{"viewer":{"id":"viewer-id","email":"me@example.org"}}}))
                } else {
                    assert_eq!(body.unwrap()["variables"]["id"], "stable-id");
                    Ok(json!({"data":{"issue":issue}}))
                }
            },
        };
        let issue = c.one(&identity).unwrap();
        assert_eq!(issue.title, "Updated");
        assert_eq!(issue.tracker.as_ref().unwrap(), &identity);
    }
}
#[test]
fn jira_repeated_and_missing_continuations_are_errors() {
    for missing in [false, true] {
        let mut c = Client {
            kind: Kind::Jira,
            account: Account {
                origin: "https://site.atlassian.net".into(),
                user: "me@example.org".into(),
                ..Default::default()
            },
            send: |_: &str, path: &str, _: Option<&Value>| {
                if path.ends_with("myself") {
                    return Ok(json!({"accountId":"viewer","emailAddress":"me@example.org"}));
                }
                Ok(if missing {
                    json!({"issues":[],"isLast":false})
                } else {
                    json!({"issues":[],"isLast":false,"nextPageToken":"repeat"})
                })
            },
        };
        assert!(c.listed().is_err());
    }
}

#[test]
fn configured_jira_criteria_take_precedence_through_link_and_card_data() {
    let row = json!({"id":"908","key":"APP-42","fields":{"summary":"Fix","description":{"type":"doc","content":[{"type":"heading","attrs":{"level":2},"content":[{"type":"text","text":"Acceptance criteria"}]},{"type":"paragraph","content":[{"type":"text","text":"old description requirement"}]}]},"customfield_10001":{"type":"doc","content":[{"type":"heading","attrs":{"level":2},"content":[{"type":"text","text":"New requirements"}]},{"type":"taskList","content":[{"type":"taskItem","attrs":{"state":"TODO"},"content":[{"type":"text","text":"selected custom-field requirement"}]}]}]}}});
    let mut client = Client {
        kind: Kind::Jira,
        account: Account {
            origin: "https://site.atlassian.net".into(),
            user: "me@example.org".into(),
            acceptance_field: "customfield_10001".into(),
            ..Default::default()
        },
        send: |_: &str, _: &str, _: Option<&Value>| -> Result<Value, String> {
            panic!("parse must not perform I/O")
        },
    };
    let parsed = client.parse(&row, "viewer").unwrap();
    assert!(parsed.body.contains("old description requirement"));
    let link = crate::repo::issue_work::link::IssueLink::of(&parsed);
    assert_eq!(link.kept, crate::repo::issue_work::criteria::Kept::Criteria);
    assert_eq!(
        link.criteria,
        "## New requirements\n\n- [ ] selected custom-field requirement"
    );
    let rendered = crate::repo::issue_work::criteria::lines(&link.criteria);
    assert!(rendered.iter().any(|l|matches!(l,crate::repo::issue_work::criteria::Line::Task{text,..} if text=="selected custom-field requirement")));
    assert!(!link.criteria.contains("old description requirement"));
    client.account.acceptance_field.clear();
    let fallback =
        crate::repo::issue_work::link::IssueLink::of(&client.parse(&row, "viewer").unwrap());
    assert_eq!(fallback.criteria, "old description requirement");
    client.account.acceptance_field = "customfield_10001".into();
    let mut empty = row.clone();
    empty["fields"]["customfield_10001"] = Value::Null;
    assert!(
        crate::repo::issue_work::link::IssueLink::of(&client.parse(&empty, "viewer").unwrap())
            .criteria
            .is_empty()
    );
    empty["fields"]
        .as_object_mut()
        .unwrap()
        .remove("customfield_10001");
    assert!(client.parse(&empty, "viewer").is_err());
}
