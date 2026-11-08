use super::*;
use crate::{
    pr_post::{LineComment, MergeMethod, Target, Verdict},
    pull_request::DiffSide,
};
use std::sync::{Arc, Mutex};
fn hosting() -> Hosting {
    Hosting {
        forge: Forge::Bitbucket,
        host: "bitbucket.org".into(),
        web: "https://bitbucket.org".into(),
        path: "team/repo".into(),
    }
}
fn pr() -> Value {
    json!({"id":7,"title":"Native PR","description":"Review body","state":"OPEN","draft":false,"author":{"uuid":"{author}","display_name":"Author"},"source":{"repository":{"uuid":"{repo}","full_name":"team/repo"},"branch":{"name":"topic"},"commit":{"hash":"a".repeat(40)}},"destination":{"repository":{"uuid":"{repo}","full_name":"team/repo"},"branch":{"name":"main"},"commit":{"hash":"b".repeat(40)}},"participants":[{"user":{"display_name":"Reviewer"},"approved":true}]})
}
fn comments() -> Vec<Value> {
    vec![
        json!({"id":2,"content":{"raw":"Old thread"},"user":{"display_name":"Reviewer"},"inline":{"path":"old.rs","from":2,"outdated":true}}),
        json!({"id":3,"parent":{"id":2},"content":{"raw":"Reply"},"user":{"display_name":"Author"}}),
    ]
}
#[derive(Default)]
struct Model {
    pr: Value,
    viewer: String,
    calls: Vec<(String, String, Option<Value>)>,
    foreign_next: bool,
    repeated: bool,
    comments_error: bool,
    drift: bool,
    diff_next: Option<String>,
}
fn recording() -> (Api, Arc<Mutex<Model>>) {
    let state = Arc::new(Mutex::new(Model {
        pr: pr(),
        viewer: "{viewer}".into(),
        ..Default::default()
    }));
    let m = state.clone();
    let api = Api::recording(
        Service::Bitbucket,
        hosting(),
        "{viewer}",
        Arc::new(move |method, path, body| {
            let mut m = m.lock().map_err(|e| e.to_string())?;
            m.calls.push((method.into(), path.into(), body.cloned()));
            let p = path.split('?').next().ok_or("path")?;
            if method != "GET" {
                return Ok(m.pr.clone());
            }
            Ok(if p == "user" {
                json!({"uuid":m.viewer})
            } else if p == "repositories/team/repo" {
                json!({"uuid":"{repo}","full_name":"team/repo"})
            } else if p.contains("/merge-base/") {
                json!({"hash":"c".repeat(40)})
            } else if p.ends_with("/comments/2") {
                comments()[0].clone()
            } else if p.ends_with("/comments") {
                if m.comments_error {
                    return Err("recorded comments failure".into());
                }
                json!({"values":comments()})
            } else if p.ends_with("/commits") {
                json!({"values":[{"hash":"a".repeat(40),"message":"Change","author":{"user":{"display_name":"Author"}}}]})
            } else if p.ends_with("/statuses") {
                if m.drift {
                    m.drift = false;
                    m.pr["source"]["commit"]["hash"] = json!("d".repeat(40));
                }
                json!({"values":[{"name":"Build","state":"SUCCESSFUL"}]})
            } else if p.ends_with("/diffstat") {
                return Err("recorded native PR diffstat returns 302, not a JSON page".into());
            } else if p.contains("/diffstat/") {
                assert!(p.starts_with("repositories/team/repo/diffstat/"));
                assert!(path.contains("topic=false") || path.contains("page=2"));
                let mut v = json!({"values":[{"old":{"path":"old.rs"},"new":{"path":"new.rs"},"lines_added":2,"lines_removed":1}]});
                if let Some(next) = &m.diff_next {
                    v["next"] = json!(next);
                }
                v
            } else if p.contains("/src/") {
                json!("one\ntwo\nthree\n")
            } else if p.ends_with("/pullrequests/7") {
                m.pr.clone()
            } else if p.ends_with("/pullrequests") {
                if m.foreign_next {
                    json!({"values":[],"next":"https://api.bitbucket.org.evil/2.0/repositories/team/repo/pullrequests?page=2"})
                } else if m.repeated {
                    json!({"values":[],"next":"https://api.bitbucket.org/2.0/repositories/team/repo/pullrequests?page=2"})
                } else if path.contains("page=2") {
                    json!({"values":[]})
                } else {
                    json!({"values":[m.pr.clone()],"next":"https://api.bitbucket.org/2.0/repositories/team/repo/pullrequests?page=2"})
                }
            } else {
                return Err(format!("Unexpected recording route {method} {path}"));
            })
        }),
    );
    (api, state)
}
fn writes(m: &Model) -> Vec<&(String, String, Option<Value>)> {
    m.calls
        .iter()
        .filter(|(method, _, _)| method != "GET")
        .collect()
}
#[test]
fn native_page_preserves_reviews_outdated_replies_and_honest_merge_capability() -> Result<(), String>
{
    let (api, _) = recording();
    let d = details::read(&api, 7)?;
    assert_eq!(d.forge, Some(Forge::Bitbucket));
    assert_eq!(d.base_oid, "c".repeat(40));
    assert_eq!(d.head_oid, "a".repeat(40));
    assert_eq!(d.changed_files, 1);
    assert_eq!((d.additions, d.deletions), (2, 1));
    assert_eq!(d.extras.threads.len(), 1);
    let t = &d.extras.threads[0];
    assert!(t.outdated && t.can_reply && t.can_resolve);
    assert_eq!(t.comments.len(), 2);
    assert!(d.extras.review_features.approve && d.extras.review_features.request_changes);
    assert!(!d.extras.review_features.pending);
    assert!(d.extras.merge_handoff);
    assert!(!d.extras.merge_allowed);
    assert!(
        d.extras
            .merge_blocker
            .as_ref()
            .is_some_and(|s| s.contains("expected-head"))
    );
    assert!(Bitbucket.pages().is_some());
    assert_eq!(
        crate::pull_request::repo_of_url(&d.url),
        Some(("bitbucket.org".into(), "team/repo".into()))
    );
    Ok(())
}
#[test]
fn native_comments_votes_lines_replies_and_resolution_use_documented_routes() -> Result<(), String>
{
    let (api, m) = recording();
    let d = details::read(&api, 7)?;
    let t = post::BitbucketTarget::new(api, &d).ok_or("target")?;
    t.comment("Conversation")?;
    t.review(Verdict::Approve, "")?;
    t.review(Verdict::RequestChanges, "")?;
    for side in [DiffSide::Left, DiffSide::Right] {
        t.comment_now(&LineComment {
            path: "new.rs".into(),
            line: 2,
            side,
            body: "Line review".into(),
        })?;
    }
    t.reply("2", "Original thread reply")?;
    t.set_resolved("2", true)?;
    t.set_resolved("2", false)?;
    assert!(t.merge(MergeMethod::Merge, false).is_err());
    assert!(t.reply("not-shown", "No").is_err());
    let m = m.lock().map_err(|e| e.to_string())?;
    let w = writes(&m);
    assert_eq!(w.len(), 8);
    assert!(
        w.iter()
            .any(|(method, p, _)| method == "POST" && p.ends_with("/approve"))
    );
    assert!(w.iter().any(|(_, p, _)| p.ends_with("/request-changes")));
    assert!(
        w.iter()
            .any(|(method, p, _)| method == "DELETE" && p.ends_with("/comments/2/resolve"))
    );
    assert!(
        w.iter()
            .any(|(_, _, b)| b.as_ref().is_some_and(|v| v["parent"]["id"] == 2))
    );
    assert!(w.iter().any(|(_, _, b)| {
        b.as_ref()
            .is_some_and(|v| v["inline"]["from"] == 2 && v["inline"]["path"] == "new.rs")
    }));
    assert!(
        m.calls
            .iter()
            .any(|(_, p, _)| p.contains(&format!("src/{}/old.rs", "c".repeat(40))))
    );
    assert!(!w.iter().any(|(_, p, _)| p.ends_with("/merge")));
    Ok(())
}
#[test]
fn changed_source_target_repository_account_and_profile_refuse_writes() -> Result<(), String> {
    for pointer in [
        "/source/commit/hash",
        "/destination/commit/hash",
        "/source/repository/uuid",
        "/destination/repository/uuid",
        "/source/branch/name",
        "/destination/branch/name",
        "/state",
    ] {
        let (api, m) = recording();
        let d = details::read(&api, 7)?;
        let t = post::BitbucketTarget::new(api, &d).ok_or("target")?;
        *m.lock()
            .map_err(|e| e.to_string())?
            .pr
            .pointer_mut(pointer)
            .ok_or("field")? = json!("changed");
        assert!(t.comment("No").is_err(), "{pointer}");
        assert!(writes(&*m.lock().map_err(|e| e.to_string())?).is_empty());
    }
    let (api, m) = recording();
    let d = details::read(&api, 7)?;
    let t = post::BitbucketTarget::new(api, &d).ok_or("target")?;
    m.lock().map_err(|e| e.to_string())?.viewer = "{other}".into();
    assert!(t.review(Verdict::Approve, "").is_err());
    assert!(writes(&*m.lock().map_err(|e| e.to_string())?).is_empty());
    let (api, m) = recording();
    let d = details::read(&api, 7)?;
    let t = post::BitbucketTarget::new(api, &d).ok_or("target")?;
    let mut c = crate::connections::current();
    c.error = Some("changed profile".into());
    c.run(|| assert!(t.set_resolved("2", true).is_err()));
    assert!(writes(&*m.lock().map_err(|e| e.to_string())?).is_empty());
    Ok(())
}
#[test]
fn list_create_pagination_and_host_binding_are_reachable_and_bounded() -> Result<(), String> {
    let (api, m) = recording();
    let rows = pages(
        &api,
        "repositories/team/repo/pullrequests?state=OPEN&pagelen=100",
    )?;
    assert_eq!(rows.len(), 1);
    assert_eq!(parse(&rows[0], &hosting())?.branch, "topic");
    assert_eq!(
        create(
            &api,
            &NewPullRequest {
                branch: "topic",
                base: "main",
                title: "Native",
                body: "Description",
                draft: true,
                reviewers: &["{reviewer}".into()]
            }
        )?,
        url(&hosting(), 7)
    );
    let body = m
        .lock()
        .map_err(|e| e.to_string())?
        .calls
        .iter()
        .find(|(method, _, _)| method == "POST")
        .and_then(|(_, _, v)| v.clone())
        .ok_or("create body")?;
    assert_eq!(body["source"]["repository"]["uuid"], "{repo}");
    assert_eq!(body["reviewers"][0]["uuid"], "{reviewer}");
    for field in ["foreign", "repeat"] {
        let (api, m) = recording();
        if field == "foreign" {
            m.lock().map_err(|e| e.to_string())?.foreign_next = true;
        } else {
            m.lock().map_err(|e| e.to_string())?.repeated = true;
        }
        assert!(pages(&api, "repositories/team/repo/pullrequests?pagelen=100").is_err());
    }
    let mut h = hosting();
    h.web = "http://bitbucket.org".into();
    assert!(endpoint(&h).is_err());
    h = hosting();
    h.path = "team/nested/repo".into();
    assert!(endpoint(&h).is_err());
    Ok(())
}
#[test]
fn source_read_retries_and_partial_discussion_failure_is_visible() -> Result<(), String> {
    let (api, m) = recording();
    m.lock().map_err(|e| e.to_string())?.drift = true;
    let d = details::read(&api, 7)?;
    assert_eq!(d.head_oid, "d".repeat(40));
    let (api, m) = recording();
    m.lock().map_err(|e| e.to_string())?.comments_error = true;
    let d = details::read(&api, 7)?;
    assert!(
        d.inline_error
            .as_ref()
            .is_some_and(|s| s.contains("comments failure"))
    );
    assert!(d.extras.threads.is_empty());
    Ok(())
}

