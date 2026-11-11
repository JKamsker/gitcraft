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
