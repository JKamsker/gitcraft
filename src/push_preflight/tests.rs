use super::*;
use std::path::PathBuf;

#[test]
fn reviewed_auth_receipts_keep_the_actual_failing_endpoint_out_of_logs() {
    let a = "https://alice:DO_NOT_LOG@a.invalid/team/repo.git?private=DUMMY";
    let b = "https://bob@b.invalid/another/repo.git";
    let ea = remember_auth_endpoint(a);
    let eb = remember_auth_endpoint(b);
    assert!(git::is_auth_error(&ea));
    assert!(!ea.contains("DO_NOT_LOG") && !ea.contains("a.invalid"));
    assert_eq!(take_auth_endpoint(&eb).as_deref(), Some(b));
    assert_eq!(take_auth_endpoint(&ea).as_deref(), Some(a));
    assert!(take_auth_endpoint(&ea).is_none());
    assert!(is_reviewed_auth_error(&ea)); // expiration never permits a fetch-URL fallback
}

#[cfg(unix)]
#[test]
fn recording_push_only_placeholder_rewrite_cannot_change_the_reviewed_receiver()
-> Result<(), String> {
    let f = Fixture::new()?;
    let intended = f
        .root
        .join("uncontacted-A.git")
        .to_string_lossy()
        .into_owned();
    let other = f
        .root
        .join("uncontacted-B.git")
        .to_string_lossy()
        .into_owned();
    f.git(&[
        "config",
        &format!("url.{other}.pushInsteadOf"),
        "gitcraft-preflight-address://",
    ])?;
    let marker = f.root.join("receiver-argv");
    let shim = f.root.join("record-receiver");
    std::fs::write(
        &shim,
        format!(
            "#!/bin/sh\nprintf '%s\\n' \"$1\" > {}\nexit 1\n",
            crate::remote::sh_quote(&marker.to_string_lossy())
        ),
    )
    .map_err(|e| e.to_string())?;
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&shim, std::fs::Permissions::from_mode(0o700))
        .map_err(|e| e.to_string())?;
    for force in [false, true] {
        let head = f.git(&["rev-parse", "HEAD"])?;
        let alias = "gitcraft-fixture-recorded";
        let mut args = vec!["push".into(), "--dry-run".into(), "--no-verify".into()];
        if force {
            args.push(format!("--force-with-lease=refs/heads/main:{head}"));
        }
        args.extend([alias.into(), format!("{head}:refs/heads/main")]);
        let result = git::push_preflight_with(
            &f.repo,
            &args,
            git::PushTarget {
                address: &intended,
                alias,
                original_remote: "origin",
                skip_hooks: true,
                lfs_config: &[],
                lfs_objects: &[],
                receive_pack: shim.to_str().ok_or("shim path")?,
            },
            None,
            None,
        );
        assert!(result.is_err());
        assert_eq!(
            std::fs::read_to_string(&marker)
                .map_err(|e| format!("{e}: {result:?}"))?
                .trim(),
            intended
        );
        assert!(!std::path::Path::new(&intended).exists());
        assert!(!std::path::Path::new(&other).exists());
    }
    Ok(())
}