#[test]
fn repository_diffstat_binds_common_and_source_commits_without_following_pr_redirects()
-> Result<(), String> {
    let (api, m) = recording();
    let d = details::read(&api, 7)?;
    assert_eq!((d.changed_files, d.additions, d.deletions), (1, 2, 1));
    let target = post::BitbucketTarget::new(api.clone(), &d).ok_or("target")?;
    target.comment_now(&LineComment {
        path: "new.rs".into(),
        line: 2,
        side: DiffSide::Left,
        body: "Renamed old side".into(),
    })?;
    {
        let model = m.lock().map_err(|e| e.to_string())?;
        let expected = format!(
            "repositories/team/repo/diffstat/{}..{}?topic=false&renames=true&pagelen=100",
            "a".repeat(40),
            "c".repeat(40)
        );
        assert_eq!(
            model
                .calls
                .iter()
                .filter(|(_, p, _)| p == &expected)
                .count(),
            2
        );
        assert!(
            !model
                .calls
                .iter()
                .any(|(_, p, _)| p.contains("pullrequests/7/diffstat"))
        );
        let w = writes(&model);
        assert_eq!(w.len(), 1);
        assert!(
            w[0].2
                .as_ref()
                .is_some_and(|v| v["inline"]["from"] == 2 && v["inline"]["path"] == "new.rs")
        );
    }
    for next in [
        "https://evil.invalid/2.0/repositories/team/repo/diffstat/anything?page=2".to_owned(),
        format!(
            "https://api.bitbucket.org/2.0/repositories/team/repo/diffstat/{}..{}?page=2",
            "a".repeat(40),
            "c".repeat(40)
        ),
    ] {
        m.lock().map_err(|e| e.to_string())?.diff_next = Some(next);
        assert!(diffstat(&api, &d.base_oid, &d.head_oid).is_err());
    }
    assert!(diffstat(&api, "main", &d.head_oid).is_err());
    Ok(())
}
