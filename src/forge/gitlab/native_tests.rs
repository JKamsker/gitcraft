use super::*;
use crate::pr_post::{LineComment, MergeMethod, Target, Verdict};
use crate::pull_request::DiffSide;
use serde_json::{Value, json};

fn mr() -> Value {
    json!({"id":71,"iid":7,"title":"A native merge request","description":"Review **this**","state":"opened","sha":"a".repeat(40),"diff_refs":{"base_sha":"b".repeat(40),"start_sha":"c".repeat(40),"head_sha":"a".repeat(40)},"source_branch":"topic","target_branch":"main","source_project_id":8,"target_project_id":8,"author":{"username":"author"},"user":{"can_merge":true},"web_url":"https://gitlab.com/group/sub/repo/-/merge_requests/7","detailed_merge_status":"mergeable","head_pipeline":{"id":5,"status":"success","web_url":"https://gitlab.com/pipelines/5"}})
}
fn project() -> Value {
    json!({"id":8,"path_with_namespace":"group/sub/repo","merge_method":"merge","squash_option":"default_off","permissions":{"project_access":{"access_level":30}}})
}
fn discussion() -> Value {
    json!({"id":"thread-with-a-removed-line","notes":[{"id":11,"body":"Why remove this?","author":{"username":"reviewer"},"resolvable":true,"resolved":false,"position":{"head_sha":"a".repeat(40),"base_sha":"b".repeat(40),"start_sha":"c".repeat(40),"old_path":"old name.rs","new_path":"new name.rs","old_line":2,"new_line":null},"created_at":"2026-11-01T01:00:00Z"},{"id":12,"body":"A reply","author":{"username":"author"}}]})
}
fn diff() -> Value {
    json!({"old_path":"old name.rs","new_path":"new name.rs","diff":"@@ -1,3 +1,3 @@\n context\n-old\n+new\n tail\n"})
}

#[test]
fn native_blockers_unknown_policy_and_approval_permissions_fail_closed() {
    let mut m = mr();
    let p = project();
    let approvals = Ok(json!({"approvals_left":0,"user_can_approve":true}));
    assert!(details::blocker(&m, &p, &approvals).is_none());
    for status in [
        "not_approved",
        "discussions_not_resolved",
        "ci_must_pass",
        "ci_still_running",
        "need_rebase",
        "checking",
        "external_status_checks",
        "",
    ] {
        m["detailed_merge_status"] = json!(status);
        assert!(details::blocker(&m, &p, &approvals).is_some(), "{status}");
    }
    m = mr();
    m["user"] = Value::Null;
    assert!(details::blocker(&m, &p, &approvals).is_some());
    assert!(details::blocker(&mr(), &json!({}), &approvals).is_some());
    assert!(details::blocker(&mr(), &p, &Err("denied".into())).is_some());
    assert!(
        details::blocker(
            &mr(),
            &p,
            &Ok(json!({"approved":true,"user_can_approve":true}))
        )
        .is_none()
    );
    assert!(details::blocker(&mr(), &p, &Ok(json!({"approved":false}))).is_some());
    assert!(details::blocker(&mr(), &p, &Ok(json!({}))).is_some());
}
#[test]
fn discussions_keep_removed_lines_replies_resolution_and_native_capabilities() -> Result<(), String>
{
    let approvals = Ok(
        json!({"approvals_left":1,"user_can_approve":false,"approved_by":[{"user":{"username":"reviewer"}}]}),
    );
    let d = details::assemble(
        &mr(),
        &project(),
        &json!({"username":"work","id":19}),
        &approvals,
        &Ok(vec![discussion()]),
        &[],
        7,
    )?;
    assert_eq!(d.forge, Some(super::super::Forge::GitLab));
    assert_eq!(d.extras.viewer.as_deref(), Some("work"));
    let t = &d.extras.threads[0];
    assert_eq!(t.side, DiffSide::Left);
    assert_eq!(t.line, Some(2));
    assert_eq!(t.path, "new name.rs");
    assert_eq!(t.comments.len(), 2);
    assert!(t.can_resolve);
    assert!(t.can_reply);
    assert!(!t.outdated);
    assert!(!d.extras.review_features.pending);
    assert!(!d.extras.review_features.request_changes);
    assert!(!d.extras.review_features.approve);
    assert!(!d.extras.review_features.apply_suggestions);
    assert!(!d.extras.rebase_allowed);
    assert!(
        d.timeline
            .iter()
            .any(|e| matches!(e.kind, crate::pull_request::EntryKind::Review("approved")))
    );
    let mut changed = mr();
    changed["diff_refs"]["start_sha"] = json!("d".repeat(40));
    assert!(!details::same_revision(&mr(), &changed));
    Ok(())
}
#[test]
fn native_line_positions_keep_context_and_rename_coordinates_and_refuse_unread_lines()
-> Result<(), String> {
    let mut c = LineComment {
        path: "new name.rs".into(),
        line: 2,
        side: DiffSide::Right,
        body: "A precise comment".into(),
    };
    let p = post::position(&mr(), &[diff()], &c)?;
    assert_eq!(p["new_line"], 2);
    assert!(p.get("old_line").is_none());
    assert_eq!(p["old_path"], "old name.rs");
    assert_eq!(p["head_sha"], "a".repeat(40));
    c.side = DiffSide::Left;
    let p = post::position(&mr(), &[diff()], &c)?;
    assert_eq!(p["old_line"], 2);
    assert!(p.get("new_line").is_none());
    c.line = 3;
    let p = post::position(&mr(), &[diff()], &c)?;
    assert_eq!(p["old_line"], 3);
    assert_eq!(p["new_line"], 3);
    c.line = 200;
    assert!(post::position(&mr(), &[diff()], &c).is_err());
    assert!(post::position(&mr(), &[diff(), diff()], &c).is_err());
    Ok(())
}