pub(crate) struct Fixture {
    pub root: PathBuf,
    pub repo: PathBuf,
    pub receiver: PathBuf,
}
impl Fixture {
    pub(crate) fn new() -> Result<Self, String> {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| e.to_string())?
            .as_nanos();
        let root =
            std::env::temp_dir().join(format!("gitcraft-preflight-{}-{stamp}", std::process::id()));
        let repo = root.join("repo");
        let receiver = root.join("receiver.git");
        std::fs::create_dir_all(&repo).map_err(|e| e.to_string())?;
        let f = Self {
            root,
            repo,
            receiver,
        };
        for args in [
            vec!["init", "-q", "-b", "main"],
            vec!["config", "user.name", "Fixture"],
            vec!["config", "user.email", "fixture@work.example"],
            vec!["config", "commit.gpgsign", "false"],
            vec!["config", "core.autocrlf", "false"],
        ] {
            f.git(&args)?;
        }
        f.commit("base.txt", "base\n", "Base")?;
        git::run(
            None,
            &["init", "--bare", "-q", &f.receiver.to_string_lossy()],
        )?;
        f.git(&["remote", "add", "origin", &f.receiver.to_string_lossy()])?;
        f.git(&["push", "-qu", "origin", "main"])?;
        Ok(f)
    }
    pub(crate) fn git(&self, args: &[&str]) -> Result<String, String> {
        git::run(Some(&self.repo), args).map(|s| s.trim_end().to_owned())
    }
    pub(crate) fn commit(&self, path: &str, text: &str, message: &str) -> Result<String, String> {
        std::fs::write(self.repo.join(path), text).map_err(|e| e.to_string())?;
        self.git(&["add", "--", path])?;
        self.git(&["commit", "-qm", message])?;
        self.git(&["rev-parse", "HEAD"])
    }
    pub(crate) fn profile(&self) -> UserProfile {
        UserProfile {
            author_name: "Fixture".into(),
            author_email: "fixture@work.example".into(),
            ..Default::default()
        }
    }
    pub(crate) fn plan(&self, profile: UserProfile) -> Result<Plan, String> {
        scan(&self.repo, vec![vec!["push".into()]], profile, &|| true)
    }
    pub(crate) fn receiver_head(&self) -> Result<String, String> {
        git::run(Some(&self.receiver), &["rev-parse", "refs/heads/main"]).map(|s| s.trim().into())
    }
    pub(crate) fn advance_receiver(&self) -> Result<String, String> {
        let before = self.receiver_head()?;
        let tree = git::run(
            Some(&self.receiver),
            &["rev-parse", &format!("{before}^{{tree}}")],
        )?;
        let new = git::run(
            Some(&self.receiver),
            &[
                "-c",
                "user.name=Other",
                "-c",
                "user.email=other@example.org",
                "commit-tree",
                tree.trim(),
                "-p",
                &before,
                "-m",
                "Remote work",
            ],
        )?;
        git::run(
            Some(&self.receiver),
            &["update-ref", "refs/heads/main", new.trim(), &before],
        )?;
        Ok(new.trim().into())
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

#[test]
fn host_and_namespace_policies_preserve_profile_serialization_and_transport_identity() {
    assert_eq!(
        target_label("file://user:DO_NOT_DISPLAY@host/path?token=DO_NOT_DISPLAY"),
        "Local repository: host/path"
    );
    let mut p = UserProfile {
        author_email: "work@example.org".into(),
        ..Default::default()
    };
    p.author_hosts.push(crate::settings::profiles::HostAuthor {
        host: "github.com".into(),
        name: "Personal".into(),
        email: "personal@example.org".into(),
    });
    p.author_hosts.push(crate::settings::profiles::HostAuthor {
        host: "github.com/company".into(),
        name: "Work".into(),
        email: "company@example.org".into(),
    });
    assert_eq!(
        p.expected_author("git@GitHub.com:personal/repo.git").1,
        "personal@example.org"
    );
    assert_eq!(
        p.expected_author("ssh://git@ssh.github.com:443/personal/repo.git")
            .1,
        "personal@example.org"
    );
    assert_eq!(
        p.expected_author("https://token:withheld@github.com/company/repo.git?token=withheld")
            .1,
        "company@example.org"
    );
    assert_eq!(
        p.expected_author("https://github.com/company-other/repo").1,
        "personal@example.org"
    );
    assert_eq!(
        p.expected_author("https://github.com:443/company/repo").1,
        "company@example.org"
    );
    let json = serde_json::to_string(&p).unwrap_or_default();
    assert_eq!(serde_json::from_str::<UserProfile>(&json).ok(), Some(p));
}

#[test]
fn replacement_refs_cannot_hide_physical_outgoing_content_or_repair_inputs() -> Result<(), String> {
    let f = Fixture::new()?;
    let bad = f.commit(
        "token.txt",
        "api_key = 'ghp_DUMMY_REPLACEMENT_CANARY_123456789'\n",
        "WIP physical",
    )?;
    let parent = f.git(&["rev-parse", "HEAD^"])?;
    let safe_tree = f.git(&["rev-parse", &format!("{parent}^{{tree}}")])?;
    let safe = f.git(&[
        "commit-tree",
        &safe_tree,
        "-p",
        &parent,
        "-m",
        "sanitized replacement",
    ])?;
    f.git(&["replace", &bad, &safe])?;
    assert_eq!(
        f.git(&["show", "-s", "--format=%s", &bad])?,
        "sanitized replacement"
    );
    let plan = f.plan(f.profile())?;
    assert_eq!(plan.head, bad);
    assert!(
        plan.commits
            .iter()
            .any(|c| c.oid == bad && c.subject == "WIP physical")
    );
    assert!(
        plan.findings
            .iter()
            .any(|v| v.kind == Kind::Secret && v.path == "token.txt")
    );
    assert!(plan.findings.iter().any(|v| v.kind == Kind::Message));
    assert!(read(&f.repo, &["cat-file", "commit", &bad], &|| true)?.contains("WIP physical"));
    let out = git::local_repair_input(&f.repo, &["show", "-s", "--format=%s", &bad], &[], &[])?;
    assert!(out.status.success());
    assert_eq!(out.stdout, b"WIP physical\n");
    Ok(())
}

#[test]
fn six_categories_cover_actual_outgoing_history_and_never_return_secret_values()
-> Result<(), String> {
    let f = Fixture::new()?;
    f.advance_receiver()?;
    f.git(&["fetch", "-q", "origin"])?;
    let secret = "ghp_THIS_IS_A_DUMMY_FIXTURE_TOKEN_123456789";
    f.commit(
        "token.txt",
        &format!("api_key = '{secret}'\n"),
        "WIP: unfinished",
    )?;
    std::fs::write(f.repo.join("large.bin"), vec![b'x'; 6 * 1024 * 1024])
        .map_err(|e| e.to_string())?;
    f.git(&["add", "large.bin"])?;
    f.git(&["commit", "-qm", "Large asset"])?;
    let mut profile = f.profile();
    profile.author_email = "personal@example.org".into();
    profile.own_signing = true;
    profile.signing.sign_commits = true;
    let before = f.git(&["status", "--porcelain=v1"])?;
    let refs = f.git(&["show-ref"])?;
    let head = f.receiver_head()?;
    let plan = f.plan(profile)?;
    for kind in [
        Kind::Identity,
        Kind::Signature,
        Kind::Message,
        Kind::Secret,
        Kind::Large,
        Kind::RemoteHistory,
    ] {
        assert!(
            plan.findings.iter().any(|i| i.kind == kind),
            "missing {kind:?}"
        );
    }
    assert!(
        plan.findings
            .iter()
            .any(|i| i.kind == Kind::RemoteHistory && i.detail.contains("1 by other authors"))
    );
    assert!(plan.findings.iter().all(|i| !i.detail.contains(secret)));
    assert!(push_args(&plan).is_err());
    let mut force = plan.clone();
    force.force = true;
    assert!(
        push_args(&force)?
            .iter()
            .any(|a| a.starts_with("--force-with-lease="))
    );
    assert_eq!(f.git(&["status", "--porcelain=v1"])?, before);
    assert_eq!(f.git(&["show-ref"])?, refs);
    assert_eq!(f.receiver_head()?, head);
    Ok(())
}

#[test]
fn secret_scan_streams_large_and_binary_objects_without_match_output() -> Result<(), String> {
    let f = Fixture::new()?;
    let token = "github_pat_DUMMY_FIXTURE_VALUE_1234567890";
    let mut bytes = vec![b' '; 3 * 1024 * 1024];
    bytes.extend_from_slice(format!("\naccess_token={token}\n").as_bytes());
    bytes.push(0);
    std::fs::write(f.repo.join("binary.dat"), bytes).map_err(|e| e.to_string())?;
    f.git(&["add", "binary.dat"])?;
    f.git(&["commit", "-qm", "Payload"])?;
    let plan = f.plan(f.profile())?;
    assert!(
        plan.findings.iter().any(|x| x.kind == Kind::Secret
            && x.path == "binary.dat"
            && x.detail.contains("line 2"))
    );
    assert!(!format!("{:?}", plan.findings).contains(token));
    Ok(())
}

#[test]
fn approved_push_is_pinned_and_refuses_new_server_tip_or_preexisting_unfetched_target()
-> Result<(), String> {
    let f = Fixture::new()?;
    f.commit("next.txt", "next\n", "Next")?;
    let plan = f.plan(f.profile())?;
    assert!(plan.targets[0].contains(f.receiver.to_string_lossy().as_ref()));
    let remote = f.advance_receiver()?;
    assert!(execute(&f.repo, &plan, None).is_err());
    assert_eq!(f.receiver_head()?, remote);
    f.git(&["fetch", "-q", "origin"])?;
    f.git(&["update-ref", "-d", "refs/remotes/origin/main"])?;
    let unknown = f.plan(f.profile())?;
    assert!(unknown.base.is_none());
    assert!(execute(&f.repo, &unknown, None).is_err());
    assert_eq!(f.receiver_head()?, remote);
    Ok(())
}

#[test]
fn stale_branch_checkout_identity_and_remote_configuration_all_refuse_before_sending()
-> Result<(), String> {
    let f = Fixture::new()?;
    f.commit("next", "next", "Next")?;
    let plan = f.plan(f.profile())?;
    let remote = f.receiver_head()?;
    f.git(&["checkout", "-qb", "other"])?;
    assert!(execute(&f.repo, &plan, None).is_err());
    f.git(&["checkout", "-q", "main"])?;
    f.git(&["config", "user.email", "changed@example.org"])?;
    assert!(execute(&f.repo, &plan, None).is_err());
    f.git(&["config", "user.email", "fixture@work.example"])?;
    let other = f.root.join("other.git");
    git::run(None, &["init", "--bare", "-q", &other.to_string_lossy()])?;
    f.git(&[
        "remote",
        "set-url",
        "--push",
        "origin",
        &other.to_string_lossy(),
    ])?;
    assert!(execute(&f.repo, &plan, None).is_err());
    f.git(&["config", "--unset", "remote.origin.pushurl"])?;
    f.commit("later", "later", "Later")?;
    assert!(execute(&f.repo, &plan, None).is_err());
    assert_eq!(f.receiver_head()?, remote);
    Ok(())
}

#[test]
fn push_pins_rewritten_url_without_putting_it_in_argv_and_never_follows_tags() -> Result<(), String>
{
    let f = Fixture::new()?;
    f.commit("next", "next", "Next")?;
    let other = f.root.join("wrong.git");
    git::run(None, &["init", "--bare", "-q", &other.to_string_lossy()])?;
    f.git(&[
        "config",
        &format!("url.{}.insteadOf", other.display()),
        "gitcraft-preflight-address://",
    ])?;
    let plan = f.plan(f.profile())?;
    let head = plan.head.clone();
    execute(&f.repo, &plan, None)?;
    assert_eq!(f.receiver_head()?, head);
    assert!(git::resolve(&other, "refs/heads/main").is_err());
    let args = push_args(&plan)?;
    assert!(args.contains(&"--no-follow-tags".into()));
    assert!(args.contains(&"--recurse-submodules=no".into()));
    assert!(!args.iter().any(|a| a.contains("withheld")));
    Ok(())
}

#[test]
fn author_and_secret_repairs_rewrite_only_outgoing_history_and_refuse_dirty_state()
-> Result<(), String> {
    let f = Fixture::new()?;
    let base = f.git(&["rev-parse", "HEAD"])?;
    f.commit(
        "private.txt",
        "password=fixture_dummy_value_123\n",
        "New message",
    )?;
    let mut p = f.profile();
    p.author_email = "personal@example.org".into();
    let plan = f.plan(p)?;
    std::fs::write(f.repo.join("untracked"), "keep").map_err(|e| e.to_string())?;
    assert!(fix(&f.repo, &plan, "author").is_err());
    std::fs::remove_file(f.repo.join("untracked")).map_err(|e| e.to_string())?;
    fix(&f.repo, &plan, "author")?;
    assert_eq!(
        f.git(&["show", "-s", "--format=%ae", "HEAD"])?,
        "personal@example.org"
    );
    assert_eq!(f.git(&["rev-parse", "HEAD^"])?, base);
    let mut profile = f.profile();
    profile.author_email = "personal@example.org".into();
    let secret = f.plan(profile)?;
    fix(&f.repo, &secret, "secrets")?;
    assert!(!f.repo.join("private.txt").exists());
    assert!(
        f.plan(secret.profile)?
            .findings
            .iter()
            .all(|x| x.kind != Kind::Secret)
    );
    Ok(())
}

#[test]
fn local_lfs_repair_rewrites_pointers_attributes_and_keeps_source_index_consistent()
-> Result<(), String> {
    let f = Fixture::new()?;
    std::fs::write(
        f.repo.join("large [asset].bin"),
        vec![b'x'; 6 * 1024 * 1024],
    )
    .map_err(|e| e.to_string())?;
    f.git(&["add", "large [asset].bin"])?;
    f.git(&["commit", "-qm", "Asset"])?;
    let plan = f.plan(f.profile())?;
    fix(&f.repo, &plan, "lfs")?;
    let pointer = f.git(&["show", "HEAD:large [asset].bin"])?;
    assert!(git::parse_lfs_pointer(&pointer).is_some());
    let attrs = f.git(&[
        "check-attr",
        "--cached",
        "filter",
        "--",
        "large [asset].bin",
    ])?;
    assert!(attrs.ends_with(": lfs"), "{attrs}");
    assert!(f.git(&["status", "--porcelain=v1"])?.is_empty());
    assert!(
        f.plan(f.profile())?
            .findings
            .iter()
            .all(|x| x.kind != Kind::Large)
    );
    Ok(())
}

#[test]
fn remote_credentials_and_query_values_are_not_in_checklist_targets() -> Result<(), String> {
    let f = Fixture::new()?;
    f.commit("next", "next", "Next")?;
    f.git(&[
        "remote",
        "set-url",
        "--push",
        "origin",
        "https://credential_user:DO_NOT_DISPLAY@github.com/personal/repo.git?token=DO_NOT_DISPLAY",
    ])?;
    let mut profile = f.profile();
    profile
        .author_hosts
        .push(crate::settings::profiles::HostAuthor {
            host: "github.com/personal".into(),
            name: "Personal".into(),
            email: "personal@example.org".into(),
        });
    let plan = f.plan(profile)?;
    assert_eq!(plan.expected_email, "personal@example.org");
    assert!(plan.findings.iter().any(|i| i.kind == Kind::Identity));
    assert_eq!(plan.targets, ["github.com/personal/repo"]);
    assert!(
        plan.findings
            .iter()
            .all(|i| !i.detail.contains("DO_NOT_DISPLAY"))
    );
    Ok(())
}

#[cfg(unix)]
#[test]
fn unsigned_repair_uses_only_a_fixture_signing_key_and_then_passes_the_presence_check()
-> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    let f = Fixture::new()?;
    f.commit("next", "next", "Next")?;
    let key = f.root.join("fixture-key");
    let out = std::process::Command::new("ssh-keygen")
        .args([
            "-q",
            "-t",
            "ed25519",
            "-N",
            "",
            "-f",
            &key.to_string_lossy(),
        ])
        .env_remove("SSH_AUTH_SOCK")
        .output()
        .map_err(|e| e.to_string())?;
    assert!(out.status.success());
    let wrapper = f.root.join("fixture-signer");
    std::fs::write(
        &wrapper,
        "#!/bin/sh\nunset SSH_AUTH_SOCK\nexec ssh-keygen \"$@\"\n",
    )
    .map_err(|e| e.to_string())?;
    std::fs::set_permissions(&wrapper, std::fs::Permissions::from_mode(0o700))
        .map_err(|e| e.to_string())?;
    f.git(&["config", "gpg.format", "ssh"])?;
    f.git(&["config", "user.signingkey", &key.to_string_lossy()])?;
    f.git(&["config", "gpg.ssh.program", &wrapper.to_string_lossy()])?;
    let mut profile = f.profile();
    profile.own_signing = true;
    profile.signing.sign_commits = true;
    let plan = f.plan(profile.clone())?;
    assert!(plan.findings.iter().any(|i| i.kind == Kind::Signature));
    fix(&f.repo, &plan, "sign")?;
    let after = f.plan(profile)?;
    assert!(after.commits.iter().all(|c| c.signed));
    assert!(!after.findings.iter().any(|i| i.kind == Kind::Signature));
    Ok(())
}

