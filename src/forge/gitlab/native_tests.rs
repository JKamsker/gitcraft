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
fn native_origins_bind_scheme_host_port_and_project_without_treating_default_ports_as_new_servers() {
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
            f.git(&["config", "user.name", "Fixture author"])?;
            f.git(&["config", "user.email", "fixture@test.invalid"])?;
            f.git(&["config", "commit.gpgSign", "false"])?;
            f.git(&["config", "protocol.allow", "never"])?;
            std::fs::write(f.repo.join("old name.rs"), "context\nold\ntail\n")
                .map_err(|e| e.to_string())?;
            f.git(&["add", "."])?;
            f.git(&["commit", "-qm", "base"])?;
            let base = f.git_output(&["rev-parse", "HEAD"])?;
            std::fs::rename(f.repo.join("old name.rs"), f.repo.join("new name.rs"))
                .map_err(|e| e.to_string())?;
            std::fs::write(f.repo.join("new name.rs"), "context\nnew\ntail\n")
                .map_err(|e| e.to_string())?;
            f.git(&["add", "-A"])?;
            f.git(&["commit", "-qm", "change"])?;
            let head = f.git_output(&["rev-parse", "HEAD"])?;

            f.git(&[
                "remote",
                "add",
                "origin",
                "https://gitlab.com/group/sub/repo.git",
            ])?;
            let mut fixtures = json!({"mr":mr(),"project":project(),"viewer":{"username":"work"},"approvals":{"approvals_left":0,"user_can_approve":true},"discussions":[discussion()],"commits":[{"id":"a".repeat(40),"title":"A change","author_name":"Author"}],"diffs":[diff()],"mode":"normal"});
            fixtures["mr"]["sha"] = json!(head);
            fixtures["mr"]["diff_refs"] = json!({"base_sha":base,"start_sha":base,"head_sha":head});
            fixtures["discussions"][0]["notes"][0]["position"]["head_sha"] = json!(head);
            fixtures["discussions"][0]["notes"][0]["position"]["base_sha"] = json!(base);
            fixtures["discussions"][0]["notes"][0]["position"]["start_sha"] = json!(base);
            fixtures["commits"][0]["id"] = json!(head);
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
if f['mode'].startswith('ambient'):
 assert os.environ.get('GLAB_CONFIG_DIR') is None
 assert os.environ.get('GITLAB_TOKEN') in [None,'FIXTURE_SELECTED_TOKEN']
else:
 assert os.environ.get('GLAB_CONFIG_DIR')==str(root/'selected auth')
 assert os.environ.get('GITLAB_TOKEN')=='FIXTURE_SELECTED_TOKEN'
method=a[a.index('--method')+1] if '--method' in a else 'GET'
e=a[a.index('--method')+2] if '--method' in a else a[1]
body=json.loads(sys.stdin.read()) if '--input' in a else None
with (root/'requests.jsonl').open('a') as out:out.write(json.dumps({'method':method,'endpoint':e,'body':body})+'\n')
if e=='user':print(json.dumps(f['viewer']));sys.exit(0)
p='projects/group%2Fsub%2Frepo'
assert e==p or e.startswith(p+'/'),e
if method!='GET':print(json.dumps(f['mr'] if e.endswith('/merge_requests') else {'ok':True}));sys.exit(0)
plain=e.split('?')[0]
if plain==p:result=f['project']
elif plain.endswith('/merge_requests'):result=[f['mr']]
elif plain.endswith('/approvals'):result=f['approvals']
elif plain.endswith('/discussions'):
 if f['mode'] in ['pages','limit','overflow']:
  page=int(e.rsplit('page=',1)[1]);last=1 if f['mode']=='pages' else (10 if f['mode']=='limit' else 11)
  result=[f['discussions'][0]]*100 if page<=last else []
 elif f['mode']=='discussion-error':print('denied',file=sys.stderr);sys.exit(1)
 else:result=f['discussions']
elif plain.endswith('/commits'):result=f['commits']
elif plain.endswith('/diffs'):result=f['diffs']
else:
 result=f['mr']
 if f['mode']=='changing':
  n=int((root/'read-count').read_text()) if (root/'read-count').exists() else 0
  (root/'read-count').write_text(str(n+1));result=dict(result);result['sha']=('%040x'%n)
if f['mode']=='ambient-switch' and plain.endswith('/merge_requests/7'):
 f['viewer']['username']='personal';(root/'fixtures.json').write_text(json.dumps(f))
print(json.dumps(result))
"#.replace("ROOT",&serde_json::to_string(&f.root.to_string_lossy()).map_err(|e|e.to_string())?);
            std::fs::write(&f.program, script).map_err(|e| e.to_string())?;
            std::fs::set_permissions(&f.program, std::fs::Permissions::from_mode(0o700))
                .map_err(|e| e.to_string())?;
            Ok(f)
        }
        fn git(&self, args: &[&str]) -> Result<(), String> {
            self.git_output(args).map(|_| ())
        }
        fn git_output(&self, args: &[&str]) -> Result<String, String> {
            let mut c = Command::new("git");
            c.args([
                "-c",
                "core.hooksPath=/dev/null",
                "-c",
                "core.fsmonitor=false",
                "-C",
            ])
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
                Ok(String::from_utf8_lossy(&out.stdout).trim().to_owned())
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
        let head = f.git_output(&["rev-parse", "HEAD"])?;
        let refs = f.git_output(&["show-ref"])?;
        let index = std::fs::read(f.repo.join(".git/index")).map_err(|e| e.to_string())?;
        f.run(|api| {
            assert_eq!(GitLab.list(&f.repo, &f.hosting())?.len(), 1);
            let d = details::read(api, 7)?;
            let mut other_origin = f.hosting();
            other_origin.web = "https://gitlab.com:8443".into();
            assert!(post::GitLabTarget::new(
                api::Api::new(&f.repo, &other_origin)?, &d,
            ).is_none());
            let files = crate::pull_request::files(
                &f.repo,
                &d,
                &[crate::git::Remote {
                    name: "origin".into(),
                    url: "https://gitlab.com/group/sub/repo.git".into(),
                }],
                None,
            )?;
            assert_eq!(files.to, head);
            assert!(files.files.iter().any(|file| file.path == "new name.rs"));
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
            assert_eq!(
                GitLab.create(
                    &f.repo,
                    &f.hosting(),
                    &NewPullRequest {
                        branch: "topic",
                        base: "main",
                        title: "New MR",
                        body: "Body $(data)",
                        draft: true
                    }
                )?,
                "https://gitlab.com/group/sub/repo/-/merge_requests/7"
            );
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
        assert_eq!(writes.len(), 7);
        assert_eq!(
            writes[0]["body"]["body"],
            "quotes \" and $(only text)\nnext"
        );
        assert_eq!(writes[1]["body"]["position"]["new_line"], 2);
        assert_eq!(writes[4]["body"]["sha"], head);
        assert_eq!(
            writes[5]["body"],
            json!({"sha":head,"squash":true,"should_remove_source_branch":true})
        );
        assert_eq!(writes[6]["body"]["title"], "Draft: New MR");
        assert_eq!(f.git_output(&["show-ref"])?, refs);
        assert_eq!(
            std::fs::read(f.repo.join(".git/index")).map_err(|e| e.to_string())?,
            index
        );
        Ok(())
    }
    #[test]
    fn stale_head_account_repository_and_profile_cannot_send_a_write() -> Result<(), String> {
        for changed in ["port", "web-origin", "head", "start", "base", "id", "viewer", "remote", "profile"] {
            let f = Fixture::new()?;
            f.run(|api| {
                let d = details::read(api, 7)?;
                let t = post::GitLabTarget::new(api::Api::new(&f.repo, &f.hosting())?, &d)
                    .ok_or("target")?;
                let mut fixture = f.fixture()?;
                match changed {
                    "port" => f.git(&[
                        "remote", "set-url", "origin",
                        "https://gitlab.com:8443/group/sub/repo.git",
                    ])?,
                    "web-origin" => fixture["mr"]["web_url"] =
                        json!("https://gitlab.com:8443/group/sub/repo/-/merge_requests/7"),
                    "head" => fixture["mr"]["sha"] = json!("d".repeat(40)),
                    "id" => fixture["mr"]["id"] = json!(999),
                    "viewer" => fixture["viewer"]["username"] = json!("personal"),
                    "start" => fixture["mr"]["diff_refs"]["start_sha"] = json!("d".repeat(40)),
                    "base" => fixture["mr"]["diff_refs"]["base_sha"] = json!("d".repeat(40)),
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
    fn default_cli_account_is_pinned_to_the_displayed_viewer_even_if_it_changes_mid_action()
    -> Result<(), String> {
        let mut f = Fixture::new()?;
        f.profile.choices.gitlab = crate::connections::Account::on("gitlab.com");
        let mut fixture = f.fixture()?;
        fixture["mode"] = json!("ambient");
        f.store(&fixture)?;
        f.run(|api| {
            let d = details::read(api, 7)?;
            assert_eq!(d.review_decision, ""); // satisfied zero-approval rules are not a review.
            let t = post::GitLabTarget::new(api::Api::new(&f.repo, &f.hosting())?, &d)
                .ok_or("target")?;
            t.comment("Authorized as the displayed work viewer")?;
            let mut fixture = f.fixture()?;
            fixture["mode"] = json!("ambient-switch");
            f.store(&fixture)?;
            assert!(t.comment("Must not publish as personal").is_err());
            Ok(())
        })?;
        assert_eq!(
            f.requests()?
                .iter()
                .filter(|r| r["method"] != "GET")
                .count(),
            1
        );
        let mut pending = Fixture::new()?;
        pending.profile.error = Some("Imported profile setup is pending".into());
        assert!(pending.run(|api| details::read(api, 7)).is_err());
        assert!(pending.requests()?.is_empty());
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
        fixture["mode"] = json!("limit");
        f.store(&fixture)?;
        f.run(|api| {
            assert_eq!(api.all(&format!("{}/discussions", api.mr(7)))?.len(), 1000);
            Ok(())
        })?;
        fixture["mode"] = json!("overflow");
        f.store(&fixture)?;
        assert!(
            f.run(|api| api.all(&format!("{}/discussions", api.mr(7))))
                .is_err_and(|e| e.contains("1,000"))
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