#[test]
fn native_origins_bind_scheme_host_port_and_project_without_treating_default_ports_as_new_servers()
{
    let mut hosting = Hosting {
        forge: super::super::Forge::GitLab,
        host: "gitlab.fixture.invalid".into(),
        path: "group/project".into(),
        web: "https://gitlab.fixture.invalid:8443".into(),
    };
    assert!(api::source_matches(
        &hosting,
        "https://gitlab.fixture.invalid:8443/group/project/-/merge_requests/7",
    ));
    for url in [
        "https://gitlab.fixture.invalid/group/project/-/merge_requests/7",
        "https://gitlab.fixture.invalid:9443/group/project/-/merge_requests/7",
        "http://gitlab.fixture.invalid:8443/group/project/-/merge_requests/7",
        "https://other.fixture.invalid:8443/group/project/-/merge_requests/7",
        "https://gitlab.fixture.invalid:8443/another/project/-/merge_requests/7",
        "https://user@gitlab.fixture.invalid:8443/group/project/-/merge_requests/7",
    ] {
        assert!(!api::source_matches(&hosting, url), "{url}");
    }
    hosting.web = "https://gitlab.fixture.invalid".into();
    assert!(api::source_matches(
        &hosting,
        "https://gitlab.fixture.invalid:443/group/project/-/merge_requests/7",
    ));
}