#[cfg(unix)]
#[test]
fn missing_promisor_blob_does_not_trigger_a_lazy_fetch_or_credential_helper() -> Result<(), String>
{
    use std::os::unix::fs::PermissionsExt;
    let f = Fixture::new()?;
    f.commit("missing.txt", "payload", "Missing")?;
    let blob = f.git(&["rev-parse", "HEAD:missing.txt"])?;
    let marker = f.root.join("upload-pack-ran");
    let program = f.root.join("upload-pack");
    std::fs::write(
        &program,
        format!(
            "#!/bin/sh\ntouch {}\nexit 1\n",
            crate::remote::sh_quote(&marker.to_string_lossy())
        ),
    )
    .map_err(|e| e.to_string())?;
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o700))
        .map_err(|e| e.to_string())?;
    f.git(&["config", "extensions.partialClone", "origin"])?;
    f.git(&["config", "remote.origin.promisor", "true"])?;
    f.git(&[
        "config",
        "remote.origin.uploadpack",
        &program.to_string_lossy(),
    ])?;
    std::fs::remove_file(
        f.repo
            .join(".git/objects")
            .join(&blob[..2])
            .join(&blob[2..]),
    )
    .map_err(|e| e.to_string())?;
    assert!(f.plan(f.profile()).is_err());
    assert!(!marker.exists());
    Ok(())
}

