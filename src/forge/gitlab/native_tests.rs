use super::*;
use crate::pr_post::{LineComment, MergeMethod, Target, Verdict};
use crate::pull_request::{DiffSide, at};
use serde_json::{Value, json};

fn mr() -> Value {
    json!({"id":71,"iid":7,"title":"A native merge request","description":"Review **this**","state":"opened","sha":"a".repeat(40),"diff_refs":{"base_sha":"b".repeat(40),"start_sha":"c".repeat(40),"head_sha":"a".repeat(40)},"source_branch":"topic","target_branch":"main","source_project_id":8,"target_project_id":8,"author":{"username":"author"},"user":{"can_merge":true},"web_url":"https://gitlab.com/group/sub/repo/-/merge_requests/7","detailed_merge_status":"mergeable","head_pipeline":{"id":5,"status":"success","web_url":"https://gitlab.com/pipelines/5"}})
}
fn project() -> Value {
    json!({"id":8,"path_with_namespace":"group/sub/repo","merge_method":"merge","squash_option":"default_off","permissions":{"project_access":{"access_level":30}}})
}
fn discussion() -> Value {
    json!({"id":"thread-with-a-removed-line","notes":[{"id":11,"body":"Why remove this?","author":{"username":"reviewer"},"resolvable":true,"resolved":false,"position":{"head_sha":"a".repeat(40),"old_path":"old name.rs","new_path":"new name.rs","old_line":2,"new_line":null},"created_at":"2026-11-01T01:00:00Z"},{"id":12,"body":"A reply","author":{"username":"author"}}]})
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
        &json!({"username":"work"}),
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
    assert_eq!(t.path, "old name.rs");
    assert_eq!(t.comments.len(), 2);
    assert!(t.can_resolve);
    assert!(t.can_reply);
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

#[cfg(unix)]
mod recording {
    use super::*;
    use std::{os::unix::fs::PermissionsExt, path::PathBuf, process::Command};
    struct Fixture {
        root: PathBuf,
        repo: PathBuf,
        program: PathBuf,
        profile: crate::connections::Context,
    }
    impl Fixture {
        fn new() -> Result<Self, String> {
            let root = std::env::temp_dir().join(format!(
                "gitcraft-gitlab-review-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_err(|e| e.to_string())?
                    .as_nanos()
            ));
            std::fs::create_dir(&root).map_err(|e| e.to_string())?;
            let repo = root.join("repository");
            std::fs::create_dir(&repo).map_err(|e| e.to_string())?;
            let program = root.join("glab-recording");
            let profile = crate::connections::Context {
                choices: crate::connections::Choices {
                    gitlab: crate::connections::Account {
                        host: "gitlab.com".into(),
                        user: "work".into(),
                        config_dir: root.join("selected auth").to_string_lossy().into_owned(),
                    },
                    ..Default::default()
                },
                ..Default::default()
            };
            let f = Self {
                root,
                repo,
                program,
                profile,
            };
            f.git(&["init", "-q"])?;
            f.git(&[
                "remote",
                "add",
                "origin",
                "https://gitlab.com/group/sub/repo.git",
            ])?;
            let fixtures = json!({"mr":mr(),"project":project(),"viewer":{"username":"work"},"approvals":{"approvals_left":0,"user_can_approve":true},"discussions":[discussion()],"commits":[{"id":"a".repeat(40),"title":"A change","author_name":"Author"}],"diffs":[diff()],"mode":"normal"});
            f.store(&fixtures)?;
            let script=r#"#!/usr/bin/python3
import sys,json,os,pathlib
root=pathlib.Path(ROOT)
a=sys.argv[1:]
f=json.loads((root/'fixtures.json').read_text())
if a[:2]==['config','get']:
 print('false' if a[2]=='is_oauth2' else 'FIXTURE_SELECTED_TOKEN');sys.exit(0)
if a[0]!='api':raise Exception('non API tool call')
host=a[a.index('--hostname')+1]
assert host=='gitlab.com',host
assert os.environ.get('GLAB_CONFIG_DIR')==str(root/'selected auth')
assert os.environ.get('GITLAB_TOKEN')=='FIXTURE_SELECTED_TOKEN'
method=a[a.index('--method')+1] if '--method' in a else 'GET'
e=a[a.index('--method')+2] if '--method' in a else a[1]
body=json.loads(sys.stdin.read()) if '--input' in a else None
with (root/'requests.jsonl').open('a') as out:out.write(json.dumps({'method':method,'endpoint':e,'body':body})+'\n')
if e=='user':print(json.dumps(f['viewer']));sys.exit(0)
p='projects/group%2Fsub%2Frepo'
assert e==p or e.startswith(p+'/'),e
if method!='GET':print(json.dumps({'ok':True}));sys.exit(0)
plain=e.split('?')[0]
if plain==p:result=f['project']
elif plain.endswith('/approvals'):result=f['approvals']
elif plain.endswith('/discussions'):
 if f['mode']=='pages':result=[f['discussions'][0]]*100 if 'page=1&' in e or e.endswith('page=1') else []
 elif f['mode']=='discussion-error':print('denied',file=sys.stderr);sys.exit(1)
 else:result=f['discussions']
elif plain.endswith('/commits'):result=f['commits']
elif plain.endswith('/diffs'):result=f['diffs']
else:
 result=f['mr']
 if f['mode']=='changing':
  n=int((root/'read-count').read_text()) if (root/'read-count').exists() else 0
  (root/'read-count').write_text(str(n+1));result=dict(result);result['sha']=('%040x'%n)
print(json.dumps(result))
"#.replace("ROOT",&serde_json::to_string(&f.root.to_string_lossy()).map_err(|e|e.to_string())?);
            std::fs::write(&f.program, script).map_err(|e| e.to_string())?;
            std::fs::set_permissions(&f.program, std::fs::Permissions::from_mode(0o700))
                .map_err(|e| e.to_string())?;
            Ok(f)
        }
        fn git(&self, args: &[&str]) -> Result<(), String> {
            let mut c = Command::new("git");
            c.arg("-C")
                .arg(&self.repo)
                .args(args)
                .env("GIT_CONFIG_GLOBAL", "/dev/null")
                .env("GIT_CONFIG_NOSYSTEM", "1");
            for e in [
                "GIT_DIR",
                "GIT_WORK_TREE",
                "GIT_COMMON_DIR",
                "GIT_CONFIG_COUNT",
                "GIT_CONFIG_PARAMETERS",
                "GIT_INDEX_FILE",
            ] {
                c.env_remove(e);
            }
            let out = c.output().map_err(|e| e.to_string())?;
            if out.status.success() {
                Ok(())
            } else {
                Err(String::from_utf8_lossy(&out.stderr).into_owned())
            }
        }
        fn hosting(&self) -> Hosting {
            Hosting {
                forge: super::super::super::Forge::GitLab,
                host: "gitlab.com".into(),
                path: "group/sub/repo".into(),
                web: "https://gitlab.com".into(),
            }
        }
        fn store(&self, v: &Value) -> Result<(), String> {
            std::fs::write(self.root.join("fixtures.json"), v.to_string())
                .map_err(|e| e.to_string())
        }
        fn fixture(&self) -> Result<Value, String> {
            serde_json::from_slice(
                &std::fs::read(self.root.join("fixtures.json")).map_err(|e| e.to_string())?,
            )
            .map_err(|e| e.to_string())
        }
        fn run<T>(&self, job: impl FnOnce(&api::Api) -> Result<T, String>) -> Result<T, String> {
            crate::connections::with_test_program("glab", &self.program, || {
                self.profile
                    .run(|| job(&api::Api::new(&self.repo, &self.hosting())?))
            })
        }
        fn requests(&self) -> Result<Vec<Value>, String> {
            let p = self.root.join("requests.jsonl");
            if !p.exists() {
                return Ok(vec![]);
            }
            std::fs::read_to_string(p)
                .map_err(|e| e.to_string())?
                .lines()
                .map(|s| serde_json::from_str(s).map_err(|e| e.to_string()))
                .collect()
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }

    #[test]
    fn recording_cli_reads_the_full_mr_and_native_action_payloads_without_network()
    -> Result<(), String> {
        let f = Fixture::new()?;
        f.run(|api| {
            let d = details::read(api, 7)?;
            assert_eq!(d.changed_files, 1);
            assert_eq!((d.additions, d.deletions), (1, 1));
            assert_eq!(d.checks[0].state, crate::pull_request::CheckState::Passed);
            let t = post::GitLabTarget::new(api::Api::new(&f.repo, &f.hosting())?, &d)
                .ok_or("target")?;
            t.comment("quotes \" and $(only text)\nnext")?;
            t.comment_now(&LineComment {
                path: "new name.rs".into(),
                line: 2,
                side: DiffSide::Right,
                body: "Line".into(),
            })?;
            t.reply("thread-with-a-removed-line", "Reply")?;
            t.set_resolved("thread-with-a-removed-line", true)?;
            t.review(Verdict::Approve, "")?;
            t.merge(MergeMethod::Squash, true)?;
            assert!(
                t.add_pending(&LineComment {
                    path: "new name.rs".into(),
                    line: 2,
                    side: DiffSide::Right,
                    body: "Unsent".into()
                })
                .is_err()
            );
            Ok(())
        })?;
        let writes: Vec<_> = f
            .requests()?
            .into_iter()
            .filter(|r| r["method"] != "GET")
            .collect();
        assert_eq!(writes.len(), 6);
        assert_eq!(
            writes[0]["body"]["body"],
            "quotes \" and $(only text)\nnext"
        );
        assert_eq!(writes[1]["body"]["position"]["new_line"], 2);
        assert_eq!(writes[4]["body"]["sha"], "a".repeat(40));
        assert_eq!(
            writes[5]["body"],
            json!({"sha":"a".repeat(40),"squash":true,"should_remove_source_branch":true})
        );
        Ok(())
    }
    #[test]
    fn stale_head_account_repository_and_profile_cannot_send_a_write() -> Result<(), String> {
        for changed in ["head", "viewer", "remote", "profile"] {
            let f = Fixture::new()?;
            f.run(|api| {
                let d = details::read(api, 7)?;
                let t = post::GitLabTarget::new(api::Api::new(&f.repo, &f.hosting())?, &d)
                    .ok_or("target")?;
                let mut fixture = f.fixture()?;
                match changed {
                    "head" => fixture["mr"]["sha"] = json!("d".repeat(40)),
                    "viewer" => fixture["viewer"]["username"] = json!("personal"),
                    "remote" => f.git(&[
                        "remote",
                        "set-url",
                        "origin",
                        "https://gitlab.com/other/project.git",
                    ])?,
                    _ => {}
                }
                f.store(&fixture)?;
                let result = if changed == "profile" {
                    crate::connections::Context::default().run(|| t.comment("must not send"))
                } else {
                    t.comment("must not send")
                };
                assert!(result.is_err(), "{changed}");
                Ok(())
            })?;
            assert!(
                f.requests()?.iter().all(|r| r["method"] == "GET"),
                "{changed}"
            );
        }
        Ok(())
    }
    #[test]
    fn pagination_optional_discussion_errors_and_changing_revision_are_explicit()
    -> Result<(), String> {
        let f = Fixture::new()?;
        let mut fixture = f.fixture()?;
        fixture["mode"] = json!("pages");
        f.store(&fixture)?;
        f.run(|api| {
            let d = details::read(api, 7)?;
            assert_eq!(d.extras.threads.len(), 100);
            Ok(())
        })?;
        assert!(
            f.requests()?
                .iter()
                .any(|r| at(r, "/endpoint").ends_with("page=2"))
        );
        fixture["mode"] = json!("discussion-error");
        f.store(&fixture)?;
        f.run(|api| {
            let d = details::read(api, 7)?;
            assert!(d.inline_error.is_some());
            assert!(d.extras.threads.is_empty());
            Ok(())
        })?;
        fixture["mode"] = json!("changing");
        f.store(&fixture)?;
        assert!(
            f.run(|api| details::read(api, 7))
                .is_err_and(|e| e.contains("changed"))
        );
        Ok(())
    }
}