#[derive(Default)]
struct Recording {
    mr: Value,
    viewer: Value,
    writes: Vec<(String, String, Option<Value>)>,
    discussion_error: bool,
    approvals_error: bool,
    overflow: bool,
    drift_on_diff: bool,
}
fn recording() -> (api::Api, std::sync::Arc<std::sync::Mutex<Recording>>) {
    let state = std::sync::Arc::new(std::sync::Mutex::new(Recording {
        mr: mr(),
        viewer: json!({"id":19,"username":"work"}),
        ..Default::default()
    }));
    let hosting = Hosting {
        forge: super::super::Forge::GitLab,
        host: "gitlab.com".into(),
        web: "https://gitlab.com".into(),
        path: "group/sub/repo".into(),
    };
    let mut api = api::Api::new(
        std::path::Path::new("/nonexistent-recording-worktree"),
        &hosting,
    )
    .unwrap();
    let model = state.clone();
    api.record = Some(std::sync::Arc::new(move |method, endpoint, body| {
        let mut state = model.lock().unwrap();
        if method != "GET" {
            state
                .writes
                .push((method.into(), endpoint.into(), body.cloned()));
            return Ok(if endpoint.ends_with("/merge") {
                json!({"state":"merged"})
            } else {
                json!({"id":90})
            });
        }
        let plain = endpoint.split('?').next().unwrap();
        Ok(if plain == "user" {
            state.viewer.clone()
        } else if plain.ends_with("/approvals") {
            if state.approvals_error {
                return Err("approvals unavailable".into());
            }
            json!({"approvals_left":0,"user_can_approve":true})
        } else if plain.ends_with("/discussions") {
            if state.discussion_error {
                return Err("discussions unavailable".into());
            }
            if state.overflow {
                Value::Array(vec![discussion(); 100])
            } else {
                json!([discussion(),{"id":"conversation","individual_note":false,"notes":[{"id":50,"body":"Conversation topic","author":{"username":"work"},"resolvable":true,"resolved":false}]}])
            }
        } else if plain.ends_with("/commits") {
            json!([{"id":"a".repeat(40),"title":"Change","author_name":"Author"}])
        } else if plain.ends_with("/diffs") {
            if state.drift_on_diff {
                state.mr["diff_refs"]["start_sha"] = json!("d".repeat(40));
            }
            json!([diff()])
        } else if plain.ends_with("/merge_requests/7") {
            state.mr.clone()
        } else if plain == "projects/group%2Fsub%2Frepo" {
            project()
        } else {
            return Err(format!("Unexpected recorded route: {endpoint}"));
        })
    }));
    (api, state)
}
#[test]
fn full_read_and_revision_bound_writes_record_native_protocol_without_executing()
-> Result<(), String> {
    let (api, state) = recording();
    let shown = details::read(&api, 7)?;
    assert_eq!(shown.changed_files, 1);
    assert_eq!((shown.additions, shown.deletions), (1, 1));
    assert_eq!(shown.extras.native_account.as_deref(), Some("19"));
    assert!(
        shown
            .extras
            .threads
            .iter()
            .any(|t| t.id == "conversation" && t.path.is_empty() && t.can_resolve)
    );
    let target = post::GitLabTarget::new(api, &shown).unwrap();
    target.comment("Reviewed conversation comment")?;
    target.comment_now(&LineComment {
        path: "new name.rs".into(),
        line: 3,
        side: DiffSide::Right,
        body: "Context line".into(),
    })?;
    target.reply("thread-with-a-removed-line", "Reviewed reply")?;
    target.reply("conversation", "Conversation reply")?;
    target.set_resolved("conversation", true)?;
    target.set_resolved("thread-with-a-removed-line", false)?;
    target.review(Verdict::Approve, "Approval comment")?;
    target.merge(MergeMethod::Squash, true)?;
    let state = state.lock().unwrap();
    let bodies = &state.writes;
    assert_eq!(bodies.len(), 9);
    let line = &bodies
        .iter()
        .find(|(_, p, _)| p.ends_with("/discussions"))
        .unwrap()
        .2
        .as_ref()
        .unwrap()["position"];
    assert_eq!(line["old_line"], 3);
    assert_eq!(line["new_line"], 3);
    assert_eq!(line["base_sha"], "b".repeat(40));
    assert_eq!(line["start_sha"], "c".repeat(40));
    assert!(
        bodies.iter().any(
            |(_, p, b)| p.ends_with("/approve") && b.as_ref().unwrap()["sha"] == "a".repeat(40)
        )
    );
    assert!(bodies.iter().any(|(_, p, b)| p.ends_with("/merge")
        && b.as_ref().unwrap()["sha"] == "a".repeat(40)
        && b.as_ref().unwrap()["squash"] == true));
    Ok(())
}
#[test]
fn stale_full_revision_stable_account_and_unshown_discussions_refuse_recorded_writes()
-> Result<(), String> {
    for pointer in [
        "/sha",
        "/diff_refs/base_sha",
        "/diff_refs/start_sha",
        "/diff_refs/head_sha",
        "/source_branch",
        "/target_branch",
        "/source_project_id",
        "/target_project_id",
        "/web_url",
        "/state",
    ] {
        let (api, state) = recording();
        let shown = details::read(&api, 7)?;
        let target = post::GitLabTarget::new(api, &shown).unwrap();
        *state.lock().unwrap().mr.pointer_mut(pointer).unwrap() = json!("changed");
        assert!(target.comment("Must not publish").is_err(), "{pointer}");
        assert!(state.lock().unwrap().writes.is_empty());
    }
    let (api, state) = recording();
    let shown = details::read(&api, 7)?;
    let target = post::GitLabTarget::new(api, &shown).unwrap();
    state.lock().unwrap().viewer["id"] = json!(999);
    assert!(target.review(Verdict::Approve, "").is_err());
    assert!(state.lock().unwrap().writes.is_empty());
    let (api, state) = recording();
    let shown = details::read(&api, 7)?;
    let target = post::GitLabTarget::new(api, &shown).unwrap();
    assert!(target.reply("not-displayed", "must refuse").is_err());
    assert!(target.comment("/merge").is_err());
    assert!(target.comment("Review\n /approve").is_err());
    assert!(state.lock().unwrap().writes.is_empty());
    let mut other = crate::connections::current();
    other.error = Some("another profile".into());
    other.run(|| assert!(target.comment("stale profile").is_err()));
    Ok(())
}
#[test]
fn partial_read_errors_and_pagination_overflow_remain_explicit_and_fail_closed()
-> Result<(), String> {
    let (api, state) = recording();
    state.lock().unwrap().discussion_error = true;
    state.lock().unwrap().approvals_error = true;
    let shown = details::read(&api, 7)?;
    assert!(shown.inline_error.as_ref().unwrap().contains("unavailable"));
    assert!(shown.extras.merge_blocker.is_some());
    assert!(!shown.extras.review_features.approve);
    let (api, state) = recording();
    state.lock().unwrap().overflow = true;
    assert!(
        api.all("projects/group%2Fsub%2Frepo/merge_requests/7/discussions")
            .unwrap_err()
            .contains("1,000")
    );
    let (api, state) = recording();
    state.lock().unwrap().drift_on_diff = true;
    // First attempt observes a changed start; retry yields one coherent newer snapshot.
    let shown = details::read(&api, 7)?;
    assert!(
        shown
            .extras
            .review_revision
            .unwrap()
            .contains(&"d".repeat(40))
    );
    Ok(())
}
#[test]
fn native_command_transform_uses_fixed_program_exact_authority_and_json_stdin() -> Result<(), String>
{
    let hosting = Hosting {
        forge: super::super::Forge::GitLab,
        host: "gitlab.fixture.invalid".into(),
        web: "https://gitlab.fixture.invalid:8443".into(),
        path: "group/sub/repo".into(),
    };
    let cmd = api::command(
        std::process::Command::new("/recording/glab"),
        &hosting,
        "POST",
        "projects/group%2Fsub%2Frepo/merge_requests/7/discussions",
        true,
    )?;
    assert_eq!(cmd.get_program(), "/recording/glab");
    let args = cmd
        .get_args()
        .map(|s| s.to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    assert!(
        args.windows(2)
            .any(|a| a == ["--hostname", "gitlab.fixture.invalid:8443"])
    );
    assert!(args.ends_with(&["--input".into(), "-".into()]));
    assert!(
        cmd.get_envs()
            .any(|(k, v)| k == "GITLAB_TOKEN" && v.is_none())
    );
    assert!(
        api::command(
            std::process::Command::new("/recording/glab"),
            &hosting,
            "POST",
            "https://foreign.invalid/api",
            true
        )
        .is_err()
    );
    let mut http = hosting;
    http.web = "http://gitlab.fixture.invalid:8443".into();
    assert!(
        api::command(
            std::process::Command::new("/recording/glab"),
            &http,
            "GET",
            "user",
            false
        )
        .is_err()
    );
    Ok(())
}