#[cfg(unix)]
#[test]
fn frozen_transport_preserves_the_original_remote_name_and_hook_configuration() -> Result<(), String>
{
    use std::os::unix::fs::PermissionsExt;
    let f = Fixture::new()?;
    f.commit("next", "next", "Next")?;
    let hooks = f.root.join("custom-hooks");
    std::fs::create_dir(&hooks).map_err(|e| e.to_string())?;
    let output = f.root.join("hook-arguments");
    let script = format!(
        "#!/bin/sh\nprintf '%s\\n' \"$1\" \"$2\" > {}\ngit config --get core.hooksPath >> {}\ncat >/dev/null\n",
        crate::remote::sh_quote(&output.to_string_lossy()),
        crate::remote::sh_quote(&output.to_string_lossy())
    );
    let hook = hooks.join("pre-push");
    std::fs::write(&hook, script).map_err(|e| e.to_string())?;
    std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o700))
        .map_err(|e| e.to_string())?;
    f.git(&["config", "core.hooksPath", &hooks.to_string_lossy()])?;
    let plan = f.plan(f.profile())?;
    execute(&f.repo, &plan, None)?;
    let arguments = std::fs::read_to_string(output).map_err(|e| e.to_string())?;
    let lines: Vec<_> = arguments.lines().collect();
    assert_eq!(lines.first(), Some(&"origin"));
    assert_eq!(
        lines.get(1).copied(),
        Some(f.receiver.to_string_lossy().as_ref())
    );
    assert_eq!(
        lines.get(2).copied(),
        Some(hooks.to_string_lossy().as_ref())
    );
    Ok(())
}

#[test]
fn repaired_lfs_objects_are_uploaded_with_the_confirmed_push_even_without_a_hook()
-> Result<(), String> {
    let f = Fixture::new()?;
    std::fs::write(f.repo.join("asset.bin"), vec![b'x'; 6 * 1024 * 1024])
        .map_err(|e| e.to_string())?;
    f.git(&["add", "asset.bin"])?;
    f.git(&["commit", "-qm", "Asset"])?;
    let plan = f.plan(f.profile())?;
    fix(&f.repo, &plan, "lfs")?;
    let plan = f.plan(f.profile())?;
    assert_eq!(plan.lfs_objects.len(), 1);
    execute(&f.repo, &plan, None)?;
    let oid = plan.lfs_objects.first().ok_or("No LFS object")?;
    let stored = f
        .receiver
        .join("lfs/objects")
        .join(&oid[..2])
        .join(&oid[2..4])
        .join(oid);
    assert!(stored.is_file(), "{}", stored.display());
    assert_eq!(f.receiver_head()?, plan.head);
    Ok(())
}

#[test]
fn a_stale_server_lease_does_not_upload_lfs_payloads() -> Result<(), String> {
    let f = Fixture::new()?;
    f.git(&["lfs", "install", "--local"])?;
    std::fs::write(f.repo.join("asset.bin"), vec![b'x'; 6 * 1024 * 1024])
        .map_err(|e| e.to_string())?;
    f.git(&["add", "asset.bin"])?;
    f.git(&["commit", "-qm", "Asset"])?;
    fix(&f.repo, &f.plan(f.profile())?, "lfs")?;
    let plan = f.plan(f.profile())?;
    let advanced = f.advance_receiver()?;
    assert!(execute(&f.repo, &plan, None).is_err());
    assert_eq!(f.receiver_head()?, advanced);
    assert!(!f.receiver.join("lfs/objects").exists());
    Ok(())
}

#[test]
fn gerrit_review_and_annotated_tag_pushes_keep_the_inspected_objects_and_namespaces()
-> Result<(), String> {
    let f = Fixture::new()?;
    let base = f.receiver_head()?;
    let id = "I0123456789abcdef0123456789abcdef01234567";
    let head = f.commit("review", "review", &format!("Review\n\nChange-Id: {id}"))?;
    let review = crate::gerrit::review_ops("origin", "main", "", &head, &base)?;
    let plan = scan(&f.repo, review, f.profile(), &|| true)?;
    assert_eq!(plan.destination, "refs/for/main");
    execute(&f.repo, &plan, None)?;
    assert_eq!(git::resolve(&f.receiver, "refs/for/main")?, head);
    f.git(&["tag", "-a", "-m", "Release annotation", "release"])?;
    let oid = f.git(&["rev-parse", "refs/tags/release"])?;
    let plan = scan(
        &f.repo,
        vec![vec![
            "push".into(),
            "origin".into(),
            "refs/tags/release".into(),
        ]],
        f.profile(),
        &|| true,
    )?;
    execute(&f.repo, &plan, None)?;
    assert_eq!(
        git::run(Some(&f.receiver), &["rev-parse", "refs/tags/release"])?.trim(),
        oid
    );
    assert_eq!(
        git::run(Some(&f.receiver), &["cat-file", "-t", "refs/tags/release"])?.trim(),
        "tag"
    );
    Ok(())
}

#[test]
fn lfs_endpoint_working_changes_invalidate_approval_before_any_network_step() -> Result<(), String>
{
    let f = Fixture::new()?;
    f.commit(
        ".lfsconfig",
        "[lfs]\nurl = https://lfs.example.org/work\n",
        "LFS endpoint",
    )?;
    let plan = f.plan(f.profile())?;
    let before = f.receiver_head()?;
    std::fs::write(
        f.repo.join(".lfsconfig"),
        "[lfs]\nurl = https://wrong.example.org/personal\n",
    )
    .map_err(|e| e.to_string())?;
    assert!(check(&f.repo, &plan).is_err());
    assert_eq!(f.receiver_head()?, before);
    Ok(())
}

#[test]
fn lfs_upload_works_with_the_existing_local_lfs_hook_too() -> Result<(), String> {
    let f = Fixture::new()?;
    f.git(&["lfs", "install", "--local"])?;
    std::fs::write(f.repo.join("asset.bin"), vec![b'x'; 6 * 1024 * 1024])
        .map_err(|e| e.to_string())?;
    f.git(&["add", "asset.bin"])?;
    f.git(&["commit", "-qm", "Asset"])?;
    let plan = f.plan(f.profile())?;
    fix(&f.repo, &plan, "lfs")?;
    let plan = f.plan(f.profile())?;
    execute(&f.repo, &plan, None)?;
    assert_eq!(f.receiver_head()?, plan.head);
    Ok(())
}

#[cfg(unix)]
#[test]
fn rejecting_pre_push_hook_preserves_receiver_refs_and_lfs_objects() -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    let f = Fixture::new()?;
    std::fs::write(f.repo.join("asset.bin"), vec![b'x'; 6 * 1024 * 1024])
        .map_err(|e| e.to_string())?;
    f.git(&["add", "asset.bin"])?;
    f.git(&["commit", "-qm", "Asset"])?;
    fix(&f.repo, &f.plan(f.profile())?, "lfs")?;
    let marker = f.root.join("hook-ran");
    let hook = f.repo.join(".git/hooks/pre-push");
    std::fs::write(
        &hook,
        format!(
            "#!/bin/sh\nprintf 'rejected\n' > {}\nexit 1\n",
            crate::remote::sh_quote(&marker.to_string_lossy())
        ),
    )
    .map_err(|e| e.to_string())?;
    std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o700))
        .map_err(|e| e.to_string())?;
    let plan = f.plan(f.profile())?;
    let before = f.receiver_head()?;
    let oid = plan.lfs_objects.iter().next().ok_or("No LFS object")?;
    let stored = f
        .receiver
        .join("lfs/objects")
        .join(&oid[..2])
        .join(&oid[2..4])
        .join(oid);
    assert!(
        !stored.exists(),
        "The receiver starts without this LFS object"
    );
    assert!(execute(&f.repo, &plan, None).is_err());
    assert!(marker.exists(), "The original hook must run");
    assert_eq!(f.receiver_head()?, before);
    assert!(
        !stored.exists(),
        "A hook veto must precede explicit LFS upload"
    );
    Ok(())
}

#[test]
fn explicit_annotated_tag_push_works_with_detached_head() -> Result<(), String> {
    let f = Fixture::new()?;
    f.git(&["tag", "-a", "-m", "Release", "release"])?;
    let tag = f.git(&["rev-parse", "refs/tags/release"])?;
    f.git(&["checkout", "-q", "--detach"])?;
    let checkout = f.git(&["rev-parse", "HEAD"])?;
    let plan = scan(
        &f.repo,
        vec![vec![
            "push".into(),
            "origin".into(),
            "refs/tags/release".into(),
        ]],
        f.profile(),
        &|| true,
    )?;
    assert!(plan.checkout.is_empty());
    assert_eq!(plan.checkout_oid, checkout);
    assert_eq!(plan.push_oid, tag);
    execute(&f.repo, &plan, None)?;
    assert_eq!(
        git::run(Some(&f.receiver), &["rev-parse", "refs/tags/release"])?.trim(),
        tag
    );
    assert_eq!(f.git(&["rev-parse", "HEAD"])?, checkout);
    Ok(())
}

#[test]
fn explicit_default_ports_keep_host_and_namespace_identity_policy() -> Result<(), String> {
    let f = Fixture::new()?;
    f.commit("next", "next", "Next")?;
    let mut profile = f.profile();
    profile
        .author_hosts
        .push(crate::settings::profiles::HostAuthor {
            host: "github.com/personal".into(),
            name: "Personal".into(),
            email: "personal@example.org".into(),
        });
    for url in [
        "https://github.com/personal/repo.git",
        "https://github.com:443/personal/repo.git",
    ] {
        f.git(&["remote", "set-url", "origin", url])?;
        let plan = f.plan(profile.clone())?;
        assert_eq!(plan.expected_email, "personal@example.org");
        assert!(
            plan.findings
                .iter()
                .any(|finding| finding.kind == Kind::Identity)
        );
    }
    assert_eq!(
        profile
            .expected_author("https://github.com:8443/personal/repo.git")
            .1,
        profile.author_email
    );
    profile.author_hosts[0].host = "github.com:443/personal".into();
    assert_eq!(
        profile
            .expected_author("https://github.com/personal/repo.git")
            .1,
        "personal@example.org"
    );
    assert_eq!(
        profile
            .expected_author("ssh://git@github.com:443/personal/repo.git")
            .1,
        "personal@example.org"
    );
    assert_eq!(
        profile
            .expected_author("ssh://git@github.com/personal/repo.git")
            .1,
        profile.author_email
    );
    Ok(())
}

#[test]
fn current_porcelain_push_and_skip_hook_policy_keep_lease_and_lfs_guards() -> Result<(), String> {
    let f = Fixture::new()?;
    let tip = f.commit("next", "next", "Next")?;
    let hooks = f.repo.join(".git/hooks/pre-push");
    std::fs::write(&hooks, "#!/bin/sh\nexit 42\n").map_err(|e| e.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&hooks, std::fs::Permissions::from_mode(0o700))
            .map_err(|e| e.to_string())?;
    }
    let plan = scan(
        &f.repo,
        vec![vec![
            "push".into(),
            "--porcelain".into(),
            "--no-verify".into(),
        ]],
        f.profile(),
        &|| true,
    )?;
    assert!(plan.skip_hooks);
    let args = push_args(&plan)?;
    assert!(args.iter().any(|s| s == "--porcelain"));
    assert!(!args.iter().any(|s| s == "--no-verify"));
    assert!(args.iter().any(|s| s.starts_with("--force-with-lease=")));
    execute(&f.repo, &plan, None)?;
    assert_eq!(f.receiver_head()?, tip);
    Ok(())
}

#[test]
fn routed_submodule_checks_survive_preflight_without_enabling_child_publication()
-> Result<(), String> {
    let f = Fixture::new()?;
    let plan = scan(
        &f.repo,
        vec![vec![
            "push".into(),
            "--recurse-submodules=check".into(),
            "origin".into(),
            "HEAD:refs/heads/main".into(),
        ]],
        f.profile(),
        &|| true,
    )?;
    let args = push_args(&plan)?;
    assert_eq!(
        args.iter()
            .filter(|s| s.starts_with("--recurse-submodules="))
            .cloned()
            .collect::<Vec<_>>(),
        vec!["--recurse-submodules=check"]
    );
    assert!(parse(&["--recurse-submodules=on-demand".into()]).is_err());
    execute(&f.repo, &plan, None)?;
    assert_eq!(f.receiver_head()?, plan.head);
    Ok(())
}

/// Supplied metadata only: no Fixture, Git, helper, filesystem or transport.
#[test]
fn merge_pure_reviewed_send_receipt_refuses_changed_exact_push() -> Result<(), String> {
    let oid = "a".repeat(40);
    let plan = Plan {
        branch: "refs/heads/topic".into(),
        remote: "team".into(),
        destination: "refs/heads/topic".into(),
        head: oid.clone(),
        base: None,
        commits: vec![],
        findings: vec![],
        targets: vec![],
        expected_name: String::new(),
        expected_email: String::new(),
        expected_accounts: BTreeMap::new(),
        profile: UserProfile::default(),
        force: false,
        deleting: false,
        tracking: false,
        lfs_objects: BTreeSet::new(),
        skip_hooks: false,
        push_oid: oid,
        original: vec![],
        urls: vec!["https://push.invalid/team/project".into()],
        checkout: String::new(),
        checkout_oid: String::new(),
        config: String::new(),
        config_before_push: String::new(),
        local_refs: BTreeMap::new(),
        extras: vec![],
        tracking_ref: None,
        lfs_config: vec![],
        receive_pack: "git-receive-pack".into(),
    };
    let receipt = ReviewedSend::from_echo(push_args(&plan)?, None);
    receipt.check_plan(&plan)?;
    for field in 0..6 {
        let mut changed = plan.clone();
        match field {
            0 => changed.push_oid = "b".repeat(40),
            1 => changed.destination = "refs/heads/other".into(),
            2 => changed.remote = "different-receiver".into(),
            3 => changed.base = Some("c".repeat(40)),
            4 => changed.force = true, // no inspected lease: refuse even before dispatch
            _ => changed.extras.push("--recurse-submodules=check".into()),
        }
        assert!(
            receipt.check_plan(&changed).is_err(),
            "changed field {field}"
        );
    }
    Ok(())
}
